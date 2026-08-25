//! Literal find-and-replace over a buffer.
//!
//! Deliberately literal rather than regex, for the same reason `/` is: a
//! replacement that quietly reinterprets `.` or `(` is how you lose a file at
//! 2am. Regex lives in `omv-find`, where it searches and never writes.

use std::ops::Range;

use crate::buffer::Buffer;

/// Where a substitution applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The match at or after the cursor, wrapping to the top of the buffer.
    Next,
    /// Every match in the buffer, collapsed into one undo step.
    All,
}

/// Every non-overlapping occurrence of `pattern`, as **char** ranges — the same
/// coordinate space as `Buffer::cursor`, not the byte offsets syntax spans use.
pub fn matches(buffer: &Buffer, pattern: &str) -> Vec<Range<usize>> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let text = buffer.rope.to_string();
    let width = pattern.chars().count();
    text.match_indices(pattern)
        .map(|(byte, _)| {
            let start = buffer.rope.byte_to_char(byte);
            start..start + width
        })
        .collect()
}

/// Index of the match reached by travelling `forward` from char index `from`,
/// wrapping at either end.
///
/// `from` is compared against match *starts*, so a cursor already sitting on a
/// match selects that match instead of skipping it — which is what makes
/// "replace this one" and "show me the first hit" the same operation.
pub fn nearest(matches: &[Range<usize>], from: usize, forward: bool) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    if forward {
        matches.iter().position(|m| m.start >= from).or(Some(0))
    } else {
        matches
            .iter()
            .rposition(|m| m.start < from)
            .or(Some(matches.len() - 1))
    }
}

/// Index of the match containing `char_idx`. Ranges are sorted and
/// non-overlapping, so this is a binary search — it runs per rendered character.
pub fn containing(matches: &[Range<usize>], char_idx: usize) -> Option<usize> {
    matches
        .binary_search_by(|m| {
            if m.end <= char_idx {
                std::cmp::Ordering::Less
            } else if m.start > char_idx {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    fn buffer(text: &str) -> Buffer {
        let mut buf = Buffer::empty();
        buf.rope = Rope::from_str(text);
        buf
    }

    #[test]
    fn matches_are_char_ranges_not_byte_ranges() {
        let buf = buffer("héllo foo, foo\n");
        let hits = matches(&buf, "foo");
        // `é` is two bytes, so a byte offset would report 7 and 12 here.
        assert_eq!(hits, vec![6..9, 11..14], "ranges must be in chars");
    }

    #[test]
    fn overlapping_candidates_only_match_once() {
        let buf = buffer("aaa\n");
        assert_eq!(
            matches(&buf, "aa"),
            vec![0..2],
            "the second `aa` overlaps the first and must not be reported"
        );
    }

    #[test]
    fn an_empty_pattern_matches_nothing() {
        assert!(matches(&buffer("anything\n"), "").is_empty());
    }

    #[test]
    fn nearest_wraps_at_both_ends() {
        let hits = vec![2..5, 10..13];
        assert_eq!(nearest(&hits, 0, true), Some(0));
        assert_eq!(
            nearest(&hits, 2, true),
            Some(0),
            "a cursor on a match picks it"
        );
        assert_eq!(nearest(&hits, 3, true), Some(1));
        assert_eq!(
            nearest(&hits, 99, true),
            Some(0),
            "forward wraps to the top"
        );
        assert_eq!(nearest(&hits, 10, false), Some(0));
        assert_eq!(
            nearest(&hits, 11, false),
            Some(1),
            "from inside a match, backward lands on that match's own start"
        );
        assert_eq!(
            nearest(&hits, 0, false),
            Some(1),
            "backward wraps to the bottom"
        );
        assert_eq!(nearest(&[], 0, true), None);
    }

    #[test]
    fn containing_finds_the_match_under_a_character() {
        let hits = vec![2..5, 10..13];
        assert_eq!(containing(&hits, 1), None);
        assert_eq!(containing(&hits, 2), Some(0));
        assert_eq!(containing(&hits, 4), Some(0));
        assert_eq!(containing(&hits, 5), None, "the end is exclusive");
        assert_eq!(containing(&hits, 12), Some(1));
    }
}
