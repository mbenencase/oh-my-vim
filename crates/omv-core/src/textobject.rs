use std::ops::Range;

use crate::buffer::Buffer;
use crate::movement::{CharClass, class_of, surrounding_pair, surrounding_quote};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Word,
    Paren,
    Bracket,
    Brace,
    Quote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The contents, excluding delimiters.
    Inside,
    /// Contents plus delimiters (or, for a word, its trailing whitespace).
    Around,
}

/// Char range covered by a text object at the cursor, or `None` when the cursor
/// isn't inside one (e.g. `di(` with no enclosing parens — a no-op, not an error).
pub fn range(buf: &Buffer, kind: ObjectKind, scope: Scope) -> Option<Range<usize>> {
    match kind {
        ObjectKind::Word => word_range(buf, scope),
        ObjectKind::Paren => pair_range(buf, '(', ')', scope),
        ObjectKind::Bracket => pair_range(buf, '[', ']', scope),
        ObjectKind::Brace => pair_range(buf, '{', '}', scope),
        ObjectKind::Quote => quote_range(buf, scope),
    }
}

fn word_range(buf: &Buffer, scope: Scope) -> Option<Range<usize>> {
    let len = buf.rope.len_chars();
    if len == 0 {
        return None;
    }
    let at = buf.cursor.min(len - 1);
    let line = buf.rope.char_to_line(at);
    let line_start = buf.line_start(line);
    let line_end = buf.line_end(line);
    if line_start == line_end {
        return None;
    }

    let class = class_of(buf.rope.char(at));
    let mut start = at;
    while start > line_start && class_of(buf.rope.char(start - 1)) == class {
        start -= 1;
    }
    let mut end = at + 1;
    while end < line_end && class_of(buf.rope.char(end)) == class {
        end += 1;
    }

    if scope == Scope::Around {
        let mut extended = end;
        while extended < line_end && class_of(buf.rope.char(extended)) == CharClass::Whitespace {
            extended += 1;
        }
        if extended > end {
            return Some(start..extended);
        }
        // No trailing whitespace, so take the leading run instead — `daw` on the
        // last word of a line shouldn't leave a dangling space behind.
        let mut lead = start;
        while lead > line_start && class_of(buf.rope.char(lead - 1)) == CharClass::Whitespace {
            lead -= 1;
        }
        return Some(lead..end);
    }
    Some(start..end)
}

fn pair_range(buf: &Buffer, open: char, close: char, scope: Scope) -> Option<Range<usize>> {
    let (start, end) = surrounding_pair(buf, buf.cursor, open, close)?;
    Some(match scope {
        Scope::Inside => (start + 1)..end,
        Scope::Around => start..(end + 1),
    })
}

fn quote_range(buf: &Buffer, scope: Scope) -> Option<Range<usize>> {
    let (start, end) = surrounding_quote(buf, buf.cursor, '"')
        .or_else(|| surrounding_quote(buf, buf.cursor, '\''))?;
    Some(match scope {
        Scope::Inside => (start + 1)..end,
        Scope::Around => start..(end + 1),
    })
}
