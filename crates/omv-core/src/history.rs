/// One primitive replacement: at `at` (char index), `removed` became `inserted`.
#[derive(Debug, Clone)]
pub struct Change {
    pub at: usize,
    pub removed: String,
    pub inserted: String,
}

impl Change {
    pub fn inverse(&self) -> Change {
        Change {
            at: self.at,
            removed: self.inserted.clone(),
            inserted: self.removed.clone(),
        }
    }
}

/// The undo unit. An insert session (`i` … `Esc`) collapses into one of these so
/// a single `u` undoes the whole typing burst, which is what muscle memory expects.
#[derive(Debug, Clone)]
pub struct Transaction {
    pub changes: Vec<Change>,
    pub cursor_before: usize,
    pub cursor_after: usize,
}

impl Transaction {
    pub fn new(cursor_before: usize) -> Self {
        Transaction {
            changes: Vec::new(),
            cursor_before,
            cursor_after: cursor_before,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Linear undo. A tree (vim's `g-`/`g+`) is a later upgrade: it only needs
/// `entries` to become a node arena, since nothing outside indexes into it.
#[derive(Debug, Default)]
pub struct History {
    entries: Vec<Transaction>,
    /// Number of transactions currently applied; entries at or past this are redoable.
    index: usize,
}

impl History {
    pub fn push(&mut self, tx: Transaction) {
        self.entries.truncate(self.index);
        self.entries.push(tx);
        self.index += 1;
    }

    /// The transaction to revert, if any.
    pub fn undo(&mut self) -> Option<&Transaction> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        self.entries.get(self.index)
    }

    /// The transaction to re-apply, if any.
    pub fn redo(&mut self) -> Option<&Transaction> {
        let tx = self.entries.get(self.index)?;
        self.index += 1;
        Some(tx)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
