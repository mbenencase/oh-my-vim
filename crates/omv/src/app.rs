use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crossterm::event::{Event as CrossEvent, KeyCode as CtKey, KeyEvent};
use omv_config::{Config, Resolve, Resolver};
use omv_core::{Editor, Effect, LspIntent, Mode, Picker as PickerRequest, SubstituteScope};
use omv_find::{display_path, walk_files};
use omv_lsp::lsp_types::Diagnostic;
use omv_lsp::{Client as LspClient, LspPosition, Notification, Registry, Request as LspRequest};
use omv_syntax::{Highlighter, Span};

use crate::event::{AppEvent, to_key};
use crate::explorer::Explorer;
use crate::help::Help;
use crate::picker::{Item, Payload, Picker, PickerKind};
use crate::substitute::{Field, Substitute};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Editor,
    Explorer,
    Picker,
    Help,
    Substitute,
}

pub struct App {
    pub editor: Editor,
    pub config: Config,
    pub theme: Theme,
    pub resolver: Resolver,
    pub focus: Focus,
    pub explorer: Explorer,
    pub picker: Option<Picker>,
    pub help: Option<Help>,
    pub substitute: Option<Substitute>,
    /// Char ranges of the current find-and-replace pattern, so the renderer can
    /// paint them. Empty whenever the prompt is closed.
    pub match_ranges: Vec<std::ops::Range<usize>>,
    pub diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
    pub diagnostics_visible: bool,
    pub hover: Option<String>,
    pub highlights: Vec<Span>,
    /// First visible buffer line.
    pub scroll: usize,
    pub status: String,
    pub quit: bool,
    pub root: PathBuf,
    /// Rows available to the editor view; the renderer keeps this current.
    pub text_height: usize,

    highlighter: Highlighter,
    lsp_registry: Registry,
    lsp: HashMap<String, LspClient>,
    lsp_events: Sender<omv_lsp::Event>,
    events: Sender<AppEvent>,
    /// Buffers already announced to a server, so `didOpen` is sent exactly once.
    opened: HashMap<PathBuf, String>,
}

impl App {
    pub fn new(
        config: Config,
        root: PathBuf,
        events: Sender<AppEvent>,
        lsp_events: Sender<omv_lsp::Event>,
    ) -> Self {
        let mut editor = Editor::new();
        editor.indent_width = config.indent_width;
        App {
            editor,
            config,
            theme: Theme::default_dark(),
            resolver: Resolver::default(),
            focus: Focus::Editor,
            explorer: Explorer::new(root.clone()),
            picker: None,
            help: None,
            substitute: None,
            match_ranges: Vec::new(),
            diagnostics: HashMap::new(),
            diagnostics_visible: false,
            hover: None,
            highlights: Vec::new(),
            scroll: 0,
            status: String::new(),
            quit: false,
            root,
            text_height: 24,
            highlighter: Highlighter::new(),
            lsp_registry: Registry::with_defaults(),
            lsp: HashMap::new(),
            lsp_events,
            events,
            opened: HashMap::new(),
        }
    }

    pub fn open_path(&mut self, path: &Path) {
        match self.editor.open(path) {
            Ok(_) => {
                self.scroll = 0;
                self.refresh_highlights();
                self.announce_to_lsp();
            }
            Err(e) => self.status = format!("E: {}: {e}", path.display()),
        }
    }

    // ---- input --------------------------------------------------------------

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Input(CrossEvent::Key(key)) => self.handle_key(key),
            AppEvent::Input(CrossEvent::Resize(..)) => {}
            AppEvent::Input(_) => {}
            AppEvent::Lsp(event) => self.handle_lsp_event(event),
            AppEvent::PickerItems {
                kind,
                generation,
                items,
                truncated,
            } => {
                if let Some(picker) = &mut self.picker
                    && picker.kind == kind
                    && picker.generation == generation
                {
                    picker.set_items(items, truncated);
                }
            }
            AppEvent::Error(text) => self.status = text,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Any keypress dismisses a hover popup, the way it works everywhere else.
        self.hover = None;
        match self.focus {
            Focus::Help => self.handle_help_key(key),
            Focus::Substitute => self.handle_substitute_key(key),
            Focus::Picker => self.handle_picker_key(key),
            Focus::Explorer => self.handle_explorer_key(key),
            Focus::Editor => self.handle_editor_key(key),
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) {
        let Some(parsed) = to_key(key) else { return };
        let mode = self.editor.mode;
        let resolved = self.resolver.feed(&self.config.keymap, mode, parsed);

        let effects = match resolved {
            Resolve::Pending => return,
            Resolve::Action(action, count) => self.editor.dispatch(action, count),
            Resolve::Unmatched => match mode {
                // Unbound keys are text in the modes where text is what they mean.
                Mode::Insert => match key.code {
                    CtKey::Char(c) => self.editor.insert_char(c),
                    CtKey::Enter => self.editor.insert_newline(),
                    CtKey::Backspace => self.editor.backspace(),
                    CtKey::Tab => {
                        let width = self.editor.indent_width;
                        (0..width)
                            .flat_map(|_| self.editor.insert_char(' '))
                            .collect()
                    }
                    _ => vec![],
                },
                Mode::Command => match key.code {
                    CtKey::Char(c) => {
                        self.editor.command_line.push(c);
                        vec![]
                    }
                    CtKey::Backspace => {
                        if self.editor.command_line.pop().is_none() {
                            self.editor.mode = Mode::Normal;
                        }
                        vec![]
                    }
                    CtKey::Enter => self.editor.execute_command_line(),
                    _ => vec![],
                },
                _ => vec![],
            },
        };
        self.apply_effects(effects);
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        // Sized for the panel the renderer draws: the popup's inner height minus
        // its header row. Close enough that a page scroll lands where you expect.
        let viewport = self.text_height.saturating_sub(4).max(1);
        let Some(help) = &mut self.help else {
            self.focus = Focus::Editor;
            return;
        };
        let ctrl = key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);
        match key.code {
            CtKey::Char('j') | CtKey::Down => help.scroll_by(1, viewport),
            CtKey::Char('k') | CtKey::Up => help.scroll_by(-1, viewport),
            CtKey::Char('d') if ctrl => help.scroll_by(viewport as isize / 2, viewport),
            CtKey::Char('u') if ctrl => help.scroll_by(-(viewport as isize) / 2, viewport),
            CtKey::PageDown => help.scroll_by(viewport as isize, viewport),
            CtKey::PageUp => help.scroll_by(-(viewport as isize), viewport),
            CtKey::Char('g') | CtKey::Home => help.scroll_to_top(),
            CtKey::Char('G') | CtKey::End => help.scroll_to_bottom(viewport),
            CtKey::Esc | CtKey::Char('q') | CtKey::Enter => {
                self.help = None;
                self.focus = Focus::Editor;
            }
            _ => {}
        }
    }

    fn handle_explorer_key(&mut self, key: KeyEvent) {
        match key.code {
            CtKey::Char('j') | CtKey::Down => self.explorer.move_selection(1),
            CtKey::Char('k') | CtKey::Up => self.explorer.move_selection(-1),
            CtKey::Char('l') | CtKey::Right => self.explorer.expand(),
            CtKey::Char('h') | CtKey::Left => self.explorer.collapse(),
            CtKey::Char('.') => {
                self.explorer.show_hidden = !self.explorer.show_hidden;
                self.explorer.refresh();
            }
            CtKey::Enter | CtKey::Char('o') => {
                if let Some(path) = self.explorer.activate() {
                    self.open_path(&path.clone());
                    self.focus = Focus::Editor;
                }
            }
            CtKey::Esc | CtKey::Char('q') => self.focus = Focus::Editor,
            _ => {}
        }
    }

    fn handle_picker_key(&mut self, key: KeyEvent) {
        let Some(picker) = &mut self.picker else {
            self.focus = Focus::Editor;
            return;
        };
        match key.code {
            CtKey::Esc => {
                self.picker = None;
                self.focus = Focus::Editor;
            }
            CtKey::Down => picker.move_selection(1),
            CtKey::Up => picker.move_selection(-1),
            CtKey::Char('n')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                picker.move_selection(1)
            }
            CtKey::Char('p')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                picker.move_selection(-1)
            }
            CtKey::Enter => {
                let payload = picker.selected_payload().cloned();
                self.picker = None;
                self.focus = Focus::Editor;
                if let Some(payload) = payload {
                    self.activate_payload(payload);
                }
            }
            CtKey::Backspace => {
                picker.query.pop();
                self.after_query_change();
            }
            CtKey::Char(c) => {
                picker.query.push(c);
                self.after_query_change();
            }
            _ => {}
        }
    }

    fn after_query_change(&mut self) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        match picker.kind {
            // Grep must re-run against the new pattern; fuzzy pickers just re-rank.
            PickerKind::Text => {
                picker.generation += 1;
                picker.loading = true;
                let (query, generation) = (picker.query.clone(), picker.generation);
                self.spawn_text_search(query, generation);
            }
            _ => picker.rerank(),
        }
    }

    // ---- find & replace -----------------------------------------------------

    fn handle_substitute_key(&mut self, key: KeyEvent) {
        let Some(panel) = &mut self.substitute else {
            self.focus = Focus::Editor;
            return;
        };
        let ctrl = key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);

        match key.code {
            CtKey::Esc => self.close_substitute(),
            // The one binding this whole feature hangs on: reveal the second
            // field and move there, from whichever field you were in.
            CtKey::Char('s') if ctrl => {
                panel.replace_open = true;
                panel.field = Field::Replace;
            }
            CtKey::Char('a') if ctrl => self.run_substitute(SubstituteScope::All),
            CtKey::Char('n') if ctrl => self.step_match(true),
            CtKey::Char('p') if ctrl => self.step_match(false),
            CtKey::Down => self.step_match(true),
            CtKey::Up => self.step_match(false),
            CtKey::Tab | CtKey::BackTab => {
                if panel.replace_open {
                    panel.field = panel.field.other();
                }
            }
            // Before there is a replacement to apply, <CR> is just "next match".
            CtKey::Enter => {
                if panel.replace_open {
                    self.run_substitute(SubstituteScope::Next)
                } else {
                    self.step_match(true)
                }
            }
            CtKey::Backspace => {
                let searching = panel.field == Field::Find;
                if panel.active_mut().pop().is_none() && searching {
                    // Backspacing past the start of an empty prompt closes it,
                    // the way the `:` line does.
                    self.close_substitute();
                } else if searching {
                    self.after_find_change();
                }
            }
            CtKey::Char(c) if !ctrl => {
                let searching = panel.field == Field::Find;
                panel.active_mut().push(c);
                if searching {
                    self.after_find_change();
                }
            }
            _ => {}
        }
    }

    fn open_substitute(&mut self) {
        let origin = self.editor.buffer().cursor;
        // Seed the find field from the last search, so `/foo` then `<C-f>` picks
        // up where you left off instead of asking you to retype it.
        let mut panel = Substitute::new(origin);
        panel.find = self.editor.last_search.clone();
        self.substitute = Some(panel);
        self.focus = Focus::Substitute;
        self.after_find_change();
    }

    fn close_substitute(&mut self) {
        if let Some(panel) = self.substitute.take()
            && !panel.edited
        {
            let buf = self.editor.buffer_mut();
            buf.cursor = panel.origin.min(buf.rope.len_chars());
            buf.clamp_cursor(false);
        }
        self.match_ranges.clear();
        self.focus = Focus::Editor;
        self.scroll_to_cursor();
    }

    /// Re-count the matches and preview the first one from `origin`.
    fn after_find_change(&mut self) {
        let Some(panel) = &self.substitute else {
            return;
        };
        let (pattern, origin) = (panel.find.clone(), panel.origin);
        self.match_ranges = self.editor.find_matches(&pattern);
        if !self.match_ranges.is_empty() {
            let effects = self.editor.goto_match(&pattern, origin, true);
            self.apply_effects(effects);
        }
    }

    fn step_match(&mut self, forward: bool) {
        let Some(panel) = &self.substitute else {
            return;
        };
        let pattern = panel.find.clone();
        let cursor = self.editor.buffer().cursor;
        // Step off the match we are sitting on, or `next` would find it again.
        let from = if forward { cursor + 1 } else { cursor };
        let effects = self.editor.goto_match(&pattern, from, forward);
        self.apply_effects(effects);
        self.sync_origin();
    }

    fn run_substitute(&mut self, scope: SubstituteScope) {
        let Some(panel) = &self.substitute else {
            return;
        };
        let (find, replace) = (panel.find.clone(), panel.replace.clone());
        let effects = self.editor.substitute(&find, &replace, scope);
        let edited = effects
            .iter()
            .any(|e| matches!(e, Effect::BufferChanged { .. }));
        self.apply_effects(effects);
        self.match_ranges = self.editor.find_matches(&find);
        if let Some(panel) = &mut self.substitute {
            panel.edited |= edited;
        }
        self.sync_origin();
    }

    /// Anchor the next search at where the cursor actually ended up.
    fn sync_origin(&mut self) {
        let cursor = self.editor.buffer().cursor;
        if let Some(panel) = &mut self.substitute {
            panel.origin = cursor;
        }
    }

    fn activate_payload(&mut self, payload: Payload) {
        match payload {
            Payload::File(path) => self.open_path(&path),
            Payload::Location(path, line) => {
                self.open_path(&path);
                let target = line.saturating_sub(1) as usize;
                let buf = self.editor.buffer_mut();
                buf.cursor = buf.line_start(target.min(buf.last_line()));
                self.scroll_to_cursor();
            }
            Payload::Buffer(index) => {
                if index < self.editor.buffers.len() {
                    self.editor.current = index;
                    self.refresh_highlights();
                    self.scroll_to_cursor();
                }
            }
        }
    }

    // ---- effects ------------------------------------------------------------

    /// Test hook: `apply_effects` is private, but tests need to drive the same path.
    #[cfg(test)]
    pub fn apply_effects_for_test(&mut self, effects: Vec<Effect>) {
        self.apply_effects(effects);
    }

    fn apply_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::BufferChanged { .. } => {
                    self.refresh_highlights();
                    self.notify_lsp_change();
                }
                Effect::ScrollToCursor => self.scroll_to_cursor(),
                Effect::Status(text) => self.status = text,
                Effect::ToggleExplorer => {
                    self.explorer.visible = !self.explorer.visible;
                    if !self.explorer.visible && self.focus == Focus::Explorer {
                        self.focus = Focus::Editor;
                    }
                }
                Effect::FocusExplorer => {
                    self.explorer.visible = true;
                    if let Some(path) = self.editor.buffer().path.clone() {
                        self.explorer.reveal(&path);
                    }
                    self.focus = Focus::Explorer;
                }
                Effect::ToggleDiagnostics => {
                    self.diagnostics_visible = !self.diagnostics_visible;
                }
                Effect::ShowKeys => {
                    // Rebuilt on each open so it reflects the live keymap.
                    self.help = Some(Help::build(&self.config));
                    self.focus = Focus::Help;
                }
                Effect::OpenPicker(kind) => self.open_picker(kind),
                Effect::OpenSubstitute => self.open_substitute(),
                Effect::Lsp(intent) => self.send_lsp(intent),
                Effect::Quit { force } => {
                    let unsaved = self.editor.buffers.iter().filter(|b| b.modified).count();
                    if force || unsaved == 0 {
                        self.quit = true;
                    } else {
                        self.status =
                            format!("{unsaved} buffer(s) with unsaved changes; :q! to discard");
                    }
                }
            }
        }
    }

    pub fn scroll_to_cursor(&mut self) {
        let line = self.editor.buffer().cursor_position().line;
        let height = self.text_height.max(1);
        // A three-line margin keeps context visible instead of pinning the
        // cursor to the very edge of the viewport.
        let margin = (height / 4).min(3);
        if line < self.scroll + margin {
            self.scroll = line.saturating_sub(margin);
        } else if line + margin >= self.scroll + height {
            self.scroll = (line + margin + 1).saturating_sub(height);
        }
        let max_scroll = self.editor.buffer().last_line();
        self.scroll = self.scroll.min(max_scroll);
    }

    // ---- pickers ------------------------------------------------------------

    fn open_picker(&mut self, kind: PickerRequest) {
        let kind = match kind {
            PickerRequest::Files => PickerKind::Files,
            PickerRequest::Text => PickerKind::Text,
            PickerRequest::Buffers => PickerKind::Buffers,
        };
        let mut picker = Picker::new(kind);
        let generation = picker.generation;

        match kind {
            PickerKind::Buffers => {
                let items = self
                    .editor
                    .buffers
                    .iter()
                    .enumerate()
                    .map(|(index, buf)| Item {
                        display: match &buf.path {
                            Some(p) => display_path(p, &self.root),
                            None => buf.name(),
                        },
                        payload: Payload::Buffer(index),
                    })
                    .collect();
                picker.set_items(items, false);
            }
            PickerKind::Files => self.spawn_file_walk(generation),
            PickerKind::Text => {} // waits for a query
        }
        self.picker = Some(picker);
        self.focus = Focus::Picker;
    }

    fn spawn_file_walk(&self, generation: u64) {
        let (root, tx) = (self.root.clone(), self.events.clone());
        std::thread::spawn(move || {
            let (paths, truncated) = walk_files(&root, 20_000);
            let items = paths
                .into_iter()
                .map(|path| Item {
                    display: display_path(&path, &root),
                    payload: Payload::File(path),
                })
                .collect();
            let _ = tx.send(AppEvent::PickerItems {
                kind: PickerKind::Files,
                generation,
                items,
                truncated,
            });
        });
    }

    fn spawn_text_search(&self, query: String, generation: u64) {
        if query.len() < 2 {
            let _ = self.events.send(AppEvent::PickerItems {
                kind: PickerKind::Text,
                generation,
                items: Vec::new(),
                truncated: false,
            });
            return;
        }
        let (root, tx) = (self.root.clone(), self.events.clone());
        std::thread::spawn(move || {
            let (items, truncated) = match omv_find::search(&root, &query, 2_000) {
                Ok((hits, truncated)) => (
                    hits.into_iter()
                        .map(|hit| Item {
                            display: format!(
                                "{}:{}: {}",
                                display_path(&hit.path, &root),
                                hit.line_number,
                                hit.line.trim_start()
                            ),
                            payload: Payload::Location(hit.path, hit.line_number),
                        })
                        .collect(),
                    truncated,
                ),
                // An in-progress regex like `foo(` is a normal state while typing,
                // not an error worth interrupting the user for.
                Err(_) => (Vec::new(), false),
            };
            let _ = tx.send(AppEvent::PickerItems {
                kind: PickerKind::Text,
                generation,
                items,
                truncated,
            });
        });
    }

    // ---- syntax -------------------------------------------------------------

    pub fn refresh_highlights(&mut self) {
        let path = self.editor.buffer().path.clone();
        let Some(language) = Highlighter::language_for(path.as_deref()) else {
            self.highlights.clear();
            return;
        };
        let text = self.editor.buffer().rope.to_string();
        self.highlights = self.highlighter.highlight(language, &text);
    }

    // ---- lsp ----------------------------------------------------------------

    fn language_of_current(&self) -> Option<&'static str> {
        Highlighter::language_for(self.editor.buffer().path.as_deref())
    }

    /// Start a server for the current buffer's language if needed, then `didOpen`.
    fn announce_to_lsp(&mut self) {
        let Some(language) = self.language_of_current() else {
            return;
        };
        let Some(path) = self.editor.buffer().path.clone() else {
            return;
        };

        if !self.lsp.contains_key(language) {
            if !self.lsp_registry.is_available(language) {
                let cmd = self.lsp_registry.get(language).map(|s| s.command.clone());
                if let Some(cmd) = cmd {
                    self.status = format!("LSP: `{cmd}` not found on PATH");
                }
                return;
            }
            let spec = self
                .lsp_registry
                .get(language)
                .cloned()
                .expect("checked above");
            match LspClient::spawn(language, &spec, &self.root, self.lsp_events.clone()) {
                Ok(client) => {
                    self.lsp.insert(language.to_string(), client);
                }
                Err(e) => {
                    self.status = format!("LSP: {e}");
                    return;
                }
            }
        }

        if self.opened.contains_key(&path) {
            return;
        }
        let text = self.editor.buffer().rope.to_string();
        let version = self.editor.buffer().version;
        if let Some(client) = self.lsp.get(language) {
            client.notify(Notification::DidOpen {
                path: path.clone(),
                language_id: language.to_string(),
                version,
                text,
            });
            self.opened.insert(path, language.to_string());
        }
    }

    fn notify_lsp_change(&mut self) {
        let Some(path) = self.editor.buffer().path.clone() else {
            return;
        };
        let Some(language) = self.opened.get(&path).cloned() else {
            return;
        };
        let Some(client) = self.lsp.get(&language) else {
            return;
        };
        client.notify(Notification::DidChange {
            path,
            version: self.editor.buffer().version,
            text: self.editor.buffer().rope.to_string(),
        });
    }

    fn send_lsp(&mut self, intent: LspIntent) {
        // Diagnostic navigation is local; it needs no round trip.
        match intent {
            LspIntent::NextDiagnostic | LspIntent::PrevDiagnostic => {
                return self.jump_diagnostic(matches!(intent, LspIntent::NextDiagnostic));
            }
            _ => {}
        }

        let Some(path) = self.editor.buffer().path.clone() else {
            self.status = "LSP: buffer has no path".into();
            return;
        };
        let Some(language) = self.opened.get(&path).cloned() else {
            self.status = "LSP: no server for this buffer".into();
            return;
        };
        let Some(client) = self.lsp.get(&language) else {
            return;
        };

        let pos = self.editor.buffer().lsp_position();
        let position = LspPosition {
            line: pos.line as u32,
            character: pos.column as u32,
        };

        match intent {
            LspIntent::Hover(_) => client.request(LspRequest::Hover { path, position }),
            LspIntent::GotoDefinition(_) => {
                client.request(LspRequest::Definition { path, position })
            }
            LspIntent::References(_) => client.request(LspRequest::References { path, position }),
            LspIntent::Format => client.request(LspRequest::Formatting {
                path,
                tab_size: self.editor.indent_width as u32,
            }),
            // Rename needs a name from the user; the prompt isn't built yet, so
            // say so rather than silently doing nothing.
            LspIntent::Rename(_) => self.status = "rename: prompt not implemented yet".into(),
            LspIntent::NextDiagnostic | LspIntent::PrevDiagnostic => unreachable!("handled above"),
        }
    }

    fn handle_lsp_event(&mut self, event: omv_lsp::Event) {
        use omv_lsp::Event as E;
        match event {
            E::Ready { language, server } => {
                self.status = format!("LSP: {server} ready ({language})")
            }
            E::Diagnostics { path, diagnostics } => {
                self.diagnostics.insert(path, diagnostics);
            }
            E::Hover { text } => self.hover = Some(text),
            E::Definition { path, position } => {
                self.open_path(&path);
                let buf = self.editor.buffer_mut();
                let pos = buf.from_utf16(position.line as usize, position.character as usize);
                buf.cursor = buf.position_to_char(pos);
                self.scroll_to_cursor();
            }
            E::References { locations } => {
                let items = locations
                    .into_iter()
                    .map(|(path, pos)| Item {
                        display: format!("{}:{}", display_path(&path, &self.root), pos.line + 1),
                        payload: Payload::Location(path, pos.line as u64 + 1),
                    })
                    .collect::<Vec<_>>();
                if items.is_empty() {
                    self.status = "no references".into();
                } else {
                    let mut picker = Picker::new(PickerKind::Buffers);
                    picker.set_items(items, false);
                    self.picker = Some(picker);
                    self.focus = Focus::Picker;
                }
            }
            E::Edits { changes } => self.apply_text_edits(changes),
            E::Message { text } => self.status = text,
            E::Error { text } => self.status = text,
            E::Stopped { language, reason } => {
                self.lsp.remove(&language);
                self.opened.retain(|_, l| l != &language);
                self.status = format!("LSP {language} stopped: {reason}");
            }
        }
    }

    fn apply_text_edits(&mut self, changes: Vec<(PathBuf, Vec<omv_lsp::lsp_types::TextEdit>)>) {
        let current = self.editor.buffer().path.clone();
        for (path, mut edits) in changes {
            // An empty path means "the buffer that asked" — formatting replies
            // carry no URI of their own.
            let target = if path.as_os_str().is_empty() {
                current.clone()
            } else {
                Some(path)
            };
            let Some(target) = target else { continue };
            let Some(index) = self
                .editor
                .buffers
                .iter()
                .position(|b| b.path.as_ref() == Some(&target))
            else {
                self.status = format!("skipped edits for unopened {}", target.display());
                continue;
            };

            // Apply bottom-up so earlier ranges keep their offsets.
            edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));
            edits.reverse();

            let buf = &mut self.editor.buffers[index];
            buf.begin_transaction();
            for edit in edits {
                let start = buf.from_utf16(
                    edit.range.start.line as usize,
                    edit.range.start.character as usize,
                );
                let end = buf.from_utf16(
                    edit.range.end.line as usize,
                    edit.range.end.character as usize,
                );
                let (start, end) = (buf.position_to_char(start), buf.position_to_char(end));
                buf.replace(start..end, &edit.new_text);
            }
            buf.commit_transaction();
            buf.clamp_cursor(false);
        }
        self.refresh_highlights();
        self.notify_lsp_change();
        self.scroll_to_cursor();
    }

    fn jump_diagnostic(&mut self, forward: bool) {
        let Some(path) = self.editor.buffer().path.clone() else {
            return;
        };
        let Some(diagnostics) = self.diagnostics.get(&path) else {
            self.status = "no diagnostics".into();
            return;
        };
        let current = self.editor.buffer().cursor_position().line as u32;
        let mut lines: Vec<u32> = diagnostics.iter().map(|d| d.range.start.line).collect();
        lines.sort_unstable();
        lines.dedup();

        let target = if forward {
            lines
                .iter()
                .find(|l| **l > current)
                .or_else(|| lines.first())
        } else {
            lines
                .iter()
                .rev()
                .find(|l| **l < current)
                .or_else(|| lines.last())
        };
        match target.copied() {
            Some(line) => {
                let buf = self.editor.buffer_mut();
                buf.cursor = buf.line_start(line as usize);
                self.scroll_to_cursor();
            }
            None => self.status = "no diagnostics".into(),
        }
    }

    pub fn current_diagnostics(&self) -> &[Diagnostic] {
        self.editor
            .buffer()
            .path
            .as_ref()
            .and_then(|p| self.diagnostics.get(p))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}
