use crate::buffer::{Buffer, Position};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    Whitespace,
    Word,
    Punctuation,
}

pub fn class_of(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

fn char_at(buf: &Buffer, idx: usize) -> Option<char> {
    (idx < buf.rope.len_chars()).then(|| buf.rope.char(idx))
}

pub fn left(buf: &Buffer, count: usize) -> usize {
    let pos = buf.cursor_position();
    let start = buf.line_start(pos.line);
    buf.cursor.saturating_sub(count).max(start)
}

pub fn right(buf: &Buffer, count: usize, past_end: bool) -> usize {
    let pos = buf.cursor_position();
    let max = buf.max_cursor_on_line(pos.line, past_end);
    (buf.cursor + count).min(max)
}

/// Vertical motion honours `goal_column` so a run of `j` through short lines
/// returns to the original column rather than collapsing to the shortest one.
pub fn vertical(buf: &Buffer, delta: isize, past_end: bool) -> (usize, usize) {
    let pos = buf.cursor_position();
    let goal = buf.goal_column.unwrap_or(pos.column);
    #[expect(
        clippy::cast_sign_loss,
        reason = "each branch is guarded by the sign test, so neither cast can lose one"
    )]
    let target = if delta < 0 {
        pos.line.saturating_sub((-delta) as usize)
    } else {
        (pos.line + delta as usize).min(buf.last_line())
    };
    let start = buf.line_start(target);
    let max = buf.max_cursor_on_line(target, past_end);
    ((start + goal).min(max), goal)
}

pub fn line_start(buf: &Buffer) -> usize {
    buf.line_start(buf.cursor_position().line)
}

pub fn line_first_non_blank(buf: &Buffer) -> usize {
    let line = buf.cursor_position().line;
    let start = buf.line_start(line);
    let end = buf.line_end(line);
    (start..end)
        .find(|&i| !buf.rope.char(i).is_whitespace())
        .unwrap_or(start)
}

pub fn line_end(buf: &Buffer, past_end: bool) -> usize {
    buf.max_cursor_on_line(buf.cursor_position().line, past_end)
}

pub fn goto_line(buf: &Buffer, line: usize) -> usize {
    let line = line.min(buf.last_line());
    let start = buf.line_start(line);
    let end = buf.line_end(line);
    (start..end)
        .find(|&i| !buf.rope.char(i).is_whitespace())
        .unwrap_or(start)
}

/// `w` — start of the next word. Crosses lines; an empty line counts as a word.
pub fn word_forward(buf: &Buffer, count: usize) -> usize {
    let len = buf.rope.len_chars();
    let mut idx = buf.cursor;
    for _ in 0..count {
        if idx >= len {
            break;
        }
        let start_class = class_of(buf.rope.char(idx));
        if start_class != CharClass::Whitespace {
            while idx < len && class_of(buf.rope.char(idx)) == start_class {
                idx += 1;
            }
        }
        while idx < len && class_of(buf.rope.char(idx)) == CharClass::Whitespace {
            // A blank line is its own destination, like vim.
            if buf.rope.char(idx) == '\n'
                && buf.line_len(buf.rope.char_to_line(idx + 1).min(buf.last_line())) == 0
            {
                return (idx + 1).min(len);
            }
            idx += 1;
        }
    }
    idx.min(len.saturating_sub(usize::from(len > 0)))
}

/// `b` — start of the previous word.
pub fn word_backward(buf: &Buffer, count: usize) -> usize {
    let mut idx = buf.cursor;
    for _ in 0..count {
        if idx == 0 {
            break;
        }
        idx -= 1;
        while idx > 0 && class_of(buf.rope.char(idx)) == CharClass::Whitespace {
            idx -= 1;
        }
        let class = class_of(buf.rope.char(idx));
        while idx > 0 && class_of(buf.rope.char(idx - 1)) == class {
            idx -= 1;
        }
    }
    idx
}

/// `e` — end of the current word, or the next one if already there.
pub fn word_end(buf: &Buffer, count: usize) -> usize {
    let len = buf.rope.len_chars();
    let mut idx = buf.cursor;
    for _ in 0..count {
        if idx + 1 >= len {
            break;
        }
        idx += 1;
        while idx < len && class_of(buf.rope.char(idx)) == CharClass::Whitespace {
            idx += 1;
        }
        if idx >= len {
            break;
        }
        let class = class_of(buf.rope.char(idx));
        while idx + 1 < len && class_of(buf.rope.char(idx + 1)) == class {
            idx += 1;
        }
    }
    idx.min(len.saturating_sub(1))
}

const PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];

/// `%` — jump between a bracket and its partner, respecting nesting.
pub fn matching_pair(buf: &Buffer, from: usize) -> Option<usize> {
    let c = char_at(buf, from)?;
    if let Some((open, close)) = PAIRS.iter().copied().find(|(o, _)| *o == c) {
        return scan_forward(buf, from + 1, open, close);
    }
    if let Some((open, close)) = PAIRS.iter().copied().find(|(_, cl)| *cl == c) {
        return scan_backward(buf, from, open, close);
    }
    None
}

fn scan_forward(buf: &Buffer, from: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 1usize;
    for idx in from..buf.rope.len_chars() {
        let c = buf.rope.char(idx);
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn scan_backward(buf: &Buffer, from: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 1usize;
    for idx in (0..from).rev() {
        let c = buf.rope.char(idx);
        if c == close {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

/// Nearest enclosing `open`/`close` pair around `at`, as char indices of the delimiters.
pub fn surrounding_pair(
    buf: &Buffer,
    at: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let len = buf.rope.len_chars();
    let at = at.min(len.saturating_sub(1));

    let start = if char_at(buf, at) == Some(open) {
        at
    } else {
        let mut depth = 1usize;
        let mut found = None;
        for idx in (0..at).rev() {
            let c = buf.rope.char(idx);
            if c == close {
                depth += 1;
            } else if c == open {
                depth -= 1;
                if depth == 0 {
                    found = Some(idx);
                    break;
                }
            }
        }
        found?
    };
    let end = scan_forward(buf, start + 1, open, close)?;
    Some((start, end))
}

/// Nearest enclosing quote pair on the cursor's line — quotes don't nest, so
/// this pairs them off from the start of the line rather than counting depth.
pub fn surrounding_quote(buf: &Buffer, at: usize, quote: char) -> Option<(usize, usize)> {
    let line = buf
        .rope
        .char_to_line(at.min(buf.rope.len_chars().saturating_sub(1)));
    let start = buf.line_start(line);
    let end = buf.line_end(line);
    let mut open: Option<usize> = None;
    for idx in start..end {
        if buf.rope.char(idx) != quote {
            continue;
        }
        match open {
            None => open = Some(idx),
            Some(o) => {
                if (o..=idx).contains(&at) {
                    return Some((o, idx));
                }
                open = None;
            }
        }
    }
    None
}

pub fn position_of(buf: &Buffer, idx: usize) -> Position {
    buf.char_to_position(idx)
}
