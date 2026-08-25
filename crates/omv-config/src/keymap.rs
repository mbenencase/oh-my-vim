use std::collections::HashMap;

use omv_core::{Action, Mode};

use crate::keys::{Key, KeyCode, parse_sequence};

/// A trie node. `action` is set on nodes that terminate a binding; `children`
/// on nodes that are a prefix of a longer one. Both can be set at once, in which
/// case the resolver waits for the longer binding (see [`Resolver::feed`]).
#[derive(Debug, Default)]
struct Node {
    action: Option<Action>,
    children: HashMap<Key, Node>,
}

impl Node {
    fn insert(&mut self, seq: &[Key], action: Action) {
        match seq.split_first() {
            None => self.action = Some(action),
            Some((head, tail)) => self.children.entry(*head).or_default().insert(tail, action),
        }
    }
}

#[derive(Debug, Default)]
pub struct KeyMap {
    modes: HashMap<Mode, Node>,
}

#[derive(Debug, thiserror::Error)]
pub enum KeyMapError {
    #[error("in {mode:?} binding `{spec}`: {source}")]
    BadKey {
        mode: Mode,
        spec: String,
        source: crate::keys::KeyParseError,
    },
}

impl KeyMap {
    /// Build the trie, substituting `leader` for every `<leader>` placeholder.
    pub fn build(
        bindings: &HashMap<Mode, HashMap<String, Action>>,
        leader: Key,
    ) -> Result<Self, KeyMapError> {
        let mut modes: HashMap<Mode, Node> = HashMap::new();
        for (mode, map) in bindings {
            let root = modes.entry(*mode).or_default();
            for (spec, action) in map {
                let seq = parse_sequence(spec).map_err(|source| KeyMapError::BadKey {
                    mode: *mode,
                    spec: spec.clone(),
                    source,
                })?;
                let seq: Vec<Key> = seq
                    .into_iter()
                    .map(|k| if k.code == KeyCode::Leader { leader } else { k })
                    .collect();
                root.insert(&seq, *action);
            }
        }
        Ok(KeyMap { modes })
    }

    /// Bindings for `mode`, falling back to Visual for VisualLine so a keymap
    /// only has to spell out the line-wise differences.
    fn root(&self, mode: Mode) -> Option<&Node> {
        self.modes.get(&mode).or_else(|| match mode {
            Mode::VisualLine => self.modes.get(&Mode::Visual),
            _ => None,
        })
    }

    pub fn describe(&self, mode: Mode) -> Vec<(String, Action)> {
        let mut out = Vec::new();
        if let Some(root) = self.root(mode) {
            collect(root, &mut String::new(), &mut out);
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

fn collect(node: &Node, prefix: &mut String, out: &mut Vec<(String, Action)>) {
    if let Some(action) = node.action {
        out.push((prefix.clone(), action));
    }
    for (key, child) in &node.children {
        let before = prefix.len();
        prefix.push_str(&key.to_string());
        collect(child, prefix, out);
        prefix.truncate(before);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolve {
    /// A prefix of one or more bindings; waiting for the next key.
    Pending,
    /// A complete binding, plus the count the user typed, if any.
    Action(Action, Option<usize>),
    /// No binding matches. In insert and command mode the UI treats the key as text.
    Unmatched,
}

/// Feeds keys into the trie, accumulating counts and multi-key sequences.
///
/// Known limitation: when a binding is a strict prefix of a longer one (`d` and
/// `dd`), the shorter one only fires once a non-matching key arrives — there is
/// no timeout yet. Avoid such pairs, or add `timeoutlen` later.
#[derive(Debug, Default)]
pub struct Resolver {
    pending: Vec<Key>,
    count: Option<usize>,
}

impl Resolver {
    pub fn feed(&mut self, keymap: &KeyMap, mode: Mode, key: Key) -> Resolve {
        // Counts only make sense where motions and operators do.
        if matches!(mode, Mode::Normal | Mode::Visual | Mode::VisualLine)
            && self.pending.is_empty()
            && let Some(d) = key.as_typed_char().and_then(|c| c.to_digit(10))
        {
            // A leading `0` is the line-start motion, not the start of a count.
            let starting_zero = d == 0 && self.count.is_none();
            if !starting_zero {
                self.count = Some(self.count.unwrap_or(0) * 10 + d as usize);
                return Resolve::Pending;
            }
        }

        self.pending.push(key);
        let Some(root) = keymap.root(mode) else {
            return self.reset_unmatched();
        };

        let mut node = root;
        for k in &self.pending {
            match node.children.get(k) {
                Some(next) => node = next,
                None => return self.reset_unmatched(),
            }
        }

        if !node.children.is_empty() {
            return Resolve::Pending;
        }
        match node.action {
            Some(action) => {
                let count = self.count.take();
                self.pending.clear();
                Resolve::Action(action, count)
            }
            None => self.reset_unmatched(),
        }
    }

    fn reset_unmatched(&mut self) -> Resolve {
        self.pending.clear();
        self.count = None;
        Resolve::Unmatched
    }

    /// What to show in the status line's right corner, vim-style.
    pub fn pending_display(&self) -> String {
        let mut s = self.count.map(|c| c.to_string()).unwrap_or_default();
        for k in &self.pending {
            s.push_str(&k.to_string());
        }
        s
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.count = None;
    }
}
