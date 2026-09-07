//! Line-oriented Markdown structuring for the render pane.
//!
//! Unlike [`crate::Highlighter`], this does not produce byte offsets into the
//! source: the render pane draws *collapsed* Markdown, so `**bold**` has to
//! arrive as the four characters `bold` carrying a bold kind. Offsets into the
//! raw text cannot express that, which is why this is a separate model in the
//! same crate rather than another `HighlightKind` or a tree-sitter grammar.
//!
//! # The invariant
//!
//! [`render`] returns **exactly one [`MarkdownLine`] per source line**, where a
//! source line is one element of `text.split('\n')` (matching how the buffer's
//! rope counts lines, including the empty last line after a trailing newline).
//! Nothing here reflows, wraps, merges or drops a line. That makes the mapping
//! from source line to rendered line the identity, so a renderer showing the
//! two side by side keeps them aligned with the source window's own scroll and
//! needs no scroll machinery of its own. Do not break it.
//!
//! # v1 scope
//!
//! Headings (by level), bold, italic, ordered and unordered list items, fenced
//! code blocks, and horizontal rules. No tables, blockquotes, links, images,
//! footnotes, HTML, or highlighting of a fence's contents.
//!
//! Malformed input degrades, it never panics: an unclosed fence leaves the rest
//! of the document as code (the fence is what the author typed, so that is the
//! visually stable reading), and an emphasis marker that never finds its
//! partner stays in the output as literal text rather than swallowing the tail
//! of the line.

/// A heading level, guaranteed to be in `1..=6`.
///
/// A newtype rather than a bare `u8` because the payload of a public enum
/// variant is constructible by anyone: a theme holding one style per level
/// would panic indexing on a `Heading(0)` or `Heading(9)` that no parser here
/// can produce but nothing else forbade. [`HeadingLevel::index`] is therefore
/// total by construction rather than by the caller remembering to check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HeadingLevel(u8);

impl HeadingLevel {
    /// The deepest level Markdown has.
    pub const MAX: u8 = 6;

    /// Clamp into `1..=6`. Named for what it does: `####### x` is a level-6
    /// heading here, not a rejected one.
    #[must_use]
    pub fn clamped(level: u8) -> Self {
        HeadingLevel(level.clamp(1, Self::MAX))
    }

    /// The level, in `1..=6`.
    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }

    /// A zero-based index into a table of [`HeadingLevel::MAX`] entries,
    /// always in `0..HeadingLevel::MAX`.
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0 - 1)
    }
}

/// What produced a rendered span.
///
/// One kind per span, because the consumer turns each span into exactly one
/// terminal style. Two consequences of that flatness are deliberate and
/// documented on [`render`]: emphasis inside a heading is absorbed into the
/// heading, and simultaneous bold+italic renders as [`MarkdownKind::Bold`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkdownKind {
    /// Paragraph text, and the text of a list item.
    Plain,
    /// A heading and its `#` marker.
    Heading(HeadingLevel),
    Bold,
    Italic,
    /// The normalised bullet of an unordered item, including its indentation.
    ListMarker,
    /// The normalised `N. ` of an ordered item, including its indentation.
    OrderedMarker,
    /// A line inside a fenced code block, fence delimiters included.
    Code,
    /// A horizontal rule, drawn as box-drawing characters.
    Rule,
}

/// A run of rendered text sharing one style. Never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSpan {
    pub text: String,
    pub kind: MarkdownKind,
}

/// One rendered line. Empty `spans` means the line renders blank.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownLine {
    pub spans: Vec<MarkdownSpan>,
}

impl MarkdownLine {
    /// The line's rendered text, markers included and markup collapsed.
    #[must_use]
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// Structure `text` into one [`MarkdownLine`] per source line.
///
/// Pure: no I/O, no globals, no allocation the caller does not receive.
///
/// Two collapsing rules are worth knowing, both consequences of a span carrying
/// a single kind:
///
/// - Emphasis inside a heading is absorbed into `Heading(level)`. The markers
///   are still removed, so `# a **b** c` renders `# a b c` styled uniformly as
///   a heading. A heading is already emphatic, and this is the only way the
///   flat kind keeps *all* of a heading line at heading style.
/// - Bold nested in italic (or `***both***`) renders as `Bold`. v1's construct
///   list names bold and italic separately, not their combination.
#[must_use]
pub fn render(text: &str) -> Vec<MarkdownLine> {
    let mut lines = Vec::new();
    let mut fence: Option<Fence> = None;
    // split('\n') rather than lines(): the rope counts the empty segment after a
    // trailing newline as a line, and the identity mapping above depends on
    // agreeing with it.
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        lines.push(render_line(line, &mut fence));
    }
    lines
}

/// An open code fence: the character that opened it and how many of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fence {
    ch: char,
    len: usize,
}

fn render_line(line: &str, fence: &mut Option<Fence>) -> MarkdownLine {
    if let Some(open) = *fence {
        if closes_fence(line, open) {
            *fence = None;
        }
        return verbatim(line, MarkdownKind::Code);
    }
    if let Some(open) = opens_fence(line) {
        *fence = Some(open);
        return verbatim(line, MarkdownKind::Code);
    }
    // Before the unordered item, because `---` and `***` match both and a rule
    // is the more specific reading.
    if let Some(width) = horizontal_rule(line) {
        return verbatim(&"\u{2500}".repeat(width), MarkdownKind::Rule);
    }
    if let Some((level, rest)) = heading(line) {
        let mut spans = vec![MarkdownSpan {
            text: format!("{} ", "#".repeat(usize::from(level.get()))),
            kind: MarkdownKind::Heading(level),
        }];
        spans.extend(inline_spans(rest, MarkdownKind::Heading(level), true));
        return MarkdownLine { spans };
    }
    if let Some((marker, rest)) = list_item(line) {
        let mut spans = vec![marker];
        spans.extend(inline_spans(rest, MarkdownKind::Plain, false));
        return MarkdownLine { spans };
    }
    MarkdownLine {
        spans: inline_spans(line, MarkdownKind::Plain, false),
    }
}

/// A line drawn as a single span of `text`. Empty text yields no span, since a
/// span is never empty.
fn verbatim(text: &str, kind: MarkdownKind) -> MarkdownLine {
    let spans = if text.is_empty() {
        Vec::new()
    } else {
        vec![MarkdownSpan {
            text: text.to_string(),
            kind,
        }]
    };
    MarkdownLine { spans }
}

fn opens_fence(line: &str) -> Option<Fence> {
    let trimmed = line.trim_start();
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|&c| c == ch).count();
    (len >= 3).then_some(Fence { ch, len })
}

fn closes_fence(line: &str, open: Fence) -> bool {
    let trimmed = line.trim();
    let len = trimmed.chars().take_while(|&c| c == open.ch).count();
    len >= open.len && len == trimmed.chars().count()
}

/// Width of a horizontal rule in marker characters, or `None`.
///
/// The width follows what the author typed rather than a constant: the parser
/// cannot know the pane's width, and mirroring the source keeps the choice with
/// the person who can see it. A width-aware renderer may still key off
/// [`MarkdownKind::Rule`] and fill its own line.
fn horizontal_rule(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    let ch = trimmed.chars().next()?;
    if ch != '-' && ch != '*' && ch != '_' {
        return None;
    }
    let mut count = 0;
    for c in trimmed.chars() {
        if c == ch {
            count += 1;
        } else if !c.is_whitespace() {
            return None;
        }
    }
    (count >= 3).then_some(count)
}

/// `(level, text after the marker)` for an ATX heading.
fn heading(line: &str) -> Option<(HeadingLevel, &str)> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if hashes == 0 {
        return None;
    }
    let rest = trimmed.get(hashes..)?;
    // `#tag` is not a heading; `#` alone on a line is.
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let level = HeadingLevel::clamped(u8::try_from(hashes).unwrap_or(HeadingLevel::MAX));
    Some((level, rest.trim_start()))
}

/// `(marker span, text after the marker)` for an ordered or unordered item.
///
/// The bullet is normalised (`-`, `*` and `+` are one construct, so they get
/// one glyph) while a heading keeps its literal `#`s, where the repetition
/// *is* the information and stays readable with no colour at all.
fn list_item(line: &str) -> Option<(MarkdownSpan, &str)> {
    let trimmed = line.trim_start();
    let indent = line.get(..line.len() - trimmed.len())?;

    if trimmed.starts_with(['-', '*', '+']) {
        let rest = trimmed.get(1..)?;
        if !rest.is_empty() && !rest.starts_with(' ') {
            return None;
        }
        let span = MarkdownSpan {
            text: format!("{indent}\u{2022} "),
            kind: MarkdownKind::ListMarker,
        };
        return Some((span, rest.strip_prefix(' ').unwrap_or(rest)));
    }

    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let after = trimmed.get(digits..)?;
    let rest = after
        .strip_prefix('.')
        .or_else(|| after.strip_prefix(')'))?;
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let number = trimmed.get(..digits)?;
    let span = MarkdownSpan {
        text: format!("{indent}{number}. "),
        kind: MarkdownKind::OrderedMarker,
    };
    Some((span, rest.strip_prefix(' ').unwrap_or(rest)))
}

// --- inline emphasis ---------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelimKind {
    /// `**` or `__`.
    Strong,
    /// `*` or `_`.
    Em,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelimState {
    /// Never found a partner, so it renders as the characters the author typed.
    Literal,
    Open,
    Close,
}

#[derive(Debug, Clone, Copy)]
enum Token {
    Text {
        start: usize,
        end: usize,
    },
    Delim {
        start: usize,
        end: usize,
        kind: DelimKind,
        can_open: bool,
        can_close: bool,
        state: DelimState,
    },
}

/// Split `text` into styled spans, collapsing matched emphasis markers.
///
/// `base` is the kind unemphasised text takes. `absorb` folds emphasis back
/// into `base` instead of emitting `Bold`/`Italic`; headings use it.
fn inline_spans(text: &str, base: MarkdownKind, absorb: bool) -> Vec<MarkdownSpan> {
    let mut tokens = tokenize(text);
    match_delims(&mut tokens);
    emit(text, &tokens, base, absorb)
}

fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut text_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        // Every byte of a multi-byte character is >= 0x80, so a byte scan can
        // never split one and never mistake one for a marker.
        if byte != b'*' && byte != b'_' {
            i += 1;
            continue;
        }
        if text_start < i {
            tokens.push(Token::Text {
                start: text_start,
                end: i,
            });
        }
        let run_start = i;
        while i < bytes.len() && bytes[i] == byte {
            i += 1;
        }

        let before = text.get(..run_start).and_then(|s| s.chars().next_back());
        let after = text.get(i..).and_then(|s| s.chars().next());
        let mut can_open = after.is_some_and(|c| !c.is_whitespace());
        let mut can_close = before.is_some_and(|c| !c.is_whitespace());
        if byte == b'_' {
            // `snake_case` must not emphasise. `*` keeps the looser rule because
            // it does not appear inside identifiers the way `_` does.
            can_open &= !before.is_some_and(char::is_alphanumeric);
            can_close &= !after.is_some_and(char::is_alphanumeric);
        }

        // A closing run nests inside-out -- in `***x***` the single `*` closes
        // the italic before the pair closes the bold -- so a closer spends its
        // odd marker first and an opener spends it last.
        let mut pos = run_start;
        let mut push = |start: usize, len: usize, kind: DelimKind| {
            tokens.push(Token::Delim {
                start,
                end: start + len,
                kind,
                can_open,
                can_close,
                state: DelimState::Literal,
            });
        };
        if can_close && (i - run_start) % 2 == 1 {
            push(pos, 1, DelimKind::Em);
            pos += 1;
        }
        while pos + 2 <= i {
            push(pos, 2, DelimKind::Strong);
            pos += 2;
        }
        if pos < i {
            push(pos, 1, DelimKind::Em);
        }

        text_start = i;
    }

    if text_start < bytes.len() {
        tokens.push(Token::Text {
            start: text_start,
            end: bytes.len(),
        });
    }
    tokens
}

/// Pair openers with closers. Anything left unpaired keeps `Literal`, which is
/// how "a stray marker is never swallowed" is enforced rather than remembered.
fn match_delims(tokens: &mut [Token]) {
    let mut open: Vec<usize> = Vec::new();
    for idx in 0..tokens.len() {
        let Token::Delim {
            kind,
            can_open,
            can_close,
            ..
        } = tokens[idx]
        else {
            continue;
        };

        if can_close {
            let found = open.iter().rposition(
                |&o| matches!(tokens[o], Token::Delim { kind: other, .. } if other == kind),
            );
            if let Some(at) = found {
                let opener = open[at];
                // Openers above the match were crossed by it and never found a
                // partner of their own; dropping them leaves them Literal.
                open.truncate(at);
                set_state(&mut tokens[opener], DelimState::Open);
                set_state(&mut tokens[idx], DelimState::Close);
                continue;
            }
        }
        if can_open {
            open.push(idx);
        }
    }
}

fn set_state(token: &mut Token, to: DelimState) {
    if let Token::Delim { state, .. } = token {
        *state = to;
    }
}

fn emit(text: &str, tokens: &[Token], base: MarkdownKind, absorb: bool) -> Vec<MarkdownSpan> {
    let mut spans: Vec<MarkdownSpan> = Vec::new();
    let mut buf = String::new();
    let mut bold = 0usize;
    let mut italic = 0usize;

    let kind_of = |bold: usize, italic: usize| {
        if absorb || (bold == 0 && italic == 0) {
            base
        } else if bold > 0 {
            MarkdownKind::Bold
        } else {
            MarkdownKind::Italic
        }
    };

    for token in tokens {
        match *token {
            Token::Text { start, end } => buf.push_str(text.get(start..end).unwrap_or_default()),
            Token::Delim {
                start,
                end,
                kind,
                state,
                ..
            } => match state {
                DelimState::Literal => buf.push_str(text.get(start..end).unwrap_or_default()),
                DelimState::Open => {
                    flush(&mut spans, &mut buf, kind_of(bold, italic));
                    match kind {
                        DelimKind::Strong => bold += 1,
                        DelimKind::Em => italic += 1,
                    }
                }
                DelimState::Close => {
                    flush(&mut spans, &mut buf, kind_of(bold, italic));
                    match kind {
                        DelimKind::Strong => bold = bold.saturating_sub(1),
                        DelimKind::Em => italic = italic.saturating_sub(1),
                    }
                }
            },
        }
    }
    flush(&mut spans, &mut buf, kind_of(bold, italic));
    spans
}

/// Append `buf` as a span, merging into the previous one when the kind is the
/// same. Empty text produces nothing, which is what keeps spans non-empty.
fn flush(spans: &mut Vec<MarkdownSpan>, buf: &mut String, kind: MarkdownKind) {
    if buf.is_empty() {
        return;
    }
    match spans.last_mut() {
        Some(last) if last.kind == kind => last.text.push_str(buf),
        _ => spans.push(MarkdownSpan {
            text: std::mem::take(buf),
            kind,
        }),
    }
    buf.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every v1 construct, once, plus the blank lines a real document has.
    const DOCUMENT: &str = "\
# Title

Some **bold** and *italic* prose.

- first
- second

1. one
2. two

```rust
fn main() {}
```

---
";

    fn kinds(line: &MarkdownLine) -> Vec<MarkdownKind> {
        line.spans.iter().map(|s| s.kind).collect()
    }

    fn heading_kind(level: u8) -> MarkdownKind {
        MarkdownKind::Heading(HeadingLevel::clamped(level))
    }

    #[test]
    fn headings_carry_their_level() {
        let lines = render("# one\n## two\n###### six\n#deep\n");
        assert_eq!(
            kinds(&lines[0]),
            vec![heading_kind(1), heading_kind(1)],
            "a heading's marker and its text are both styled as the heading"
        );
        assert_eq!(
            lines[0].text(),
            "# one",
            "the `#` marker is shown, not dropped"
        );
        assert!(
            lines[1].spans.iter().all(|s| s.kind == heading_kind(2)),
            "the level distinguishes `##` from `#`; the render pane styles by it"
        );
        assert!(
            lines[2].spans.iter().all(|s| s.kind == heading_kind(6)),
            "six hashes is the deepest level Markdown has"
        );
        assert_eq!(
            kinds(&lines[3]),
            vec![MarkdownKind::Plain],
            "`#` needs a space after it, or `#deep` would make every hashtag a heading"
        );
    }

    #[test]
    fn a_seventh_hash_clamps_to_level_six() {
        let lines = render("####### too deep");
        assert!(
            lines[0]
                .spans
                .iter()
                .all(|s| s.kind == heading_kind(HeadingLevel::MAX)),
            "levels are clamped so `Heading` is always 1..=6 and a theme can index six styles"
        );
        assert_eq!(
            lines[0].text(),
            "###### too deep",
            "the displayed marker shows the clamped level, not the seven hashes"
        );
    }

    #[test]
    fn a_heading_level_can_only_index_inside_a_six_entry_table() {
        let table = [0u8; HeadingLevel::MAX as usize];
        for raw in [u8::MIN, 1, 3, 6, 7, u8::MAX] {
            let level = HeadingLevel::clamped(raw);
            assert!(
                (1..=HeadingLevel::MAX).contains(&level.get()),
                "clamped({raw}) escaped 1..=6, and a theme indexing by level would panic"
            );
            assert!(
                table.get(level.index()).is_some(),
                "index() must be total against a table of HeadingLevel::MAX entries"
            );
        }
    }

    #[test]
    fn bold_and_italic_collapse_their_markers() {
        let lines = render("a **b** c *d* e __f__ g _h_ i");
        assert_eq!(
            lines[0].text(),
            "a b c d e f g h i",
            "the render is collapsed: markers are removed, not shown"
        );
        let emphasised: Vec<_> = lines[0]
            .spans
            .iter()
            .filter(|s| s.kind != MarkdownKind::Plain)
            .map(|s| (s.text.as_str(), s.kind))
            .collect();
        assert_eq!(
            emphasised,
            vec![
                ("b", MarkdownKind::Bold),
                ("d", MarkdownKind::Italic),
                ("f", MarkdownKind::Bold),
                ("h", MarkdownKind::Italic),
            ],
            "`**`/`__` are bold and `*`/`_` italic, and only the enclosed text carries the kind"
        );
    }

    #[test]
    fn emphasis_inside_a_heading_is_absorbed_into_the_heading() {
        let lines = render("## a **b** c");
        assert_eq!(
            lines[0].text(),
            "## a b c",
            "markers are still collapsed inside a heading"
        );
        assert!(
            lines[0].spans.iter().all(|s| s.kind == heading_kind(2)),
            "one kind per span means bold inside a heading would otherwise lose the heading's \
             style; a heading is already emphatic, so the emphasis folds into it"
        );
    }

    #[test]
    fn an_underscore_inside_a_word_is_not_emphasis() {
        let lines = render("call render_line_for now");
        assert_eq!(
            kinds(&lines[0]),
            vec![MarkdownKind::Plain],
            "`snake_case` in prose is an identifier, not emphasis"
        );
        assert_eq!(
            lines[0].text(),
            "call render_line_for now",
            "and it keeps its underscores"
        );
    }

    #[test]
    fn ordered_and_unordered_items_keep_a_marker() {
        let lines = render("- dash\n* star\n+ plus\n  - nested\n3. three\n4) four");
        for (i, expected) in ["\u{2022} dash", "\u{2022} star", "\u{2022} plus"]
            .iter()
            .enumerate()
        {
            assert_eq!(
                lines[i].text(),
                *expected,
                "`-`, `*` and `+` are one construct, so they normalise to one bullet"
            );
            assert_eq!(
                lines[i].spans[0].kind,
                MarkdownKind::ListMarker,
                "the bullet is its own span so a theme can style it apart from the text"
            );
        }
        assert_eq!(
            lines[3].text(),
            "  \u{2022} nested",
            "indentation belongs to the marker, so nesting survives the render"
        );
        assert_eq!(
            lines[4].text(),
            "3. three",
            "an ordered item keeps the author's number"
        );
        assert_eq!(
            lines[5].spans[0].kind,
            MarkdownKind::OrderedMarker,
            "`N)` is the same construct as `N.` and normalises to it"
        );
        assert_eq!(
            lines[5].text(),
            "4. four",
            "the delimiter normalises, the number does not"
        );
    }

    #[test]
    fn fenced_code_block_lines_are_kept_verbatim() {
        let lines = render("```rust\nlet x = **not bold**;\n```\nafter");
        assert_eq!(
            lines[1].text(),
            "let x = **not bold**;",
            "inline emphasis is not applied inside a fence -- code means what it says"
        );
        for line in lines.iter().take(3) {
            assert_eq!(
                kinds(line),
                vec![MarkdownKind::Code],
                "the fence delimiters are part of the block, so the reader sees where it ends"
            );
        }
        assert_eq!(
            kinds(&lines[3]),
            vec![MarkdownKind::Plain],
            "the closing fence ends the block"
        );
    }

    #[test]
    fn an_unclosed_fence_still_yields_every_line() {
        let text = "before\n```\none\ntwo\n";
        let lines = render(text);
        assert_eq!(
            lines.len(),
            text.split('\n').count(),
            "a fence with no closer must not swallow or drop a line"
        );
        assert_eq!(
            kinds(&lines[0]),
            vec![MarkdownKind::Plain],
            "text before the fence is unaffected"
        );
        for line in &lines[1..4] {
            assert_eq!(
                kinds(line),
                vec![MarkdownKind::Code],
                "an unclosed fence runs to the end of the document: the fence is what the \
                 author typed, so that is the stable reading"
            );
        }
        assert!(
            render("```").len() == 1,
            "a fence opened on the last line is not a panic"
        );
    }

    #[test]
    fn a_horizontal_rule_becomes_a_rule_line() {
        for source in ["---", "***", "___", "- - -", "  ----------"] {
            let lines = render(source);
            assert_eq!(
                kinds(&lines[0]),
                vec![MarkdownKind::Rule],
                "three or more `-`, `*` or `_` alone on a line is a rule, spaces allowed"
            );
            assert!(
                lines[0].text().chars().all(|c| c == '\u{2500}'),
                "a rule renders as box drawing, not as the characters that declared it"
            );
        }
        assert_eq!(
            render("--").len(),
            1,
            "two markers are not a rule, but are still one rendered line"
        );
        assert_eq!(
            kinds(&render("- item")[0]),
            vec![MarkdownKind::ListMarker, MarkdownKind::Plain],
            "a rule must not steal a one-item list"
        );
    }

    #[test]
    fn plain_prose_renders_as_plain_text() {
        let lines = render("just some words\n\nand more");
        assert_eq!(
            kinds(&lines[0]),
            vec![MarkdownKind::Plain],
            "prose with no markup is one plain span"
        );
        assert_eq!(
            lines[0].text(),
            "just some words",
            "and its text is unchanged"
        );
        assert!(
            lines[1].spans.is_empty(),
            "a blank line renders blank: a span is never empty, so it has none"
        );
    }

    #[test]
    fn a_stray_emphasis_marker_stays_literal_text() {
        for source in ["***", "**a*", "a * b", "_", "**unclosed bold", "*a**b*"] {
            let lines = render(source);
            assert_eq!(lines.len(), 1, "a malformed line is still exactly one line");
            let rendered = lines[0].text();
            for word in source
                .split_whitespace()
                .filter(|w| !w.chars().all(|c| c == '*' || c == '_'))
            {
                let bare = word.trim_matches(['*', '_']);
                assert!(
                    rendered.contains(bare),
                    "`{source}` lost `{bare}`: an unmatched marker must not swallow the line"
                );
            }
        }
        assert_eq!(
            render("**a*")[0].text(),
            "**a*",
            "with no partner on either marker, both stay the characters the author typed"
        );
        assert_eq!(
            render("a_b_c")[0].text(),
            "a_b_c",
            "and an intraword underscore is never a marker to begin with"
        );
    }

    #[test]
    fn every_source_line_produces_exactly_one_rendered_line() {
        for source in [
            DOCUMENT,
            "",
            "\n",
            "no trailing newline",
            "trailing\n",
            "crlf\r\nlines\r\n",
            "```\nunclosed",
            "***\n**\n*",
        ] {
            assert_eq!(
                render(source).len(),
                source.split('\n').count(),
                "the render pane's scroll is the source window's scroll, which only holds \
                 while the line mapping is the identity: {source:?}"
            );
        }
    }

    #[test]
    fn no_rendered_span_is_ever_empty() {
        for line in render(DOCUMENT) {
            for span in &line.spans {
                assert!(
                    !span.text.is_empty(),
                    "an empty span would paint no cell yet still cost the renderer a style"
                );
            }
        }
    }
}
