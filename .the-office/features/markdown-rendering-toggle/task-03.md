---
id: markdown-rendering-toggle/task-03
task_no: 3
title: Draw the render pane to the right of the source window
spec_required: true
requirements: [REQ-001, REQ-002, REQ-006, REQ-009]
acceptance_criteria: [AC-001, AC-002, AC-006, AC-009]
verification_mode: acceptance
depends_on: [markdown-rendering-toggle/task-01, markdown-rendering-toggle/task-02]
status: pending
tier: deep
scope:
  - crates/omv/src/ui.rs
  - crates/omv/src/theme.rs
  - crates/omv/src/app.rs
  - crates/omv/src/main.rs
checks:
  - cargo fmt --all --check
  - ./.github/scripts/arch-check.py
  - "! grep -qE 'markdown::(\\{[^}]*, *)?render' crates/omv/src/ui.rs"
  - cargo test -p omv --bin omv -- --list | grep -q tests::the_render_pane_appears_beside_the_raw_text_and_leaves_again
  - cargo test -p omv --bin omv -- --list | grep -q tests::the_md_command_shows_and_hides_the_same_pane
  - cargo test -p omv --bin omv -- --list | grep -q tests::raw_source_on_the_left_and_the_render_on_the_right
  - cargo test -p omv --bin omv -- --list | grep -q tests::every_v1_construct_is_styled_distinctly_in_the_render_columns
  - cargo test -p omv --bin omv -- --list | grep -q tests::heading_levels_are_styled_apart_from_each_other
  - cargo test -p omv --bin omv -- --list | grep -q tests::the_render_pane_ignores_the_files_extension
  - cargo test -p omv --bin omv -- --list | grep -q tests::an_empty_buffer_renders_an_empty_pane_rather_than_failing
  - cargo test -p omv --bin omv -- --list | grep -q tests::a_scroll_past_the_end_of_the_cache_draws_nothing_rather_than_panicking
  - cargo test -p omv --bin omv
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo test --workspace
sensors_added:
  - "grep guard: `ui.rs` must not call `markdown::render` — keeps parsing in `App`, out of the per-frame path"
dod: |
  With `app.markdown_render == Some(id)` and window `id` on screen, one frame
  shows that window's rect divided in two: the raw buffer text on the left,
  unchanged and still carrying the real cursor, and a bordered `Markdown` pane
  on the right drawing the parsed document from task-01. Headings (each level),
  bold, italic, unordered items, ordered items, fenced code and horizontal rules
  each render with a style distinguishable from plain text and from each other,
  asserted on the `TestBackend` cell styles rather than by eye. Toggling off
  returns the window to its full width in the next frame. None of this depends
  on the buffer's path or extension; an empty buffer draws an empty pane; and a
  scroll offset past the end of a missing or stale cache entry draws an empty
  pane rather than panicking. `ui.rs` never parses — it reads a cache on `App`.
attempts: 0
max_attempts: 3
base_commit: null
branch: null
commit: null
---

## Context

This is the renderer-level companion pane the human approved: it subdivides the
**source window's on-screen rect** at draw time and is **not** a node in the
`Windows` split tree. That is the whole reason it can be unfocusable — a real
`Window` would carry a buffer and a cursor and could receive the editor's single
live cursor via `<C-w>` navigation or `window_only`.

**Mutate the source window's stored `area` before the window loop draws it; do
not pass an override rect to `render_text`.** Two things downstream read that
stored rect and would be wrong if it still described the full width:

- `render_text` places the terminal cursor only when `x < area.right()`
  (`crates/omv/src/ui.rs:286-293`). Against a full-width rect it would place the
  cursor over the render pane's columns.
- `Windows::in_direction` (`crates/omv/src/window.rs:202`) picks the neighbour
  from the rects the last frame assigned — `area.right() <= origin.x` and the
  overlap test. task-06's directional-focus test depends on that geometry
  describing the raw pane, not a rect the pane is drawn on top of.

(An earlier draft of this task claimed the narrowing had to precede the
`app.text_height` / `viewport_height` assignment at `ui.rs:72-73`. That was
wrong and is corrected here: both read `.area.height`, and a
`Layout::horizontal` subdivision changes width only. The ordering constraint is
"before the window loop", for the two reasons above.)

**Cache shape.** `App::highlights` is `HashMap<usize, Vec<Span>>` keyed by
buffer index with a total accessor (`highlights_for` returns `&[]` for a missing
key, `app.rs:772-774`). Mirror it exactly: `markdown: HashMap<usize,
Vec<MarkdownLine>>`, `refresh_markdown_for(index)`, `markdown_for(buffer) ->
&[MarkdownLine]`. Keying by buffer rather than holding one bare `Vec` is what
makes the pane safe when the source window's buffer changes underneath it —
task-04 explains the several code paths that do exactly that without passing
through `apply_effects`.

**The draw must be total.** Slicing `lines[scroll..]` panics when `scroll >
len`, and that is reachable: a jump to a location in a longer file sets
`window.scroll` from the new buffer while the cache may hold the old one. Use
`get(scroll..).unwrap_or(&[])` (or an equivalent `skip`), and prove it with
`a_scroll_past_the_end_of_the_cache_draws_nothing_rather_than_panicking`.

Read `render_diagnostics` and `render_explorer` for the house style of a
bordered panel (`panel_block(theme, "Markdown", false)` — never focused, so the
focused flag is always false), and `render_divider` for the one-cell `\u{2502}`
seam between side-by-side windows.

`omv/src/ui.rs` and `omv/src/app.rs` are outside `omv-core`, so casts are not
compile errors here — but they already carry 18 of the workspace's 37 cast-lint
warnings, so prefer `u16::try_from(..).unwrap_or(u16::MAX)` shaped code over a
new `as`, and remember `unwrap()`/`expect()` in library code is a ratchet at 12.

## Approach

`App`: add the `markdown` map, `refresh_markdown_for(index)` (fills or removes
the entry using `omv_syntax::markdown::render`), and `markdown_for(buffer)`.
Call `refresh_markdown_for` from the `Effect::ToggleMarkdownRender` arm for the
newly attached window's buffer. Task-04 owns every other call site — do not
parse inside `ui.rs` to shortcut it, which is what the `grep` check enforces.

`Theme`: add the colours the pane needs and a `markdown_style(kind) -> Style`
next to `style_for`, so `ui.rs` never hard-codes a colour. Heading levels need
to differ from one another (an array indexed by level is the cheap answer);
bold is `Modifier::BOLD`, italic `Modifier::ITALIC`, code its own colour, rule
and list markers their own. Add them to `default_dark()`.

`ui::render`: after `layout`, if `markdown_render` names a window that exists,
split that window's `area` with `Layout::horizontal([Fill(1), Length(1),
Fill(1)])`, write the first slot back into the window's `area`, paint the seam
like `render_divider` does, and after the window loop call a new
`render_markdown_pane(frame, app, rect)`. If the id no longer names a live
window, draw nothing (task-05 owns clearing the flag). Skip the pane when the
rect is too narrow to hold a border plus a column — the spec accepts an
unreadable narrow pane, not a panic.

`render_markdown_pane` draws the source window's cached lines from
`window.scroll` into the block's inner area, one `Line` per `MarkdownLine`, each
span styled by `theme.markdown_style`, truncated at the pane width. Reading
scroll from the window is what keeps the two sides aligned for free (task-01
guarantees one rendered line per source line).

Tests go in `crates/omv/src/main.rs` next to the existing window tests. The
existing `screen()` helper flattens away styles, so add a sibling helper that
returns the `TestBackend` buffer's cells with their styles — e.g.
`fn screen_cells(app: &mut App) -> Vec<Vec<(String, Style)>>` — and assert with
it; task-04, task-05 and task-06 all build on that helper.
`every_v1_construct_is_styled_distinctly_in_the_render_columns` should use a
fixture buffer holding all seven constructs, collect the style of each
construct's first render-column cell, and assert the set is pairwise distinct
and distinct from a plain paragraph's;
`raw_source_on_the_left_and_the_render_on_the_right` asserts the literal
`**bold**` source appears in columns left of the pane's first column in the
*same* screen buffer (AC-002 and AC-009 in one assertion).
`the_render_pane_ignores_the_files_extension` covers a buffer with `path: None`
and one named `notes.txt`.
`a_scroll_past_the_end_of_the_cache_draws_nothing_rather_than_panicking` puts
the source window's `scroll` past the cached line count (a second, longer buffer
with no cache entry is the easiest way) and asserts the frame renders.

## Notes

- Known and accepted, not a regression: once the raw pane is narrowed, a cursor
  past the pane's width stops being placed at all, because `render_text` clamps
  at `x < area.right()` and there is no horizontal scrolling — roughly column 33
  on an 80-column `TestBackend`. `:vsp` already behaves this way, and the spec's
  "terminal too narrow" clause covers it. Do not add horizontal scrolling here.

- **This task's commit subject must be `feat:`, and so must the PR title.**
  It is the commit that first makes the feature observable — the pane is drawn
  and `<leader>m` stops being a documented no-op. task-01 (`f13dcba`) and
  task-02 (`2a535e7`) both landed as `chore:` deliberately and correctly, and
  task-07 is instructed to use `docs:`/`chore:` on the reasoning that "the
  feature's own release comes from the earlier commits" — which is false as
  written, because there are no earlier releasing commits. `next-version.sh`
  runs `git log --no-merges` over the range, and this repo lands feature
  branches with merge commits (`eba62a3`), so the individual subjects are what
  the classifier reads; CI additionally checks the PR *title*, which is what a
  squash would land. If neither carries `feat:`, this ships nothing — the exact
  failure of `ac06cf3` and `2863293` that CLAUDE.md's Releases section exists to
  prevent, for the third time.
