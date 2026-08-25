use std::collections::HashSet;
use std::path::{Path, PathBuf};

use omv_find::{DirEntry, read_dir};

/// One visible line of the tree.
#[derive(Debug, Clone)]
pub struct Row {
    pub entry: DirEntry,
    pub depth: usize,
    pub expanded: bool,
}

/// A file tree docked to the left. Directories are lazily read and cached only
/// while expanded, so a huge tree costs nothing until you open into it.
pub struct Explorer {
    pub visible: bool,
    pub root: PathBuf,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub show_hidden: bool,
    expanded: HashSet<PathBuf>,
}

impl Explorer {
    pub fn new(root: PathBuf) -> Self {
        let mut explorer = Explorer {
            visible: false,
            root,
            rows: Vec::new(),
            selected: 0,
            show_hidden: false,
            expanded: HashSet::new(),
        };
        explorer.refresh();
        explorer
    }

    pub fn refresh(&mut self) {
        let mut rows = Vec::new();
        let root = self.root.clone();
        self.push_level(&root, 0, &mut rows);
        self.rows = rows;
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    fn push_level(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        let Ok(entries) = read_dir(dir, self.show_hidden) else {
            return;
        };
        for entry in entries {
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            let path = entry.path.clone();
            rows.push(Row {
                entry,
                depth,
                expanded,
            });
            if expanded {
                self.push_level(&path, depth + 1, rows);
            }
        }
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() - 1;
        self.selected = if delta < 0 {
            self.selected.saturating_sub((-delta) as usize)
        } else {
            (self.selected + delta as usize).min(last)
        };
    }

    /// Expand a collapsed directory, or step into an expanded one.
    pub fn expand(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        if row.entry.is_dir && !row.expanded {
            self.expanded.insert(row.entry.path.clone());
            self.refresh();
        } else if row.entry.is_dir {
            self.move_selection(1);
        }
    }

    /// Collapse an expanded directory, or jump to the parent row.
    pub fn collapse(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let (is_dir, expanded, path, depth) = (
            row.entry.is_dir,
            row.expanded,
            row.entry.path.clone(),
            row.depth,
        );
        if is_dir && expanded {
            self.expanded.remove(&path);
            self.refresh();
            return;
        }
        if depth == 0 {
            return;
        }
        if let Some(parent) = self.rows[..self.selected]
            .iter()
            .rposition(|r| r.depth < depth)
        {
            self.selected = parent;
        }
    }

    /// Toggle the selected row; returns a file path when one should be opened.
    pub fn activate(&mut self) -> Option<PathBuf> {
        let row = self.selected_row()?;
        if !row.entry.is_dir {
            return Some(row.entry.path.clone());
        }
        let path = row.entry.path.clone();
        if row.expanded {
            self.expanded.remove(&path);
        } else {
            self.expanded.insert(path);
        }
        self.refresh();
        None
    }

    /// Reveal `path`, expanding every directory on the way to it.
    pub fn reveal(&mut self, path: &Path) {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return;
        };
        let mut current = self.root.clone();
        for component in relative.components().take(
            relative.components().count().saturating_sub(1), // stop at the parent dir
        ) {
            current = current.join(component);
            self.expanded.insert(current.clone());
        }
        self.refresh();
        if let Some(index) = self.rows.iter().position(|r| r.entry.path == path) {
            self.selected = index;
        }
    }
}
