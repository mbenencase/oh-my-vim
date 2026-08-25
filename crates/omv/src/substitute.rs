/// The find-and-replace prompt.
///
/// Two fields, but only one of them exists until you ask for it: `<C-f>` opens a
/// plain find, and `<C-s>` is what turns it into a replace. That keeps the
/// common case — "where is this string" — a single line, and makes the
/// destructive case something you opt into with a keystroke.
pub struct Substitute {
    pub find: String,
    pub replace: String,
    pub field: Field,
    /// False until `<C-s>`; while false, `<CR>` only walks the matches, so a
    /// stray Enter can never delete what you were merely looking for.
    pub replace_open: bool,
    /// Where the match walk resumes from: the cursor when the prompt opened,
    /// then updated by every jump and replacement. Editing the find text
    /// re-searches from here rather than from wherever the last preview landed,
    /// so typing another character doesn't slide you down the file.
    pub origin: usize,
    /// Set once anything has actually been replaced. Esc restores the cursor
    /// only while this is false — abandoning a search shouldn't move you, but
    /// abandoning an edit shouldn't teleport you away from it either.
    pub edited: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Find,
    Replace,
}

impl Field {
    pub fn other(self) -> Field {
        match self {
            Field::Find => Field::Replace,
            Field::Replace => Field::Find,
        }
    }
}

impl Substitute {
    pub fn new(origin: usize) -> Self {
        Substitute {
            find: String::new(),
            replace: String::new(),
            field: Field::Find,
            replace_open: false,
            origin,
            edited: false,
        }
    }

    pub fn active(&self) -> &String {
        match self.field {
            Field::Find => &self.find,
            Field::Replace => &self.replace,
        }
    }

    pub fn active_mut(&mut self) -> &mut String {
        match self.field {
            Field::Find => &mut self.find,
            Field::Replace => &mut self.replace,
        }
    }

    /// The key hints shown along the bottom of the panel. They change with
    /// `replace_open` because so does what `<CR>` does.
    pub fn hint(&self) -> &'static str {
        // Kept inside the panel's 54-column budget; the full story is in the README.
        if self.replace_open {
            "<CR> one  <C-a> all  <Tab> field  <Esc> close"
        } else {
            "<C-s> replace  <C-n>/<C-p> next  <Esc> close"
        }
    }
}
