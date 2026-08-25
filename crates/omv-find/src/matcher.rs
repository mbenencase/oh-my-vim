use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Utf32Str};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Index into the haystack slice that was ranked.
    pub index: usize,
    pub score: u32,
    /// Char positions that matched, for highlighting in the picker.
    pub positions: Vec<u32>,
}

/// Fuzzy ranking over a list of strings, using the same matcher Helix uses.
pub struct Matcher {
    inner: nucleo::Matcher,
    buf: Vec<char>,
    positions: Vec<u32>,
}

impl Matcher {
    pub fn new() -> Self {
        Matcher {
            inner: nucleo::Matcher::new(Config::DEFAULT.match_paths()),
            buf: Vec::new(),
            positions: Vec::new(),
        }
    }

    /// Rank `items` against `query`, best first, capped at `limit`.
    /// An empty query keeps the original order rather than ranking everything equally.
    pub fn rank(&mut self, items: &[String], query: &str, limit: usize) -> Vec<Match> {
        if query.is_empty() {
            return items
                .iter()
                .enumerate()
                .take(limit)
                .map(|(index, _)| Match {
                    index,
                    score: 0,
                    positions: Vec::new(),
                })
                .collect();
        }

        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut hits: Vec<Match> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            self.buf.clear();
            self.positions.clear();
            let haystack = Utf32Str::new(item, &mut self.buf);
            if let Some(score) = pattern.indices(haystack, &mut self.inner, &mut self.positions) {
                self.positions.sort_unstable();
                self.positions.dedup();
                hits.push(Match {
                    index,
                    score,
                    positions: self.positions.clone(),
                });
            }
        }
        // Ties broken by original order so results don't shuffle as you type.
        hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
        hits.truncate(limit);
        hits
    }
}

impl Default for Matcher {
    fn default() -> Self {
        Matcher::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<String> {
        [
            "src/main.rs",
            "src/editor/buffer.rs",
            "docs/readme.md",
            "src/ui/mod.rs",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn subsequence_matches_rank_above_nothing() {
        let mut m = Matcher::new();
        let items = items();
        let hits = m.rank(&items, "srcmain", 10);
        assert_eq!(items[hits[0].index], "src/main.rs");
    }

    #[test]
    fn non_matching_query_returns_nothing() {
        let mut m = Matcher::new();
        assert!(m.rank(&items(), "zzzzqqq", 10).is_empty());
    }

    #[test]
    fn empty_query_preserves_order() {
        let mut m = Matcher::new();
        let hits = m.rank(&items(), "", 2);
        assert_eq!(hits.iter().map(|h| h.index).collect::<Vec<_>>(), vec![0, 1]);
    }
}
