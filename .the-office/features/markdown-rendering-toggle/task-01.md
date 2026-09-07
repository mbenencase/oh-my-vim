---
id: markdown-rendering-toggle/task-01
task_no: 1
title: Pure Markdown parser producing rendered lines in omv-syntax
spec_required: true
requirements: [REQ-002]
acceptance_criteria: [AC-002]
verification_mode: acceptance
depends_on: []
status: completed
tier: deep
scope:
  - crates/omv-syntax/src/**
checks:
  - cargo fmt --all --check
  - ./.github/scripts/arch-check.py
  - cargo clippy -p omv-syntax --all-targets -- -D warnings
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::headings_carry_their_level
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::bold_and_italic_collapse_their_markers
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::ordered_and_unordered_items_keep_a_marker
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::fenced_code_block_lines_are_kept_verbatim
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::a_horizontal_rule_becomes_a_rule_line
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::plain_prose_renders_as_plain_text
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::an_unclosed_fence_still_yields_every_line
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::a_stray_emphasis_marker_stays_literal_text
  - cargo test -p omv-syntax --lib -- --list | grep -q markdown::tests::every_source_line_produces_exactly_one_rendered_line
  - cargo test -p omv-syntax --lib markdown::
  - cargo test --workspace
sensors_added:
  - "type: `HeadingLevel` newtype (private `u8`, `clamped`/`get`/`index`) — `MarkdownKind::Heading`'s payload can no longer be 0 or >6 from anywhere in the workspace, so a theme's per-level style table is indexed totally by construction rather than by the caller remembering to clamp"
  - "test: every_source_line_produces_exactly_one_rendered_line — the load-bearing identity line mapping, asserted over 8 documents (empty, bare newline, no trailing newline, CRLF, unclosed fence, all-markers) against `source.split('\n').count()`; any future reflow/wrap/merge fails it"
  - "test: no_rendered_span_is_ever_empty — locks the documented `MarkdownSpan` guarantee"
  - "test: a_stray_emphasis_marker_stays_literal_text — locks 'never panics, never loses text' over pathological emphasis input"
  - "test: a_heading_level_can_only_index_inside_a_six_entry_table — exercises the newtype's clamp at `u8::MIN`/`u8::MAX` and proves `index()` lands inside a `HeadingLevel::MAX`-entry table"
dod: |
  `omv_syntax::markdown::render(text)` turns Markdown source into one
  `MarkdownLine` per source line, each carrying spans tagged with the v1
  construct that produced them: heading (with its level 1-6), bold, italic,
  unordered item marker, ordered item marker, fenced code, horizontal rule, or
  plain. Inline markers (`**`, `*`, `_`) are collapsed out of the rendered text;
  `#` heading markers and list bullets are replaced by a display marker rather
  than being silently dropped. Malformed input (unclosed fence, stray `**`) is
  never a panic and never loses a source line. The parser is pure: text in,
  structure out, no I/O, no new crate dependency.
attempts: 1
max_attempts: 3
base_commit: 7725cdb2275726c9c949439888377e50c83a1eb2
branch: development
commit: f13dcba4db5ca727542538673d1f721ca914195c
---

## Context

The render pane (task-03) draws *collapsed* Markdown: `**bold**` shows as
`bold`. That rules out modelling the render as byte spans over the source the
way `omv_syntax::Highlighter` does for Rust and JSON — the renderer needs
rendered text, not offsets into the raw text. So this is a new, separate model
in the same crate rather than a new `HighlightKind` or a tree-sitter grammar.

`omv-syntax` is the right home: it already exists to turn buffer text into
structured display data, `omv -> omv-syntax` is an edge `F1 dep-direction`
already allows, and putting it here means **no edit to
`.github/scripts/arch-check.py` and no new workspace crate**. Do not add a
dependency (no `pulldown-cmark`, no `tree-sitter-md`): the v1 construct list is
six block constructs and two inline ones, a dependency would need a new entry in
`deny.toml`'s advisory surface for no benefit, and a source-span-oriented
Markdown grammar does not give the collapsed output the pane needs.

The **one rendered line per source line** invariant is load-bearing, not
incidental. Task-04 has to keep the pane's scroll aligned with the source
window's scroll (REQ-005); if the mapping is the identity, that alignment is
`window.scroll` and nothing else. Do not reflow, wrap, or merge lines.

`omv-syntax` does not deny the cast lints (only `omv-core` does), but the house
rule still applies: no `unwrap()`/`expect()` in library code — the ratchet is 12
across the workspace and this must not add the thirteenth.

## Approach

New module `crates/omv-syntax/src/markdown.rs`, declared and re-exported from
`lib.rs`. Roughly:

- `MarkdownKind` — `Heading(u8)`, `Bold`, `Italic`, `ListMarker`,
  `OrderedMarker`, `Code`, `Rule`, `Plain`. Levels 1..=6, clamped.
- `MarkdownSpan { text: String, kind: MarkdownKind }`, `MarkdownLine { spans:
  Vec<MarkdownSpan> }`, `pub fn render(text: &str) -> Vec<MarkdownLine>`.
- Public API takes `&str` and returns owned types, per the repo convention.

Two passes per line: a block pass decides what the line *is* (fence
open/close and everything between them is `Code`; `#{1,6} ` is a heading;
`-`/`*`/`+` plus a space is an unordered item; `\d+.` plus a space is an
ordered item; three or more `-`, `*` or `_` alone on a line is a rule), then an
inline pass over the line's text splits `**`/`__` into `Bold` and `*`/`_` into
`Italic`. Inline emphasis is not applied inside `Code` lines.

Degradation rules to pin down and test, since the spec leaves them to you but
requires "never panics, never loses text": an unclosed fence leaves the rest of
the document as `Code` (visually stable — the fence is what the author typed);
an unmatched `**` or `*` stays in the output as literal text rather than
swallowing the tail of the line. Both are cheap to test and both are the
"do not lose content" answer.

Tests in `#[cfg(test)] mod tests` in the same file, named as sentences, with the
names the checks list (they are the contract). Include
`every_source_line_produces_exactly_one_rendered_line` over a document mixing
all seven constructs, and give `a_stray_emphasis_marker_stays_literal_text` a
pathological input (`***`, `**a*`, a fence opened at EOF) so "never panics" is
actually exercised.

## Notes

### Design decision: one kind per span, not block-kind + emphasis modifiers

The real choice here was how a rendered line carries style information. Two
shapes were on the table:

**(A) Flat — `MarkdownSpan { text, kind }`, one `MarkdownKind` per span**
(what the Approach proposed, and what shipped).

**(B) Composed — `MarkdownLine { block: MarkdownBlock, spans }` with
`MarkdownSpan { text, bold, italic }`**, so a span's style is the block style
patched with emphasis modifiers.

(B) is strictly more expressive: `# a **b** c` keeps heading colour *and* bold,
`***x***` keeps both modifiers, and a blank line inside a fence still knows it
is code. It also maps onto `Style::patch` + `Modifier`, which is how ratatui
composes styles anyway.

I chose **(A)**, for two reasons:

1. Three downstream tasks are written against it. task-03's Approach specifies
   `Theme::markdown_style(kind) -> Style` called *per span*, with "bold is
   `Modifier::BOLD`, italic `Modifier::ITALIC`". (B) changes that signature and
   the shape of task-03's and task-04's draw code. The divergence cost is
   concrete and spread across tasks I am not implementing; the gains are
   cosmetic.
2. Both losses (A) causes can be defined away or are outside v1's construct
   list, and I did define the important one away rather than leaving it as a
   silent defect:
   - **Emphasis inside a heading is absorbed into `Heading(level)`.** Markers
     are still collapsed (`# a **b** c` → `# a b c`), and the whole line stays
     heading-styled. Without this rule (A) would render a bold word inside a
     heading in plain-bold, dropping the heading's colour — the one place the
     flat kind is genuinely lossy. Covered by
     `emphasis_inside_a_heading_is_absorbed_into_the_heading`.
   - **`***both***` renders as `Bold`.** REQ-002's v1 list names bold and
     italic separately, not their combination. Documented on `render`.

If a later version wants true composition, (B) is the migration and this note is
the reason it was not done now. The line-level block kind (B) would also add is
*not* needed by tasks 03–04: a blank line has no cells to paint, so a code-line
background could not be drawn from it either way at this scale.

### Deviation from the Approach: `HeadingLevel` instead of `Heading(u8)`

The Approach said "`Heading(u8)` — levels 1..=6, clamped". Clamping at the one
construction site plus a doc comment is not a control: the payload of a public
enum variant is constructible by anyone, and task-03 plans "an array indexed by
level", which panics on `Heading(0)` or `Heading(9)`. `HeadingLevel` wraps a
private `u8`, is built only through `clamped()`, and exposes `index()` returning
`0..HeadingLevel::MAX`. This is the cheapest control that works, per the SWE
protocol's preference for a type over a test. Downstream reads
`MarkdownKind::Heading(level) => table[level.index()]` — infallible, no
`as`, no bounds check.

### Smaller decisions worth a reviewer's eye

- **`text.split('\n')`, not `str::lines()`.** `lines()` drops the empty segment
  after a trailing newline; the buffer's rope counts it. The identity mapping
  only holds against a splitter that agrees with the rope, and
  `every_source_line_produces_exactly_one_rendered_line` asserts against
  `split('\n').count()` precisely so a later switch to `lines()` fails.
- **Rules paint cells.** A `Rule` line renders `─` repeated to the number of
  marker characters the author typed, not a constant and not zero. Zero-width
  was tempting (the parser cannot know the pane's width) but task-03's
  `every_v1_construct_is_styled_distinctly_in_the_render_columns` samples "each
  construct's first render-column cell" — a rule that paints nothing has no cell
  to sample. Mirroring the source also leaves the length with the person who can
  see the pane; a width-aware renderer may still fill its own line off
  `MarkdownKind::Rule`.
- **Headings keep their literal `#`s; bullets normalise to `•`.** Deliberate
  asymmetry. `-`/`*`/`+` are one construct, so one glyph. A heading's repetition
  *is* its level, stays readable with no colour at all, and keeps the rendered
  line's column count near the raw line's — which matters when the two are read
  side by side. Neither marker is silently dropped, which is what the DoD asks.
- **Intraword `_` is not emphasis** (`render_line_for` stays literal), while `*`
  keeps the looser rule. `_` is the one that appears inside identifiers in
  prose about code, which this editor's own docs are full of.
- **Emphasis matching is a small delimiter stack**, not a regex: a closing run
  spends its odd marker first so `***x***` nests inside-out, and any delimiter
  left unpaired stays `DelimState::Literal` and re-emits the characters the
  author typed. That is *how* "never swallows the tail of a line" is enforced
  rather than remembered.

### Out of scope, flagged rather than fixed

`cargo test --workspace` is now **97** (was 83, +14 here). I did **not** raise
the ratchet in `.github/workflows/ci.yml`, `.githooks/pre-push` or
`.the-office/harness.md`, even though CLAUDE.md says to raise it in the same
commit: those three files are task-07's `scope`, and task-07's checks assert the
three numbers agree. Widening this task's scope to touch them would collide with
it. CI still passes (the check is a floor, not equality). If tasks 02–06 are
abandoned, task-07's ratchet bump must still happen or 14 tests could be deleted
undetected.

`omv-syntax`'s `lib.rs` gained `pub mod markdown;` and a `pub use` — inside the
`crates/omv-syntax/src/**` glob, so no scope change was needed.
