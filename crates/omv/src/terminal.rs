//! The terminal panel: a real shell in a pty, docked at the bottom.
//!
//! The session outlives the panel. Hiding the panel with `<C-j>` leaves the
//! shell running, so reopening it shows the same screen with the same history;
//! only the shell *exiting* — `exit`, or `<C-d>` at an empty prompt — ends the
//! session, and the next `<C-j>` then starts a fresh one.
//!
//! Rule 2 of the architecture holds here: the reader thread does not touch the
//! screen, it posts bytes to the one `AppEvent` channel and `App` feeds them to
//! the parser. That keeps the terminal state single-owned, with no lock between
//! the reader and the renderer.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::event::AppEvent;

/// (rows, cols) a session assumes when it is started before the first frame
/// has had a chance to measure the panel.
pub const DEFAULT_SIZE: (u16, u16) = (12, 80);
/// Lines the emulator keeps above the visible screen, so shrinking the panel
/// and growing it again doesn't throw away output.
const SCROLLBACK: usize = 1_000;

/// Identifies which shell a batch of output came from. A session that has just
/// exited can still have bytes in flight; they must not land on its successor.
pub type SessionId = u64;

/// One shell, its pty, and the screen its output has painted.
pub struct Session {
    pub id: SessionId,
    parser: vt100::Parser,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    size: (u16, u16),
}

impl Session {
    /// Start the user's shell. `cwd` is the project root, so the terminal opens
    /// where the editor is working rather than wherever omv was launched from.
    pub fn spawn(
        id: SessionId,
        cwd: &Path,
        size: (u16, u16),
        events: Sender<AppEvent>,
    ) -> anyhow::Result<Session> {
        let mut command = CommandBuilder::new(shell());
        command.cwd(cwd);
        // Without this the shell inherits omv's own TERM, which may promise
        // capabilities this emulator does not have.
        command.env("TERM", "xterm-256color");
        Session::spawn_command(id, command, size, events)
    }

    fn spawn_command(
        id: SessionId,
        command: CommandBuilder,
        size: (u16, u16),
        events: Sender<AppEvent>,
    ) -> anyhow::Result<Session> {
        let (rows, cols) = (size.0.max(1), size.1.max(1));
        let pair = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let child = pair.slave.spawn_command(command)?;
        // The slave fd must go, or the master never sees EOF when the shell exits.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        std::thread::Builder::new()
            .name(format!("omv-term-{id}"))
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    // A closed pty reports EOF on some platforms and EIO on
                    // others; both mean the shell is gone.
                    let read = match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let event = AppEvent::TerminalOutput {
                        session: id,
                        bytes: buffer[..read].to_vec(),
                    };
                    if events.send(event).is_err() {
                        return; // the app is gone
                    }
                }
                let _ = events.send(AppEvent::TerminalExited { session: id });
            })?;

        Ok(Session {
            id,
            parser: vt100::Parser::new(rows, cols, SCROLLBACK),
            writer,
            master: pair.master,
            child,
            size: (rows, cols),
        })
    }

    /// Feed shell output to the emulator.
    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    /// Send a keypress to the shell. Errors are dropped: a write failing means
    /// the shell has died, which the reader thread is already reporting.
    pub fn send(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    /// Tell both the pty and the emulator how big the panel now is. A no-op
    /// unless the size actually changed, since resizing signals the shell.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = (rows.max(1), cols.max(1));
        if self.size == (rows, cols) {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.parser.screen_mut().set_size(rows, cols);
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Closing the pty would normally hang the shell up anyway, but a child
        // that ignores SIGHUP would be left running and unreachable.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The panel: a session that may or may not exist, and whether it is on screen.
#[derive(Default)]
pub struct Terminal {
    pub session: Option<Session>,
    pub visible: bool,
    next_id: SessionId,
}

impl Terminal {
    /// Show the panel, starting a shell if the last one finished. Returns the
    /// error to put on the status line if the shell could not be started.
    pub fn open(
        &mut self,
        cwd: &Path,
        size: (u16, u16),
        events: &Sender<AppEvent>,
    ) -> Result<(), String> {
        if self.session.is_none() {
            let id = self.next_id;
            self.next_id += 1;
            match Session::spawn(id, cwd, size, events.clone()) {
                Ok(session) => self.session = Some(session),
                Err(e) => return Err(format!("terminal: {e}")),
            }
        }
        self.visible = true;
        Ok(())
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Route output to the session it came from, ignoring anything left over
    /// from a shell that has already exited.
    pub fn process(&mut self, session: SessionId, bytes: &[u8]) {
        if let Some(active) = &mut self.session
            && active.id == session
        {
            active.process(bytes);
        }
    }

    /// The shell finished. Drop it so the next `<C-j>` starts a fresh one, and
    /// report whether this was the session currently on screen.
    pub fn finish(&mut self, session: SessionId) -> bool {
        if self.session.as_ref().is_some_and(|s| s.id == session) {
            self.session = None;
            let was_visible = self.visible;
            self.visible = false;
            return was_visible;
        }
        false
    }
}

/// The shell to run: whatever the user's environment names, else a sane default.
fn shell() -> String {
    if cfg!(windows) {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}

/// Turn a keypress into the bytes a terminal would send for it.
///
/// Returns `None` for keys with no encoding (a bare modifier, a release event),
/// which are simply not forwarded.
pub fn encode_key(key: KeyEvent) -> Option<Vec<u8>> {
    use crossterm::event::KeyEventKind;
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    let bytes = match key.code {
        KeyCode::Char(c) if ctrl => vec![control_byte(c)?],
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        // `\r`, not `\n`: the pty's line discipline turns it into a newline, and
        // shells read a bare `\n` as a continuation rather than a submit.
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::F(n) => function_key(n)?,
        _ => return None,
    };

    // Alt is the ESC prefix, which is how every terminal has spelled Meta since
    // the terminals that had a Meta key stopped being made.
    Some(if alt {
        let mut prefixed = vec![0x1b];
        prefixed.extend(bytes);
        prefixed
    } else {
        bytes
    })
}

/// The C0 control code a `<C-x>` chord produces. `<C-d>` is 4 — end of
/// transmission — which is exactly how a shell is told the input is over.
fn control_byte(c: char) -> Option<u8> {
    match c.to_ascii_lowercase() {
        'a'..='z' => Some(c.to_ascii_lowercase() as u8 - b'a' + 1),
        '@' | ' ' => Some(0),
        '[' => Some(27),
        '\\' => Some(28),
        ']' => Some(29),
        '^' => Some(30),
        '_' | '/' => Some(31),
        '?' => Some(127),
        _ => None,
    }
}

fn function_key(n: u8) -> Option<Vec<u8>> {
    let sequence: &[u8] = match n {
        1 => b"\x1bOP",
        2 => b"\x1bOQ",
        3 => b"\x1bOR",
        4 => b"\x1bOS",
        5 => b"\x1b[15~",
        6 => b"\x1b[17~",
        7 => b"\x1b[18~",
        8 => b"\x1b[19~",
        9 => b"\x1b[20~",
        10 => b"\x1b[21~",
        11 => b"\x1b[23~",
        12 => b"\x1b[24~",
        _ => return None,
    };
    Some(sequence.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn ctrl_d_encodes_as_end_of_transmission() {
        assert_eq!(
            encode_key(key(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(vec![4]),
            "<C-d> must be the byte that ends a shell's input"
        );
        assert_eq!(
            encode_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(vec![3]),
            "<C-c> must interrupt rather than type a letter"
        );
    }

    #[test]
    fn ordinary_keys_encode_the_way_a_terminal_would() {
        assert_eq!(
            encode_key(key(KeyCode::Char('é'), KeyModifiers::NONE)),
            Some("é".as_bytes().to_vec()),
            "non-ascii text must go through as UTF-8"
        );
        assert_eq!(
            encode_key(key(KeyCode::Enter, KeyModifiers::NONE)),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            encode_key(key(KeyCode::Up, KeyModifiers::NONE)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode_key(key(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(vec![0x7f])
        );
        assert_eq!(
            encode_key(key(KeyCode::Char('b'), KeyModifiers::ALT)),
            Some(vec![0x1b, b'b']),
            "alt is the ESC prefix"
        );
    }

    /// Drive a real shell to completion, the way the app does: read events off
    /// the channel and feed them to the parser.
    fn run(argv: &[&str]) -> (String, bool) {
        let (tx, rx) = channel();
        let command =
            CommandBuilder::from_argv(argv.iter().map(OsString::from).collect::<Vec<_>>());
        let mut session =
            Session::spawn_command(7, command, (10, 40), tx).expect("spawning a pty shell");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut exited = false;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(AppEvent::TerminalOutput { session: id, bytes }) => {
                    assert_eq!(id, 7, "output must be tagged with its session");
                    session.process(&bytes);
                }
                Ok(AppEvent::TerminalExited { .. }) => {
                    exited = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        (session.screen().contents(), exited)
    }

    #[test]
    fn a_shell_writes_onto_the_panel_screen() {
        let (contents, _) = run(&["/bin/sh", "-c", "echo omv-terminal-ok"]);
        assert!(
            contents.contains("omv-terminal-ok"),
            "the shell's output must reach the emulator screen: {contents:?}"
        );
    }

    #[test]
    fn a_shell_that_exits_reports_that_it_finished() {
        let (_, exited) = run(&["/bin/sh", "-c", "exit 0"]);
        assert!(
            exited,
            "`exit` must end the session, so the next <C-j> starts a fresh one"
        );
    }

    #[test]
    fn output_from_a_finished_session_never_lands_on_its_successor() {
        let mut panel = Terminal::default();
        let (tx, _rx) = channel();
        panel
            .open(Path::new("."), (10, 40), &tx)
            .expect("starting a shell");
        let first = panel.session.as_ref().unwrap().id;

        assert!(panel.finish(first), "the visible session finished");
        assert!(panel.session.is_none(), "a finished shell must be dropped");

        panel
            .open(Path::new("."), (10, 40), &tx)
            .expect("starting a second shell");
        let second = panel.session.as_ref().unwrap().id;
        assert_ne!(first, second, "reopening must start a fresh session");

        panel.process(first, b"stale output");
        assert_eq!(
            panel.session.as_ref().unwrap().screen().contents(),
            "",
            "bytes in flight from the old shell must be discarded"
        );
    }
}
