---
id: markdown-rendering-toggle/task-01
task_no: 1
title: Pure Markdown parser producing rendered lines in omv-syntax
spec_required: true
requirements: [REQ-002]
acceptance_criteria: [AC-002]
verification_mode: acceptance
depends_on: []
status: pending
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
sensors_added: []
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
attempts: 0
max_attempts: 3
base_commit: null
branch: null
commit: null
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
