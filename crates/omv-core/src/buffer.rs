use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ropey::Rope;

use crate::history::{Change, History, Transaction};

/// A cursor position expressed the way humans and LSP think about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: usize,
    /// Column in *characters*, not bytes and not display cells.
    pub column: usize,
}

/// An open document: rope, cursor, undo history, and the LSP version counter.
pub struct Buffer {
    pub rope: Rope,
    pub path: Option<PathBuf>,
    /// Char index of the cursor.
    pub cursor: usize,
    /// Sticky column for vertical motion, so `j` over a short line doesn't lose it.
    pub goal_column: Option<usize>,
    /// The fixed end of a visual selection; `None` outside visual mode.
    pub anchor: Option<usize>,
    pub modified: bool,
    /// `textDocument/didChange` version. Monotonic, never reset.
    pub version: i32,
    /// True when the file ends without a trailing newline and we should preserve that.
    pub no_final_newline: bool,
    history: History,
    /// Open transaction; edits accumulate here instead of committing individually.
    pending: Option<Transaction>,
}

impl Buffer {
    pub fn empty() -> Self {
        Buffer {
            rope: Rope::new(),
            path: None,
            cursor: 0,
            goal_column: None,
            anchor: None,
            modified: false,
            version: 0,
            no_final_newline: false,
            history: History::default(),
            pending: None,
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let mut buf = Buffer::empty();
        buf.path = Some(path.to_path_buf());
        if path.exists() {
            let text = fs::read_to_string(path)?;
            buf.no_final_newline = !text.is_empty() && !text.ends_with('\n');
            buf.rope = Rope::from_str(&text);
        }
        Ok(buf)
    }

    pub fn save(&mut self) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "buffer has no path"))?;
        let mut text = self.rope.to_string();
        if !self.no_final_newline && !text.ends_with('\n') {
            text.push('\n');
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, text)?;
        self.modified = false;
        Ok(())
    }

    pub fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "[No Name]".to_string())
    }

    // ---- geometry -----------------------------------------------------------

    /// Lines as a human counts them: a trailing newline does not create a phantom line.
    pub fn line_count(&self) -> usize {
        let n = self.rope.len_lines();
        if n > 1 && self.rope.char(self.rope.len_chars() - 1) == '\n' {
            n - 1
        } else {
            n
        }
    }

    pub fn last_line(&self) -> usize {
        self.line_count().saturating_sub(1)
    }

    /// Char index where `line` starts.
    pub fn line_start(&self, line: usize) -> usize {
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        self.rope.line_to_char(line)
    }

    /// Char index of the newline ending `line` (or end of rope on the last line).
    pub fn line_end(&self, line: usize) -> usize {
        let start = self.line_start(line);
        let slice = self
            .rope
            .line(line.min(self.rope.len_lines().saturating_sub(1)));
        let mut len = slice.len_chars();
        // Trim the line terminator; the cursor never sits on it.
        if len > 0 && slice.char(len - 1) == '\n' {
            len -= 1;
            if len > 0 && slice.char(len - 1) == '\r' {
                len -= 1; // CRLF counts as one terminator, not two
            }
        }
        start + len
    }

    /// Number of characters on `line`, excluding the terminator.
    pub fn line_len(&self, line: usize) -> usize {
        self.line_end(line) - self.line_start(line)
    }

    pub fn char_to_position(&self, char_idx: usize) -> Position {
        let char_idx = char_idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(char_idx);
        Position {
            line,
            column: char_idx - self.rope.line_to_char(line),
        }
    }

    pub fn position_to_char(&self, pos: Position) -> usize {
        let line = pos.line.min(self.last_line());
        let start = self.line_start(line);
        (start + pos.column).min(self.line_end(line))
    }

    pub fn cursor_position(&self) -> Position {
        self.char_to_position(self.cursor)
    }

    /// Highest char index the cursor may occupy on `line` for the given mode.
    /// Normal mode rests *on* a character; insert mode may sit past the last one.
    pub fn max_cursor_on_line(&self, line: usize, past_end: bool) -> usize {
        let end = self.line_end(line);
        if past_end {
            end
        } else {
            end.saturating_sub(usize::from(self.line_len(line) > 0))
        }
    }

    pub fn clamp_cursor(&mut self, past_end: bool) {
        let pos = self.cursor_position();
        let line = pos.line.min(self.last_line());
        let start = self.line_start(line);
        let max = self.max_cursor_on_line(line, past_end);
        self.cursor = self.cursor.clamp(start, max);
    }

    // ---- LSP coordinates ----------------------------------------------------

    /// Column in UTF-16 code units, which is what LSP means by "character"
    /// under the default position encoding. Differs from our char column on any
    /// line containing astral-plane characters (emoji, some CJK extensions).
    pub fn utf16_column(&self, pos: Position) -> usize {
        let start = self.line_start(pos.line);
        let end = (start + pos.column).min(self.line_end(pos.line));
        self.rope
            .slice(start..end)
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    /// Inverse of [`Buffer::utf16_column`], clamped to the line.
    pub fn from_utf16(&self, line: usize, utf16_column: usize) -> Position {
        let line = line.min(self.last_line());
        let start = self.line_start(line);
        let end = self.line_end(line);
        let mut units = 0usize;
        let mut column = 0usize;
        for idx in start..end {
            let width = self.rope.char(idx).len_utf16();
            // Round *down* on a position landing mid-surrogate: consuming the
            // char would overshoot and hand back a column past it.
            if units + width > utf16_column {
                break;
            }
            units += width;
            column += 1;
        }
        Position { line, column }
    }

    /// Cursor as an LSP position.
    pub fn lsp_position(&self) -> Position {
        let pos = self.cursor_position();
        Position {
            line: pos.line,
            column: self.utf16_column(pos),
        }
    }

    // ---- selection ----------------------------------------------------------

    /// Inclusive-start, exclusive-end char range of the visual selection.
    pub fn selection_range(&self, line_wise: bool) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        let (lo, hi) = if anchor <= self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        };
        if line_wise {
            let first = self.rope.char_to_line(lo);
            let last = self.rope.char_to_line(hi);
            let start = self.line_start(first);
            let end = (self.line_end(last) + 1).min(self.rope.len_chars());
            Some((start, end))
        } else {
            Some((lo, (hi + 1).min(self.rope.len_chars())))
        }
    }

    // ---- editing ------------------------------------------------------------

    /// Open an undo group. Edits until `commit` collapse into one transaction.
    pub fn begin_transaction(&mut self) {
        if self.pending.is_none() {
            self.pending = Some(Transaction::new(self.cursor));
        }
    }

    pub fn commit_transaction(&mut self) {
        if let Some(mut tx) = self.pending.take()
            && !tx.is_empty()
        {
            tx.cursor_after = self.cursor;
            self.history.push(tx);
        }
    }

    pub fn insert(&mut self, at: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        let at = at.min(self.rope.len_chars());
        self.rope.insert(at, text);
        self.record(Change {
            at,
            removed: String::new(),
            inserted: text.to_string(),
        });
    }

    pub fn remove(&mut self, range: std::ops::Range<usize>) -> String {
        let start = range.start.min(self.rope.len_chars());
        let end = range.end.min(self.rope.len_chars());
        if start >= end {
            return String::new();
        }
        let removed = self.rope.slice(start..end).to_string();
        self.rope.remove(start..end);
        self.record(Change {
            at: start,
            removed: removed.clone(),
            inserted: String::new(),
        });
        removed
    }

    pub fn replace(&mut self, range: std::ops::Range<usize>, text: &str) -> String {
        let removed = self.remove(range.clone());
        self.insert(range.start, text);
        removed
    }

    fn record(&mut self, change: Change) {
        self.modified = true;
        self.version += 1;
        match &mut self.pending {
            Some(tx) => tx.changes.push(change),
            None => {
                let mut tx = Transaction::new(self.cursor);
                tx.changes.push(change);
                tx.cursor_after = self.cursor;
                self.history.push(tx);
            }
        }
    }

    // ---- undo / redo --------------------------------------------------------

    pub fn undo(&mut self) -> bool {
        self.commit_transaction();
        let Some(tx) = self.history.undo() else {
            return false;
        };
        let changes: Vec<Change> = tx.changes.iter().rev().map(Change::inverse).collect();
        let cursor = tx.cursor_before;
        self.apply_raw(&changes);
        self.cursor = cursor.min(self.rope.len_chars());
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(tx) = self.history.redo() else {
            return false;
        };
        let changes = tx.changes.clone();
        let cursor = tx.cursor_after;
        self.apply_raw(&changes);
        self.cursor = cursor.min(self.rope.len_chars());
        true
    }

    /// Apply changes without recording them — used only by undo/redo.
    fn apply_raw(&mut self, changes: &[Change]) {
        for c in changes {
            let at = c.at.min(self.rope.len_chars());
            if !c.removed.is_empty() {
                let end = (at + c.removed.chars().count()).min(self.rope.len_chars());
                self.rope.remove(at..end);
            }
            if !c.inserted.is_empty() {
                self.rope.insert(at, &c.inserted);
            }
        }
        self.modified = true;
        self.version += 1;
    }
}
