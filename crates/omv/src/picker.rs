use std::path::PathBuf;

use omv_find::{Match, Matcher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Files,
    Text,
    Buffers,
}

impl PickerKind {
    pub fn title(self) -> &'static str {
        match self {
            PickerKind::Files => " Files ",
            PickerKind::Text => " Search ",
            PickerKind::Buffers => " Buffers ",
        }
    }
}

/// What selecting an item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    File(PathBuf),
    /// A file and a 1-based line to jump to.
    Location(PathBuf, u64),
    Buffer(usize),
}

#[derive(Debug, Clone)]
pub struct Item {
    pub display: String,
    pub payload: Payload,
}

/// A floating fuzzy picker.
///
/// `Files` and `Buffers` fuzzy-match a list held in memory. `Text` re-runs grep
/// on the worker thread per keystroke, so its `items` are already the results
/// and `matches` is just the identity ordering.
pub struct Picker {
    pub kind: PickerKind,
    pub query: String,
    pub items: Vec<Item>,
    pub matches: Vec<Match>,
    pub selected: usize,
    pub loading: bool,
    pub truncated: bool,
    /// Bumped per query so results from a stale background search are dropped.
    pub generation: u64,
    matcher: Matcher,
    haystack: Vec<String>,
}

impl Picker {
    pub fn new(kind: PickerKind) -> Self {
        Picker {
            kind,
            query: String::new(),
            items: Vec::new(),
            matches: Vec::new(),
            selected: 0,
            loading: true,
            truncated: false,
            generation: 0,
            matcher: Matcher::new(),
            haystack: Vec::new(),
        }
    }

    pub fn set_items(&mut self, items: Vec<Item>, truncated: bool) {
        self.haystack = items.iter().map(|i| i.display.clone()).collect();
        self.items = items;
        self.truncated = truncated;
        self.loading = false;
        self.rerank();
    }

    pub fn rerank(&mut self) {
        self.matches = match self.kind {
            // Text results are already ranked by the search itself; fuzzy-filtering
            // them again would fight the regex the user typed.
            PickerKind::Text => (0..self.items.len())
                .map(|index| Match {
                    index,
                    score: 0,
                    positions: Vec::new(),
                })
                .collect(),
            _ => self.matcher.rank(&self.haystack, &self.query, 500),
        };
        self.selected = self.selected.min(self.matches.len().saturating_sub(1));
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.matches.is_empty() {
            return;
        }
        let last = self.matches.len() - 1;
        self.selected = if delta < 0 {
            self.selected.saturating_sub((-delta) as usize)
        } else {
            (self.selected + delta as usize).min(last)
        };
    }

    pub fn selected_payload(&self) -> Option<&Payload> {
        let m = self.matches.get(self.selected)?;
        Some(&self.items.get(m.index)?.payload)
    }

    /// (display string, matched char positions) for each visible row.
    pub fn visible(&self) -> impl Iterator<Item = (&str, &[u32])> {
        self.matches.iter().filter_map(|m| {
            self.items
                .get(m.index)
                .map(|i| (i.display.as_str(), m.positions.as_slice()))
        })
    }
}
