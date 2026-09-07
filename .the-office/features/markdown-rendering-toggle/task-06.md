---
id: markdown-rendering-toggle/task-06
task_no: 6
title: Lock the invariants — never focusable, source window untouched
spec_required: true
requirements: [REQ-003, REQ-004]
acceptance_criteria: [AC-003, AC-004]
verification_mode: preservation
depends_on: [markdown-rendering-toggle/task-05]
status: pending
tier: standard
scope:
  - crates/omv/src/main.rs
  - crates/omv/src/app.rs
checks:
  - cargo fmt --all --check
  - ./.github/scripts/arch-check.py
  - "! grep -A9 -e 'pub enum Focus' crates/omv/src/app.rs | grep -qi -e markdown -e render"
  - cargo test -p omv --bin omv -- --list | grep -q tests::directional_focus_never_reaches_the_render_pane
  - cargo test -p omv --bin omv -- --list | grep -q tests::editing_keys_still_reach_the_raw_buffer_while_the_render_pane_is_visible
  - cargo test -p omv --bin omv -- --list | grep -q tests::toggling_the_render_pane_leaves_the_source_window_exactly_as_editing_left_it
  - cargo test -p omv --bin omv
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo test --workspace
sensors_added:
  - "grep guard: `Focus` gains no markdown/render variant (task check, mirrors the spec's 'no new Focus variant')"
  - "test: directional_focus_never_reaches_the_render_pane — locks REQ-003"
  - "test: toggling_the_render_pane_leaves_the_source_window_exactly_as_editing_left_it — locks REQ-004"
dod: |
  With the render pane visible **and drawn**, `<C-w>h/j/k/l` pressed repeatedly
  only ever focus windows that already existed, `app.focus` stays
  `Focus::Editor`, and `windows.count()` is unchanged. Insert/delete/paste
  bindings edit the raw buffer identically to a run with the pane never toggled.
  Running the same edit script twice — once plain, once with the pane toggled on
  partway and off again, with frames drawn in between — leaves identical buffer
  text, cursor char index, window scroll and window count. `Focus` still has its
  six pre-existing variants.
attempts: 0
max_attempts: 3
base_commit: null
branch: null
commit: null
---

## Context

By construction none of this should need a code change: the render pane holds no
`WindowId` of its own, captures no keys, and never writes to the source window.
That is exactly why it is worth pinning with tests — an invariant that holds "by
construction" is one nobody notices breaking. If a test here does fail, the fix
belongs in `app.rs` (the only in-scope source file), not in loosening the test.

**Every test in this task must draw frames, or it tests nothing.** The one place
a bug could plausibly hide is task-03's narrowing of the source window's `area`,
and that narrowing happens *inside* `ui::render` — a test that only presses keys
never runs it, and the pane-on app would be indistinguishable from the pane-off
app. `app.text_height` is likewise only written during a draw
(`crates/omv/src/ui.rs:72`); the repo already knows this and says so at
`crates/omv/src/main.rs:264`: `screen(&mut app); // establishes text_height,
which sizes the scroll page`. So: call `screen(&mut app)` after toggling the
pane on, between edit steps, and before comparing state. Both apps in the AC-004
comparison must be drawn the same number of times, or `text_height` alone will
make them differ for an uninteresting reason.

The same applies to directional focus for a second reason: `Windows::in_direction`
(`crates/omv/src/window.rs:202`) reads the rects the last frame assigned, so
without a draw it is reasoning about stale or zero rects rather than about the
narrowed raw pane.

The comparison style for AC-004 — run the same script on two `App`s and assert
the states match — is stronger than asserting hard-coded numbers, because it
stays true if the editor's own behaviour changes later.

## Approach

Three tests in `crates/omv/src/main.rs`:

- `directional_focus_never_reaches_the_render_pane`: `:vsp`, toggle the pane on,
  `screen(&mut app)` so the rects are real, record
  `app.windows.iter().map(|w| w.id)`, then press `<C-w>` + each of `h/j/k/l`
  several times — drawing a frame after each press — asserting every time that
  `focused_id()` is in the recorded set, `app.focus == Focus::Editor`, and
  `windows.count()` is unchanged. Run it against a split, not a single window:
  a single window has no neighbour and would pass vacuously.
- `editing_keys_still_reach_the_raw_buffer_while_the_render_pane_is_visible`:
  pane on and drawn, then `i` + text + `<Esc>`, then `dd`, then `p`, asserting
  the rope contents after each step, with a frame drawn between steps.
- `toggling_the_render_pane_leaves_the_source_window_exactly_as_editing_left_it`:
  two apps from the same fixture text; run an identical key script on both, with
  the second toggling the pane on before the edits and off after, and with both
  drawing a frame at the same points (the second therefore draws at least one
  frame with the pane visible, which is the only way the narrowing is
  exercised); assert equal buffer text, equal `editor.buffer().cursor`, equal
  `windows.focused().scroll` and equal `windows.count()`.

The `Focus` grep in the checks is a cheap standing guard that this feature never
grows a focus variant; leave the enum alone.

## Notes
