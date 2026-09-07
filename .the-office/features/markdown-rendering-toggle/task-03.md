---
id: markdown-rendering-toggle/task-03
task_no: 3
title: Draw the render pane to the right of the source window
spec_required: true
requirements: [REQ-001, REQ-002, REQ-006, REQ-009]
acceptance_criteria: [AC-001, AC-002, AC-006, AC-009]
verification_mode: acceptance
depends_on: [markdown-rendering-toggle/task-01, markdown-rendering-toggle/task-02]
status: in-progress
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
  - "test `a_scroll_past_the_end_of_the_cache_draws_nothing_rather_than_panicking` — pins the draw as total; verified to panic (`range start index 400 out of range for slice of length 0`) when the `get(scroll..)` is replaced by a bare slice"
  - "tests `every_v1_construct_is_styled_distinctly_in_the_render_columns` and `heading_levels_are_styled_apart_from_each_other` — pairwise style-distinctness fitness checks over the drawn cells, so a new `markdown_style` arm or theme colour that collides with an existing one fails rather than being merged"
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
attempts: 1
max_attempts: 3
base_commit: 730d4e3aeb1cc0114ff15a6dcd9464af93bcd03d
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

## Implementation notes (attempt 1)

**What landed.** `Theme` gained five Markdown fields (`markdown_heading:
[Color; HeadingLevel::MAX as usize]`, `markdown_list`, `markdown_ordered`,
`markdown_code`, `markdown_rule`) and `Theme::markdown_style(MarkdownKind) ->
Style`. `App` gained `markdown: HashMap<usize, Vec<MarkdownLine>>`,
`refresh_markdown_for(index)` and `markdown_for(buffer) -> &[MarkdownLine]`,
mirroring `highlights`/`highlights_for`. `ui::render` carves the pane out of
the source window's rect; `ui::render_markdown_pane` draws it. Eight tests plus
three helpers went into `crates/omv/src/main.rs`.

**Design decision 1 — the rect is narrowed in place, and the split is
computed before `app.text_height`.** The subdivision happens immediately after
`app.windows.layout(text_area)` and writes the left slot back into
`Windows::get_mut(id).area`, exactly as the Approach requires, for the two
stated reasons (`render_text`'s `x < area.right()` cursor clamp, and
`Windows::in_direction` reading last-frame rects). It sits *above* the
`text_height` / `viewport_height` assignment even though the corrected
reasoning says the ordering there is irrelevant — a horizontal subdivision
changes width only. Placing it directly after `layout()` keeps "the rect a
window owns is decided in one place" true for a reader, and is inert with
respect to height. If a future change ever makes the pane a *horizontal*
companion, this ordering is the one that stays correct.

**Design decision 2 — the flag is read, never written, by the renderer.** A
`markdown_render` naming a window that no longer exists draws nothing and the
renderer leaves the stale flag alone; clearing it is task-05's. That keeps
`ui::render`'s only mutation of `App` the two pre-existing viewport fields plus
the window rect it is already responsible for assigning.

**Design decision 3 — `refresh_markdown_for(self.editor.current)`, not
`self.windows.focused().buffer`, in the `ToggleMarkdownRender` arm.** The
focused window's `buffer` is written back by `sync_focused_window` only at the
*end* of the effect batch, so inside the arm it is stale for the same reason
its `cursor` is. `editor.current` is the buffer the focused window will be
showing when the frame is drawn.

**Design decision 4 (worth the Reviewer's eye) — the seam is painted, so the
raw pane's edge and the render pane's border are two adjacent rules
(`...text │┌ Markdown ───┐`).** The Approach prescribes painting it "like
`render_divider` does", and this keeps "a window's right edge is marked by a
one-cell seam" true whether its neighbour is another window or the render
pane — a real `:vsp` beside a render pane looks the same on both sides. The
alternative (drop the seam and let the pane's own border separate them) is one
column narrower and visually lighter, but makes the raw window's edge
inconsistent with every other split. Followed the plan; flagging it because it
is the one purely aesthetic choice in the change.

**Ratchets.** No new `unwrap()`/`expect()` in library code — the count is
still 12; the test helpers' `unwrap`/`panic!` are in `#[cfg(test)]`. The one
`as` added is `HeadingLevel::MAX as usize` as an array length, where `From` is
unavailable (not const) and the `u8 -> usize` widening trips none of the four
cast lints.

**Outside this task, not fixed here.** The workspace test count is now 109
(`cargo test --workspace`), up from 101 before this change and from the 83 the
floor still names. Raising the floor in
`.github/workflows/ci.yml`, `.githooks/pre-push` and `.the-office/harness.md`
is task-07's scope, and CI only fails *below* the floor, so nothing is red in
the meantime.

**Known and accepted, per the Notes above.** With the raw pane narrowed to ~39
columns on an 80-column terminal, a cursor past that column is no longer
placed, because `render_text` clamps at `area.right()` and there is no
horizontal scrolling. `:vsp` behaves identically. Not addressed.
