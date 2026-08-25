use std::path::{Path, PathBuf};

use crate::action::Action;
use crate::buffer::{Buffer, Position};
use crate::mode::Mode;
use crate::movement as mv;
use crate::textobject::{self, ObjectKind, Scope};

/// What the UI layer must do after a dispatch. The core never opens a panel,
/// talks to a language server, or exits — it only says that it wants to.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Buffer text changed; re-highlight and notify the language server.
    BufferChanged {
        buffer: usize,
    },
    /// Keep the cursor on screen.
    ScrollToCursor,
    ToggleExplorer,
    FocusExplorer,
    ToggleDiagnostics,
    /// Show the key-binding reference.
    ShowKeys,
    OpenPicker(Picker),
    Lsp(LspIntent),
    /// Show a message on the status line.
    Status(String),
    /// Close the editor. `force` skips the modified-buffer check.
    Quit {
        force: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picker {
    Files,
    Text,
    Buffers,
}

/// LSP requests expressed without depending on `lsp-types`, so `omv-core`
/// stays free of the protocol crate and of tokio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspIntent {
    Hover(Position),
    GotoDefinition(Position),
    References(Position),
    Rename(Position),
    Format,
    NextDiagnostic,
    PrevDiagnostic,
}

#[derive(Debug, Clone, Default)]
pub struct Register {
    pub text: String,
    /// Line-wise yanks paste onto their own line rather than mid-line.
    pub line_wise: bool,
}

pub struct Editor {
    pub buffers: Vec<Buffer>,
    pub current: usize,
    pub mode: Mode,
    pub register: Register,
    /// Contents of the `:` prompt while in command mode, without the leading colon.
    pub command_line: String,
    pub status: String,
    /// Visible text rows, kept up to date by the UI so half-page motions know how far to go.
    pub viewport_height: usize,
    pub indent_width: usize,
    pub last_search: String,
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            buffers: vec![Buffer::empty()],
            current: 0,
            mode: Mode::Normal,
            register: Register::default(),
            command_line: String::new(),
            status: String::new(),
            viewport_height: 24,
            indent_width: 4,
            last_search: String::new(),
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffers[self.current]
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.current]
    }

    pub fn open(&mut self, path: impl AsRef<Path>) -> std::io::Result<usize> {
        let path = path.as_ref().to_path_buf();
        if let Some(idx) = self
            .buffers
            .iter()
            .position(|b| b.path.as_ref() == Some(&path))
        {
            self.current = idx;
            return Ok(idx);
        }
        let buf = Buffer::from_file(&path)?;
        // The initial empty scratch buffer is disposable; don't accumulate it.
        if self.buffers.len() == 1 && self.buffers[0].path.is_none() && !self.buffers[0].modified {
            self.buffers[0] = buf;
            self.current = 0;
        } else {
            self.buffers.push(buf);
            self.current = self.buffers.len() - 1;
        }
        Ok(self.current)
    }

    /// Insert-mode input that isn't bound to an action.
    pub fn insert_char(&mut self, c: char) -> Vec<Effect> {
        let buffer = self.current;
        let buf = self.buffer_mut();
        buf.begin_transaction();
        let at = buf.cursor;
        buf.insert(at, &c.to_string());
        buf.cursor = at + 1;
        buf.goal_column = None;
        vec![Effect::BufferChanged { buffer }, Effect::ScrollToCursor]
    }

    pub fn insert_newline(&mut self) -> Vec<Effect> {
        let buffer = self.current;
        let indent = self.current_indent();
        let buf = self.buffer_mut();
        buf.begin_transaction();
        let at = buf.cursor;
        let text = format!("\n{indent}");
        buf.insert(at, &text);
        buf.cursor = at + text.chars().count();
        buf.goal_column = None;
        vec![Effect::BufferChanged { buffer }, Effect::ScrollToCursor]
    }

    pub fn backspace(&mut self) -> Vec<Effect> {
        let buffer = self.current;
        let buf = self.buffer_mut();
        if buf.cursor == 0 {
            return vec![];
        }
        buf.begin_transaction();
        let at = buf.cursor;
        buf.remove((at - 1)..at);
        buf.cursor = at - 1;
        buf.goal_column = None;
        vec![Effect::BufferChanged { buffer }, Effect::ScrollToCursor]
    }

    fn current_indent(&self) -> String {
        let buf = self.buffer();
        let line = buf.cursor_position().line;
        let start = buf.line_start(line);
        let end = buf.line_end(line);
        (start..end)
            .map(|i| buf.rope.char(i))
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect()
    }

    // ---- dispatch -----------------------------------------------------------

    /// `count` is `None` when the user typed no count. That distinction matters:
    /// bare `G` goes to the last line, but `13G` goes to line 13.
    pub fn dispatch(&mut self, action: Action, count: Option<usize>) -> Vec<Effect> {
        let explicit = count.filter(|n| *n > 0);
        let count = explicit.unwrap_or(1);
        let buffer = self.current;
        let past_end = self.mode == Mode::Insert;

        macro_rules! moved {
            ($idx:expr) => {{
                let idx = $idx;
                let buf = self.buffer_mut();
                buf.cursor = idx;
                buf.goal_column = None;
                return vec![Effect::ScrollToCursor];
            }};
        }
        macro_rules! changed {
            () => {
                return vec![Effect::BufferChanged { buffer }, Effect::ScrollToCursor]
            };
        }

        match action {
            Action::Nop => vec![],

            // ---- motion -----------------------------------------------------
            Action::MoveLeft => moved!(mv::left(self.buffer(), count)),
            Action::MoveRight => moved!(mv::right(self.buffer(), count, past_end)),
            Action::MoveUp | Action::MoveDown => {
                let delta = if action == Action::MoveUp {
                    -(count as isize)
                } else {
                    count as isize
                };
                let (idx, goal) = mv::vertical(self.buffer(), delta, past_end);
                let buf = self.buffer_mut();
                buf.cursor = idx;
                buf.goal_column = Some(goal);
                vec![Effect::ScrollToCursor]
            }
            Action::MoveWordForward => moved!(mv::word_forward(self.buffer(), count)),
            Action::MoveWordBackward => moved!(mv::word_backward(self.buffer(), count)),
            Action::MoveWordEnd => moved!(mv::word_end(self.buffer(), count)),
            Action::MoveLineStart => moved!(mv::line_start(self.buffer())),
            Action::MoveLineFirstNonBlank => moved!(mv::line_first_non_blank(self.buffer())),
            Action::MoveLineEnd => moved!(mv::line_end(self.buffer(), past_end)),
            // `gg`/`G` take a line number when counted, which is how you jump
            // to a diagnostic or a stack-trace line without a `:` detour.
            Action::MoveFileStart => {
                let line = explicit.map_or(0, |n| n.saturating_sub(1));
                moved!(mv::goto_line(self.buffer(), line))
            }
            Action::MoveFileEnd => {
                let line = explicit.map_or(self.buffer().last_line(), |n| n.saturating_sub(1));
                moved!(mv::goto_line(self.buffer(), line))
            }
            Action::MoveHalfPageDown | Action::MoveHalfPageUp => {
                let half = (self.viewport_height / 2).max(1) as isize;
                let delta = if action == Action::MoveHalfPageUp {
                    -half
                } else {
                    half
                };
                let (idx, goal) = mv::vertical(self.buffer(), delta, past_end);
                let buf = self.buffer_mut();
                buf.cursor = idx;
                buf.goal_column = Some(goal);
                vec![Effect::ScrollToCursor]
            }
            Action::MoveMatchingPair => {
                match mv::matching_pair(self.buffer(), self.buffer().cursor) {
                    Some(idx) => moved!(idx),
                    None => vec![],
                }
            }

            // ---- mode -------------------------------------------------------
            Action::EnterNormalMode => {
                let buf = self.buffer_mut();
                buf.commit_transaction();
                buf.anchor = None;
                self.mode = Mode::Normal;
                self.command_line.clear();
                self.buffer_mut().clamp_cursor(false);
                vec![Effect::ScrollToCursor]
            }
            Action::EnterInsertMode => self.enter_insert(None),
            Action::EnterInsertAfter => {
                let idx = mv::right(self.buffer(), 1, true);
                self.enter_insert(Some(idx))
            }
            Action::EnterInsertLineStart => {
                let idx = mv::line_first_non_blank(self.buffer());
                self.enter_insert(Some(idx))
            }
            Action::EnterInsertLineEnd => {
                let idx = mv::line_end(self.buffer(), true);
                self.enter_insert(Some(idx))
            }
            Action::EnterVisualMode | Action::EnterVisualLineMode => {
                let cursor = self.buffer().cursor;
                self.buffer_mut().anchor = Some(cursor);
                self.mode = if action == Action::EnterVisualMode {
                    Mode::Visual
                } else {
                    Mode::VisualLine
                };
                vec![]
            }
            Action::EnterCommandMode => {
                self.mode = Mode::Command;
                self.command_line.clear();
                vec![]
            }

            // ---- editing ----------------------------------------------------
            Action::OpenLineBelow | Action::OpenLineAbove => {
                let indent = self.current_indent();
                let line = self.buffer().cursor_position().line;
                let buf = self.buffer_mut();
                buf.begin_transaction();
                let at = if action == Action::OpenLineBelow {
                    let end = buf.line_end(line);
                    buf.insert(end, &format!("\n{indent}"));
                    end + 1 + indent.chars().count()
                } else {
                    let start = buf.line_start(line);
                    buf.insert(start, &format!("{indent}\n"));
                    start + indent.chars().count()
                };
                buf.cursor = at;
                buf.goal_column = None;
                self.mode = Mode::Insert;
                changed!()
            }
            Action::DeleteCharForward => {
                let buf = self.buffer_mut();
                let at = buf.cursor;
                let line = buf.cursor_position().line;
                let end = (at + count).min(buf.line_end(line));
                if end > at {
                    let removed = buf.remove(at..end);
                    self.register = Register {
                        text: removed,
                        line_wise: false,
                    };
                    self.buffer_mut().clamp_cursor(false);
                    changed!()
                }
                vec![]
            }
            Action::DeleteCharBackward => self.backspace(),
            Action::DeleteLine => {
                let buf = self.buffer_mut();
                let first = buf.cursor_position().line;
                let last = (first + count - 1).min(buf.last_line());
                let start = buf.line_start(first);
                let end = (buf.line_end(last) + 1).min(buf.rope.len_chars());
                let removed = buf.remove(start..end);
                self.register = Register {
                    text: removed,
                    line_wise: true,
                };
                let buf = self.buffer_mut();
                buf.cursor = start.min(buf.rope.len_chars());
                buf.clamp_cursor(false);
                changed!()
            }
            Action::DeleteToLineEnd | Action::ChangeToLineEnd => {
                let buf = self.buffer_mut();
                let at = buf.cursor;
                let end = buf.line_end(buf.cursor_position().line);
                let removed = buf.remove(at..end);
                self.register = Register {
                    text: removed,
                    line_wise: false,
                };
                if action == Action::ChangeToLineEnd {
                    self.mode = Mode::Insert;
                    self.buffer_mut().begin_transaction();
                } else {
                    self.buffer_mut().clamp_cursor(false);
                }
                changed!()
            }
            Action::DeleteWord | Action::ChangeWord => {
                let target = if action == Action::ChangeWord {
                    mv::word_end(self.buffer(), count) + 1
                } else {
                    mv::word_forward(self.buffer(), count)
                };
                let buf = self.buffer_mut();
                let at = buf.cursor;
                let end = target.max(at).min(buf.rope.len_chars());
                let removed = buf.remove(at..end);
                self.register = Register {
                    text: removed,
                    line_wise: false,
                };
                if action == Action::ChangeWord {
                    self.mode = Mode::Insert;
                    self.buffer_mut().begin_transaction();
                } else {
                    self.buffer_mut().clamp_cursor(false);
                }
                changed!()
            }
            Action::ChangeLine => {
                let indent = self.current_indent();
                let line = self.buffer().cursor_position().line;
                let buf = self.buffer_mut();
                buf.begin_transaction();
                let start = buf.line_start(line);
                let end = buf.line_end(line);
                let removed = buf.remove(start..end);
                self.register = Register {
                    text: removed,
                    line_wise: false,
                };
                let buf = self.buffer_mut();
                buf.insert(start, &indent);
                buf.cursor = start + indent.chars().count();
                self.mode = Mode::Insert;
                changed!()
            }
            Action::DeleteSelection | Action::ChangeSelection | Action::YankSelection => {
                let line_wise = self.mode == Mode::VisualLine;
                let Some((start, end)) = self.buffer().selection_range(line_wise) else {
                    return vec![];
                };
                if action == Action::YankSelection {
                    let text = self.buffer().rope.slice(start..end).to_string();
                    self.register = Register { text, line_wise };
                    let buf = self.buffer_mut();
                    buf.cursor = start;
                    buf.anchor = None;
                    self.mode = Mode::Normal;
                    return vec![Effect::ScrollToCursor];
                }
                let buf = self.buffer_mut();
                buf.begin_transaction();
                let removed = buf.remove(start..end);
                self.register = Register {
                    text: removed,
                    line_wise,
                };
                let buf = self.buffer_mut();
                buf.cursor = start;
                buf.anchor = None;
                if action == Action::ChangeSelection {
                    self.mode = Mode::Insert;
                } else {
                    self.mode = Mode::Normal;
                    self.buffer_mut().commit_transaction();
                    self.buffer_mut().clamp_cursor(false);
                }
                changed!()
            }
            Action::YankLine => {
                let buf = self.buffer();
                let first = buf.cursor_position().line;
                let last = (first + count - 1).min(buf.last_line());
                let start = buf.line_start(first);
                let end = (buf.line_end(last) + 1).min(buf.rope.len_chars());
                let text = buf.rope.slice(start..end).to_string();
                self.register = Register {
                    text,
                    line_wise: true,
                };
                vec![Effect::Status(format!(
                    "{} line(s) yanked",
                    last - first + 1
                ))]
            }
            Action::PasteAfter | Action::PasteBefore => {
                if self.register.text.is_empty() {
                    return vec![];
                }
                let text = self.register.text.clone();
                let line_wise = self.register.line_wise;
                let buf = self.buffer_mut();
                buf.begin_transaction();
                if line_wise {
                    let line = buf.cursor_position().line;
                    let at = if action == Action::PasteAfter {
                        (buf.line_end(line) + 1).min(buf.rope.len_chars())
                    } else {
                        buf.line_start(line)
                    };
                    let mut payload = text.clone();
                    if !payload.ends_with('\n') {
                        payload.push('\n');
                    }
                    buf.insert(at, &payload);
                    buf.cursor = at;
                } else {
                    let at = if action == Action::PasteAfter {
                        mv::right(buf, 1, true)
                    } else {
                        buf.cursor
                    };
                    buf.insert(at, &text);
                    buf.cursor = at + text.chars().count() - 1;
                }
                let buf = self.buffer_mut();
                buf.commit_transaction();
                buf.clamp_cursor(false);
                changed!()
            }
            Action::JoinLines => {
                let buf = self.buffer_mut();
                let line = buf.cursor_position().line;
                if line >= buf.last_line() {
                    return vec![];
                }
                buf.begin_transaction();
                let end = buf.line_end(line);
                let next_start = buf.line_start(line + 1);
                let mut skip = next_start;
                while skip < buf.line_end(line + 1) && buf.rope.char(skip).is_whitespace() {
                    skip += 1;
                }
                buf.replace(end..skip, " ");
                buf.cursor = end;
                let buf = self.buffer_mut();
                buf.commit_transaction();
                changed!()
            }
            Action::Indent | Action::Outdent => {
                let width = self.indent_width;
                let line_wise = self.mode == Mode::VisualLine;
                let (first, last) = match self.buffer().selection_range(line_wise) {
                    Some((s, e)) => {
                        let buf = self.buffer();
                        (
                            buf.rope.char_to_line(s),
                            buf.rope.char_to_line(e.saturating_sub(1)),
                        )
                    }
                    None => {
                        let line = self.buffer().cursor_position().line;
                        (line, line)
                    }
                };
                let buf = self.buffer_mut();
                buf.begin_transaction();
                for line in (first..=last.min(buf.last_line())).rev() {
                    let start = buf.line_start(line);
                    if action == Action::Indent {
                        buf.insert(start, &" ".repeat(width));
                    } else {
                        let end = buf.line_end(line);
                        let strip = (start..end)
                            .take(width)
                            .take_while(|&i| buf.rope.char(i) == ' ')
                            .count();
                        if strip > 0 {
                            buf.remove(start..start + strip);
                        }
                    }
                }
                let buf = self.buffer_mut();
                buf.commit_transaction();
                buf.clamp_cursor(false);
                changed!()
            }
            Action::Undo | Action::Redo => {
                let ok = if action == Action::Undo {
                    self.buffer_mut().undo()
                } else {
                    self.buffer_mut().redo()
                };
                if !ok {
                    let what = if action == Action::Undo {
                        "undo"
                    } else {
                        "redo"
                    };
                    return vec![Effect::Status(format!(
                        "Already at oldest/newest change ({what})"
                    ))];
                }
                self.buffer_mut().clamp_cursor(false);
                changed!()
            }

            // ---- text objects -----------------------------------------------
            Action::DeleteInsideWord => {
                self.object_op(ObjectKind::Word, Scope::Inside, ObjectOp::Delete)
            }
            Action::DeleteAroundWord => {
                self.object_op(ObjectKind::Word, Scope::Around, ObjectOp::Delete)
            }
            Action::DeleteInsideParen => {
                self.object_op(ObjectKind::Paren, Scope::Inside, ObjectOp::Delete)
            }
            Action::DeleteAroundParen => {
                self.object_op(ObjectKind::Paren, Scope::Around, ObjectOp::Delete)
            }
            Action::DeleteInsideBracket => {
                self.object_op(ObjectKind::Bracket, Scope::Inside, ObjectOp::Delete)
            }
            Action::DeleteAroundBracket => {
                self.object_op(ObjectKind::Bracket, Scope::Around, ObjectOp::Delete)
            }
            Action::DeleteInsideBrace => {
                self.object_op(ObjectKind::Brace, Scope::Inside, ObjectOp::Delete)
            }
            Action::DeleteAroundBrace => {
                self.object_op(ObjectKind::Brace, Scope::Around, ObjectOp::Delete)
            }
            Action::DeleteInsideQuote => {
                self.object_op(ObjectKind::Quote, Scope::Inside, ObjectOp::Delete)
            }
            Action::DeleteAroundQuote => {
                self.object_op(ObjectKind::Quote, Scope::Around, ObjectOp::Delete)
            }
            Action::ChangeInsideWord => {
                self.object_op(ObjectKind::Word, Scope::Inside, ObjectOp::Change)
            }
            Action::ChangeInsideParen => {
                self.object_op(ObjectKind::Paren, Scope::Inside, ObjectOp::Change)
            }
            Action::ChangeInsideBracket => {
                self.object_op(ObjectKind::Bracket, Scope::Inside, ObjectOp::Change)
            }
            Action::ChangeInsideBrace => {
                self.object_op(ObjectKind::Brace, Scope::Inside, ObjectOp::Change)
            }
            Action::ChangeInsideQuote => {
                self.object_op(ObjectKind::Quote, Scope::Inside, ObjectOp::Change)
            }
            Action::YankInsideWord => {
                self.object_op(ObjectKind::Word, Scope::Inside, ObjectOp::Yank)
            }
            Action::YankInsideParen => {
                self.object_op(ObjectKind::Paren, Scope::Inside, ObjectOp::Yank)
            }
            Action::YankInsideBrace => {
                self.object_op(ObjectKind::Brace, Scope::Inside, ObjectOp::Yank)
            }

            // ---- panels & lsp: pure intent, resolved by the UI ---------------
            Action::ToggleExplorer => vec![Effect::ToggleExplorer],
            Action::FocusExplorer => vec![Effect::FocusExplorer],
            Action::ToggleDiagnostics => vec![Effect::ToggleDiagnostics],
            Action::ShowKeys => vec![Effect::ShowKeys],
            Action::FindFiles => vec![Effect::OpenPicker(Picker::Files)],
            Action::FindText => vec![Effect::OpenPicker(Picker::Text)],
            Action::FindBuffers => vec![Effect::OpenPicker(Picker::Buffers)],
            Action::SearchForward => {
                self.mode = Mode::Command;
                self.command_line = "/".to_string();
                vec![]
            }
            Action::SearchNext => self.search(true),
            Action::SearchPrev => self.search(false),
            Action::LspHover => vec![Effect::Lsp(LspIntent::Hover(
                self.buffer().cursor_position(),
            ))],
            Action::LspGotoDefinition => {
                vec![Effect::Lsp(LspIntent::GotoDefinition(
                    self.buffer().cursor_position(),
                ))]
            }
            Action::LspReferences => {
                vec![Effect::Lsp(LspIntent::References(
                    self.buffer().cursor_position(),
                ))]
            }
            Action::LspRename => vec![Effect::Lsp(LspIntent::Rename(
                self.buffer().cursor_position(),
            ))],
            Action::LspFormat => vec![Effect::Lsp(LspIntent::Format)],
            Action::LspNextDiagnostic => vec![Effect::Lsp(LspIntent::NextDiagnostic)],
            Action::LspPrevDiagnostic => vec![Effect::Lsp(LspIntent::PrevDiagnostic)],

            // ---- files & buffers --------------------------------------------
            Action::Save => match self.buffer_mut().save() {
                Ok(()) => {
                    let name = self.buffer().name();
                    vec![Effect::Status(format!("\"{name}\" written"))]
                }
                Err(e) => vec![Effect::Status(format!("E: {e}"))],
            },
            Action::Quit => {
                if self.buffer().modified {
                    vec![Effect::Status(
                        "No write since last change (add ! to override)".into(),
                    )]
                } else {
                    vec![Effect::Quit { force: false }]
                }
            }
            Action::ForceQuit => vec![Effect::Quit { force: true }],
            Action::SaveAndQuit => match self.buffer_mut().save() {
                Ok(()) => vec![Effect::Quit { force: false }],
                Err(e) => vec![Effect::Status(format!("E: {e}"))],
            },
            Action::NextBuffer | Action::PrevBuffer => {
                let n = self.buffers.len();
                if n > 1 {
                    self.current = if action == Action::NextBuffer {
                        (self.current + 1) % n
                    } else {
                        (self.current + n - 1) % n
                    };
                }
                vec![Effect::ScrollToCursor, Effect::Status(self.buffer().name())]
            }
        }
    }

    fn enter_insert(&mut self, at: Option<usize>) -> Vec<Effect> {
        self.mode = Mode::Insert;
        let buf = self.buffer_mut();
        if let Some(idx) = at {
            buf.cursor = idx;
        }
        buf.anchor = None;
        buf.goal_column = None;
        buf.begin_transaction();
        vec![Effect::ScrollToCursor]
    }

    fn object_op(&mut self, kind: ObjectKind, scope: Scope, op: ObjectOp) -> Vec<Effect> {
        let buffer = self.current;
        let Some(range) = textobject::range(self.buffer(), kind, scope) else {
            return vec![Effect::Status("no matching text object".into())];
        };
        if op == ObjectOp::Yank {
            let text = self.buffer().rope.slice(range.clone()).to_string();
            self.register = Register {
                text,
                line_wise: false,
            };
            return vec![Effect::Status(format!("{} chars yanked", range.len()))];
        }
        let buf = self.buffer_mut();
        buf.begin_transaction();
        let removed = buf.remove(range.clone());
        self.register = Register {
            text: removed,
            line_wise: false,
        };
        let buf = self.buffer_mut();
        buf.cursor = range.start;
        if op == ObjectOp::Change {
            self.mode = Mode::Insert;
        } else {
            buf.commit_transaction();
            buf.clamp_cursor(false);
        }
        vec![Effect::BufferChanged { buffer }, Effect::ScrollToCursor]
    }

    fn search(&mut self, forward: bool) -> Vec<Effect> {
        if self.last_search.is_empty() {
            return vec![Effect::Status("no previous search".into())];
        }
        let needle = self.last_search.clone();
        let haystack = self.buffer().rope.to_string();
        let cursor_byte = self.buffer().rope.char_to_byte(self.buffer().cursor);

        let hit = if forward {
            haystack[cursor_byte.saturating_add(1).min(haystack.len())..]
                .find(&needle)
                .map(|i| i + cursor_byte + 1)
                .or_else(|| haystack.find(&needle))
        } else {
            haystack[..cursor_byte]
                .rfind(&needle)
                .or_else(|| haystack.rfind(&needle))
        };

        match hit {
            Some(byte) => {
                let buf = self.buffer_mut();
                buf.cursor = buf.rope.byte_to_char(byte);
                buf.goal_column = None;
                vec![Effect::ScrollToCursor]
            }
            None => vec![Effect::Status(format!("pattern not found: {needle}"))],
        }
    }

    /// Run whatever is sitting in the `:` (or `/`) prompt.
    pub fn execute_command_line(&mut self) -> Vec<Effect> {
        let line = std::mem::take(&mut self.command_line);
        self.mode = Mode::Normal;

        if let Some(pattern) = line.strip_prefix('/') {
            self.last_search = pattern.to_string();
            return self.search(true);
        }

        let mut parts = line.split_whitespace();
        let Some(cmd) = parts.next() else {
            return vec![];
        };
        let arg = parts.next();

        match cmd {
            "w" | "write" => {
                if let Some(path) = arg {
                    self.buffer_mut().path = Some(PathBuf::from(path));
                }
                self.dispatch(Action::Save, None)
            }
            "q" | "quit" => self.dispatch(Action::Quit, None),
            "q!" | "quit!" => self.dispatch(Action::ForceQuit, None),
            "wq" | "x" => self.dispatch(Action::SaveAndQuit, None),
            "e" | "edit" => match arg {
                Some(path) => match self.open(path) {
                    Ok(_) => vec![
                        Effect::ScrollToCursor,
                        Effect::BufferChanged {
                            buffer: self.current,
                        },
                    ],
                    Err(e) => vec![Effect::Status(format!("E: {e}"))],
                },
                None => vec![Effect::Status("E: :e needs a path".into())],
            },
            // `!` is conventionally vim's shell-escape, so `:keys` is the real
            // name and `:!keys` is accepted as an alias rather than claiming `!`.
            "keys" | "!keys" | "map" => self.dispatch(Action::ShowKeys, None),
            "bn" => self.dispatch(Action::NextBuffer, None),
            "bp" => self.dispatch(Action::PrevBuffer, None),
            other => match other.parse::<usize>() {
                Ok(line_no) => {
                    let idx = mv::goto_line(self.buffer(), line_no.saturating_sub(1));
                    let buf = self.buffer_mut();
                    buf.cursor = idx;
                    vec![Effect::ScrollToCursor]
                }
                Err(_) => vec![Effect::Status(format!("E: not a command: {other}"))],
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectOp {
    Delete,
    Change,
    Yank,
}

impl Default for Editor {
    fn default() -> Self {
        Editor::new()
    }
}
