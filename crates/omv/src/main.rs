mod app;
mod event;
mod explorer;
mod help;
mod picker;
mod substitute;
mod terminal;
mod theme;
mod ui;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::cursor::SetCursorStyle;
use crossterm::execute;
use omv_config::Config;
use omv_core::{Action, Mode};

use crate::app::{App, Focus};
use crate::event::{AppEvent, spawn_input_thread};

#[derive(Parser, Debug)]
#[command(name = "omv", version, about = "A modal text editor")]
struct Args {
    /// Files to open.
    files: Vec<PathBuf>,

    /// Config file to use instead of ~/.config/omv/config.yaml.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Project root for the explorer, pickers, and language servers.
    #[arg(long)]
    root: Option<PathBuf>,

    /// Print every action a keymap may bind, then exit.
    #[arg(long)]
    list_actions: bool,

    /// Print the resolved keymap for each mode, then exit.
    #[arg(long)]
    list_keys: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.list_actions {
        let mut category = "";
        for (_, name, action_category, description) in Action::ALL {
            if *action_category != category {
                category = action_category;
                println!("\n{category}");
            }
            println!("  {name:<28} {description}");
        }
        return Ok(());
    }

    let config = Config::load(args.config.as_deref()).context("loading config")?;

    if args.list_keys {
        for mode in [Mode::Normal, Mode::Insert, Mode::Visual, Mode::Command] {
            println!("\n[{}]", mode.label().to_lowercase());
            for (keys, action) in config.keymap.describe(mode) {
                println!("  {keys:<14} {}", action.name());
            }
        }
        return Ok(());
    }

    let root = args
        .root
        .or_else(|| {
            args.files
                .first()
                .and_then(|f| f.parent().map(Path::to_path_buf))
        })
        .filter(|p| p.is_dir())
        .unwrap_or(std::env::current_dir()?);

    run(config, root, args.files)
}

fn run(config: Config, root: PathBuf, files: Vec<PathBuf>) -> Result<()> {
    let (tx, rx): (Sender<AppEvent>, Receiver<AppEvent>) = channel();

    // LSP servers get their own channel; a bridge thread folds it into the main one
    // so the render loop only ever waits on a single receiver.
    let (lsp_tx, lsp_rx) = channel::<omv_lsp::Event>();
    {
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("omv-lsp-bridge".into())
            .spawn(move || {
                while let Ok(event) = lsp_rx.recv() {
                    if tx.send(AppEvent::Lsp(event)).is_err() {
                        break;
                    }
                }
            })
            .context("spawning the LSP bridge thread")?;
    }

    let mut app = App::new(config, root, tx.clone(), lsp_tx);
    for file in &files {
        app.open_path(file);
    }
    if files.is_empty() {
        app.explorer.visible = true;
        app.status = "omv — <leader>e explorer · <C-p> files · :q to quit".into();
    }

    spawn_input_thread(tx);

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app, rx);
    ratatui::restore();
    // Leave the terminal cursor the way we found it.
    let _ = execute!(io::stdout(), SetCursorStyle::DefaultUserShape);
    result
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    rx: Receiver<AppEvent>,
) -> Result<()> {
    let mut cursor_style = None;

    while !app.quit {
        // Match the terminal cursor to the mode: a bar while inserting, a block
        // otherwise — the same signal every modal editor gives.
        let wanted = match (app.focus, app.editor.mode) {
            (Focus::Editor, Mode::Insert) => SetCursorStyle::SteadyBar,
            (Focus::Editor, _) => SetCursorStyle::SteadyBlock,
            _ => SetCursorStyle::SteadyBlock,
        };
        let wanted_tag = std::mem::discriminant(&wanted);
        if cursor_style != Some(wanted_tag) {
            let _ = execute!(io::stdout(), wanted);
            cursor_style = Some(wanted_tag);
        }

        // Panels draw their own selection highlight, so the terminal cursor is
        // simply left unset (and therefore hidden) unless the editor has focus.
        terminal.draw(|frame| ui::render(frame, app))?;

        let event = rx.recv().context("event channel closed")?;
        app.handle_event(event);

        // Drain whatever else is already queued before redrawing, so a burst of
        // input (paste, key repeat) costs one frame rather than one frame each.
        loop {
            match rx.try_recv() {
                Ok(event) => app.handle_event(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::PickerKind;
    use omv_core::Action;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::sync::mpsc::channel;

    fn test_app() -> App {
        let (tx, _rx) = channel();
        let (lsp_tx, _lsp_rx) = channel();
        let config = Config::builtin().expect("builtin config");
        App::new(config, std::env::current_dir().unwrap(), tx, lsp_tx)
    }

    fn screen(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| ui::render(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(80)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn find_files_opens_the_picker() {
        let mut app = test_app();
        let effects = app.editor.dispatch(Action::FindFiles, None);
        app.apply_effects_for_test(effects);
        assert!(app.picker.is_some(), "FindFiles must open a picker");
        assert_eq!(app.focus, Focus::Picker);
        let out = screen(&mut app);
        assert!(
            out.contains("Files"),
            "picker must be visible on screen:\n{out}"
        );
    }

    #[test]
    fn explorer_toggles_into_view() {
        let mut app = test_app();
        let effects = app.editor.dispatch(Action::ToggleExplorer, None);
        app.apply_effects_for_test(effects);
        let out = screen(&mut app);
        assert!(out.contains("Explorer"), "explorer must be visible:\n{out}");
    }

    fn press(app: &mut App, code: crossterm::event::KeyCode) {
        app.handle_event(AppEvent::Input(crossterm::event::Event::Key(
            crossterm::event::KeyEvent::from(code),
        )));
    }

    #[test]
    fn show_keys_renders_bindings_with_their_descriptions() {
        let mut app = test_app();
        let effects = app.editor.dispatch(Action::ShowKeys, None);
        app.apply_effects_for_test(effects);
        assert_eq!(app.focus, Focus::Help);

        let out = screen(&mut app);
        assert!(out.contains("Key bindings"), "panel must be framed:\n{out}");
        assert!(out.contains("NORMAL MODE"), "modes must be grouped:\n{out}");
        assert!(out.contains("Motion"), "categories must be grouped:\n{out}");
        assert!(
            out.contains("move_left"),
            "action names must be listed:\n{out}"
        );
        assert!(
            out.contains("Cursor one character left"),
            "descriptions must show:\n{out}"
        );
    }

    #[test]
    fn both_keys_and_bang_keys_open_the_panel() {
        for command in ["keys", "!keys", "map"] {
            let mut app = test_app();
            app.editor.command_line = command.to_string();
            let effects = app.editor.execute_command_line();
            app.apply_effects_for_test(effects);
            assert!(app.help.is_some(), ":{command} must open the key reference");
        }
    }

    #[test]
    fn the_panel_scrolls_and_clamps_at_both_ends() {
        let mut app = test_app();
        let effects = app.editor.dispatch(Action::ShowKeys, None);
        app.apply_effects_for_test(effects);
        screen(&mut app); // establishes text_height, which sizes the scroll page

        press(&mut app, crossterm::event::KeyCode::Char('k'));
        assert_eq!(
            app.help.as_ref().unwrap().scroll,
            0,
            "cannot scroll above the top"
        );

        press(&mut app, crossterm::event::KeyCode::Char('j'));
        assert_eq!(app.help.as_ref().unwrap().scroll, 1);

        press(&mut app, crossterm::event::KeyCode::Char('G'));
        let bottom = app.help.as_ref().unwrap().scroll;
        assert!(bottom > 1, "G must jump to the end");
        press(&mut app, crossterm::event::KeyCode::Char('j'));
        assert_eq!(
            app.help.as_ref().unwrap().scroll,
            bottom,
            "cannot scroll past the end"
        );

        press(&mut app, crossterm::event::KeyCode::Char('g'));
        assert_eq!(app.help.as_ref().unwrap().scroll, 0);
    }

    #[test]
    fn esc_closes_the_panel_and_returns_focus() {
        let mut app = test_app();
        let effects = app.editor.dispatch(Action::ShowKeys, None);
        app.apply_effects_for_test(effects);
        press(&mut app, crossterm::event::KeyCode::Esc);
        assert!(app.help.is_none());
        assert_eq!(app.focus, Focus::Editor);
    }

    #[test]
    fn the_panel_reflects_the_users_own_bindings() {
        use omv_config::config::RawConfig;
        // Built from the resolved keymap, so a user override must show up and a
        // binding they removed with `nop` must not.
        let (tx, _rx) = channel();
        let (lsp_tx, _lsp_rx) = channel();
        let base: RawConfig =
            serde_yaml_ng::from_str(include_str!("../../omv-config/assets/default.yaml")).unwrap();
        let user: RawConfig =
            serde_yaml_ng::from_str("keys:\n  normal:\n    zz: join_lines\n    x: nop\n").unwrap();
        let config = omv_config::Config::merge_for_test(base, Some(user));
        let mut app = App::new(config, std::env::current_dir().unwrap(), tx, lsp_tx);

        let effects = app.editor.dispatch(Action::ShowKeys, None);
        app.apply_effects_for_test(effects);
        // Scope to the NORMAL section: visual mode binds `x` too, and only the
        // normal-mode one was removed.
        use crate::help::HelpRow;
        let help = app.help.as_ref().unwrap();
        let mut in_normal = false;
        let mut normal: Vec<String> = Vec::new();
        for row in &help.rows {
            match row {
                HelpRow::Mode(label) => in_normal = *label == "NORMAL",
                HelpRow::Binding { keys, action, .. } if in_normal => {
                    normal.push(format!("{keys}={action}"))
                }
                _ => {}
            }
        }
        assert!(
            normal.contains(&"zz=join_lines".to_string()),
            "user binding missing"
        );
        assert!(
            !normal.iter().any(|b| b.starts_with("x=")),
            "a binding removed with `nop` must not be listed: {normal:?}"
        );
    }

    fn press_ctrl(app: &mut App, c: char) {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        app.handle_event(AppEvent::Input(crossterm::event::Event::Key(
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL),
        )));
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, crossterm::event::KeyCode::Char(c));
        }
    }

    /// An app holding one scratch buffer of `text`, cursor at the top.
    fn app_with_text(text: &str) -> App {
        let mut app = test_app();
        app.editor.buffer_mut().insert(0, text);
        app.editor.buffer_mut().cursor = 0;
        app
    }

    #[test]
    fn ctrl_f_opens_the_find_prompt_through_the_default_keymap() {
        let mut app = app_with_text("foo bar\n");
        press_ctrl(&mut app, 'f');
        assert_eq!(app.focus, Focus::Substitute, "<C-f> must open the prompt");
        let out = screen(&mut app);
        assert!(
            out.contains("Find & Replace"),
            "prompt must be framed:\n{out}"
        );
        assert!(
            out.contains("find"),
            "the find field must be labelled:\n{out}"
        );
        assert!(
            !out.contains("with"),
            "the replacement field stays hidden until <C-s>:\n{out}"
        );
    }

    #[test]
    fn typing_a_pattern_counts_and_jumps_to_the_matches() {
        let mut app = app_with_text("alpha\nbeta\nalpha\n");
        press_ctrl(&mut app, 'f');
        type_text(&mut app, "alpha");
        assert_eq!(app.match_ranges.len(), 2, "both matches must be found");
        assert_eq!(app.editor.buffer().cursor, 0, "preview lands on the first");

        press_ctrl(&mut app, 'n');
        assert_eq!(app.editor.buffer().cursor, 11, "<C-n> walks to the next");
        let out = screen(&mut app);
        assert!(out.contains("2/2"), "the prompt must count matches:\n{out}");
    }

    #[test]
    fn ctrl_s_reveals_the_replacement_field_and_routes_typing_there() {
        let mut app = app_with_text("foo\n");
        press_ctrl(&mut app, 'f');
        type_text(&mut app, "foo");
        press_ctrl(&mut app, 's');
        type_text(&mut app, "bar");

        let panel = app.substitute.as_ref().expect("prompt open");
        assert_eq!(panel.find, "foo", "the pattern must not absorb the reply");
        assert_eq!(panel.replace, "bar");
        let out = screen(&mut app);
        assert!(out.contains("with"), "the second field must appear:\n{out}");
        assert!(out.contains("bar"), "and hold what was typed:\n{out}");
    }

    #[test]
    fn enter_replaces_one_match_and_ctrl_a_replaces_the_rest() {
        let mut app = app_with_text("foo\nfoo\nfoo\n");
        press_ctrl(&mut app, 'f');
        type_text(&mut app, "foo");
        press_ctrl(&mut app, 's');
        type_text(&mut app, "bar");

        press(&mut app, crossterm::event::KeyCode::Enter);
        assert_eq!(
            app.editor.buffer().rope.to_string(),
            "bar\nfoo\nfoo\n",
            "<CR> must replace exactly one match"
        );

        press_ctrl(&mut app, 'a');
        assert_eq!(
            app.editor.buffer().rope.to_string(),
            "bar\nbar\nbar\n",
            "<C-a> must replace every remaining match"
        );
        assert!(
            app.status.contains("replaced 2"),
            "the count must reach the status line: {}",
            app.status
        );
    }

    #[test]
    fn enter_only_navigates_until_a_replacement_has_been_offered() {
        let mut app = app_with_text("foo\nfoo\n");
        press_ctrl(&mut app, 'f');
        type_text(&mut app, "foo");
        press(&mut app, crossterm::event::KeyCode::Enter);
        assert_eq!(
            app.editor.buffer().rope.to_string(),
            "foo\nfoo\n",
            "<CR> before <C-s> must not delete the match it found"
        );
        assert_eq!(app.editor.buffer().cursor, 4, "it walks to the next match");
    }

    #[test]
    fn esc_closes_the_prompt_and_restores_the_cursor() {
        let mut app = app_with_text("one\ntwo\nthree\n");
        app.editor.buffer_mut().cursor = 4; // on `two`
        press_ctrl(&mut app, 'f');
        type_text(&mut app, "three");
        assert_eq!(app.editor.buffer().cursor, 8, "preview jumped to the match");

        press(&mut app, crossterm::event::KeyCode::Esc);
        assert!(app.substitute.is_none());
        assert_eq!(app.focus, Focus::Editor);
        assert!(app.match_ranges.is_empty(), "highlights must be dropped");
        assert_eq!(
            app.editor.buffer().cursor,
            4,
            "an abandoned search must put the cursor back"
        );
    }

    #[test]
    fn esc_after_a_replacement_leaves_the_cursor_on_the_edit() {
        let mut app = app_with_text("one\ntwo\nthree\n");
        app.editor.buffer_mut().cursor = 0;
        press_ctrl(&mut app, 'f');
        type_text(&mut app, "three");
        press_ctrl(&mut app, 's');
        type_text(&mut app, "3");
        press(&mut app, crossterm::event::KeyCode::Enter);
        press(&mut app, crossterm::event::KeyCode::Esc);
        assert_eq!(app.editor.buffer().rope.to_string(), "one\ntwo\n3\n");
        assert_eq!(
            app.editor.buffer().cursor_position().line,
            2,
            "the cursor must stay where the edit happened, not snap back"
        );
    }

    #[test]
    fn hover_text_renders_in_a_popup() {
        let mut app = test_app();
        app.handle_event(AppEvent::Lsp(omv_lsp::Event::Hover {
            text: "fn word_counts(text: &str) -> HashMap<String, usize>".into(),
        }));
        let out = screen(&mut app);
        assert!(out.contains("Hover"), "hover popup must be framed:\n{out}");
        assert!(
            out.contains("word_counts"),
            "hover text must be shown:\n{out}"
        );
    }

    #[test]
    fn a_keypress_dismisses_the_hover_popup() {
        let mut app = test_app();
        app.handle_event(AppEvent::Lsp(omv_lsp::Event::Hover {
            text: "docs".into(),
        }));
        assert!(app.hover.is_some());
        app.handle_event(AppEvent::Input(crossterm::event::Event::Key(
            crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('j')),
        )));
        assert!(app.hover.is_none(), "hover must clear on the next keypress");
    }

    #[test]
    fn diagnostics_reach_the_panel_and_the_status_line() {
        use omv_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
        let mut app = test_app();
        let path = std::env::current_dir().unwrap().join("scratch.rs");
        app.editor.buffer_mut().path = Some(path.clone());
        app.handle_event(AppEvent::Lsp(omv_lsp::Event::Diagnostics {
            path,
            diagnostics: vec![Diagnostic {
                range: Range::new(Position::new(0, 0), Position::new(0, 3)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "mismatched types".into(),
                ..Default::default()
            }],
        }));
        let effects = app.editor.dispatch(Action::ToggleDiagnostics, None);
        app.apply_effects_for_test(effects);

        let out = screen(&mut app);
        assert!(
            out.contains("mismatched types"),
            "diagnostic must be listed:\n{out}"
        );
        assert!(out.contains("E1"), "status line must count errors:\n{out}");
    }

    // ---- terminal panel ----------------------------------------------------

    /// Like `test_app`, but keeps the receiving end so a test can pump the
    /// event loop by hand.
    fn test_app_with_events() -> (App, Receiver<AppEvent>) {
        let (tx, rx) = channel();
        let (lsp_tx, _lsp_rx) = channel();
        let config = Config::builtin().expect("builtin config");
        let app = App::new(config, std::env::current_dir().unwrap(), tx, lsp_tx);
        (app, rx)
    }

    /// Deliver whatever the shell has said, up to `deadline`, stopping early
    /// once `done` is satisfied.
    fn pump(app: &mut App, rx: &Receiver<AppEvent>, done: impl Fn(&App) -> bool) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if done(app) {
                return true;
            }
            match rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(event) => app.handle_event(event),
                Err(_) => continue,
            }
        }
        done(app)
    }

    #[test]
    fn ctrl_j_opens_a_terminal_docked_at_the_bottom() {
        let mut app = app_with_text("alpha\n");
        press_ctrl(&mut app, 'j');
        assert!(app.terminal.visible, "<C-j> must open the panel");
        assert_eq!(app.focus, Focus::Terminal, "and give it the keyboard");

        let out = screen(&mut app);
        let rows: Vec<&str> = out.lines().collect();
        let framed = rows
            .iter()
            .position(|row| row.contains("Terminal"))
            .unwrap_or_else(|| panic!("the panel must be framed:\n{out}"));
        assert!(
            framed > rows.len() / 2,
            "and drawn at the bottom, not the top:\n{out}"
        );
        assert!(
            rows[0].contains("alpha"),
            "the buffer must still be visible above it:\n{out}"
        );
    }

    #[test]
    fn the_term_command_toggles_the_same_panel() {
        let mut app = app_with_text("alpha\n");
        app.editor.command_line = "term".into();
        let effects = app.editor.execute_command_line();
        app.apply_effects_for_test(effects);
        assert!(app.terminal.visible, ":term must open the panel too");
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn ctrl_j_again_hides_the_panel_but_keeps_the_shell_running() {
        let mut app = app_with_text("alpha\n");
        press_ctrl(&mut app, 'j');
        let session = app.terminal.session.as_ref().expect("a shell started").id;

        press_ctrl(&mut app, 'j');
        assert!(!app.terminal.visible, "<C-j> must close the panel");
        assert_eq!(app.focus, Focus::Editor, "and hand the keyboard back");
        assert!(
            app.terminal.session.is_some(),
            "closing the panel must not kill the shell"
        );
        assert!(
            !screen(&mut app).contains("Terminal"),
            "and it must be off the screen"
        );

        press_ctrl(&mut app, 'j');
        assert_eq!(
            app.terminal.session.as_ref().map(|s| s.id),
            Some(session),
            "reopening must show the same session, history and all"
        );
    }

    #[test]
    fn a_shell_that_exits_is_replaced_by_a_fresh_one() {
        let mut app = app_with_text("alpha\n");
        press_ctrl(&mut app, 'j');
        let first = app.terminal.session.as_ref().expect("a shell started").id;

        // What the reader thread posts when the shell has run `exit` or seen <C-d>.
        app.handle_event(AppEvent::TerminalExited { session: first });
        assert!(app.terminal.session.is_none(), "the session is over");
        assert!(!app.terminal.visible, "so the panel goes away with it");
        assert_eq!(app.focus, Focus::Editor);

        press_ctrl(&mut app, 'j');
        let second = app.terminal.session.as_ref().expect("a new shell").id;
        assert_ne!(first, second, "<C-j> must start a fresh terminal");
    }

    #[test]
    fn what_is_typed_in_the_panel_reaches_the_shell_and_comes_back() {
        let (mut app, rx) = test_app_with_events();
        press_ctrl(&mut app, 'j');
        screen(&mut app); // sizes the pty to the panel

        type_text(&mut app, "echo omv-terminal-roundtrip");
        press(&mut app, crossterm::event::KeyCode::Enter);

        let saw_output = pump(&mut app, &rx, |app| {
            app.terminal
                .session
                .as_ref()
                .is_some_and(|s| s.screen().contents().contains("omv-terminal-roundtrip"))
        });
        assert!(
            saw_output,
            "typing must reach the shell and its output must reach the screen: {:?}",
            app.terminal.session.as_ref().map(|s| s.screen().contents())
        );

        let out = screen(&mut app);
        assert!(
            out.contains("omv-terminal-roundtrip"),
            "and be drawn in the panel:\n{out}"
        );
    }

    #[test]
    fn buffer_picker_lists_open_buffers() {
        let mut app = test_app();
        let effects = app.editor.dispatch(Action::FindBuffers, None);
        app.apply_effects_for_test(effects);
        let picker = app.picker.as_ref().expect("picker");
        assert_eq!(picker.kind, PickerKind::Buffers);
        assert_eq!(picker.matches.len(), 1, "one scratch buffer is open");
    }
}
