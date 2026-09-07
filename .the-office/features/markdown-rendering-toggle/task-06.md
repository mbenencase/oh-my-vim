---
id: markdown-rendering-toggle/task-06
task_no: 6
title: Lock the invariants — never focusable, source window untouched
spec_required: true
requirements: [REQ-003, REQ-004]
acceptance_criteria: [AC-003, AC-004]
verification_mode: preservation
depends_on: [markdown-rendering-toggle/task-05]
status: completed
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
  - "test: focus_still_has_exactly_its_six_pre_existing_variants — a compile-time, no-wildcard match over every `Focus` variant. Strengthens the task's own grep guard, which a prior reviewer noted only excludes the literal words markdown/render and would wave through e.g. `Focus::Preview`; this fails to compile the instant a seventh variant is added anywhere, regardless of its name. Verified by mutation (see Notes)."
dod: |
  With the render pane visible **and drawn**, `<C-w>h/j/k/l` pressed repeatedly
  only ever focus windows that already existed, `app.focus` stays
  `Focus::Editor`, and `windows.count()` is unchanged. Insert/delete/paste
  bindings edit the raw buffer identically to a run with the pane never toggled.
  Running the same edit script twice — once plain, once with the pane toggled on
  partway and off again, with frames drawn in between — leaves identical buffer
  text, cursor char index, window scroll and window count. `Focus` still has its
  six pre-existing variants.
attempts: 1
max_attempts: 3
base_commit: 952a516025fb990d430d4a9b73e679ca70d063b6
branch: development
commit: 80ec0f4d20a8259239ebbacd0890b825d6ed6ea6
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

Implemented as three tests in `crates/omv/src/main.rs` (no `app.rs` change was
needed — the invariant already held by construction, as the Context predicted),
plus one extra sensor test pinning the exact `Focus` variant list at compile
time. `cargo test --workspace` moved 123 -> 127; the ratchet in
`.github/workflows/ci.yml` / `.githooks/pre-push` / `.the-office/harness.md` is
left untouched per this task's out-of-scope note (task-07's job).

Mutation testing (all reverted before commit; `git diff` on `app.rs`/`ui.rs`
is clean):

- `Effect::FocusWindow`: forced `self.focus = Focus::Terminal` after a
  direction press -> killed by `directional_focus_never_reaches_the_render_pane`
  (the `app.focus == Focus::Editor` assertion).
- `Effect::FocusWindow`: added a spurious `self.split_window(..)` after a
  direction press -> killed by the same test (the "focused a window that did
  not exist before" / count assertions).
- `handle_editor_key`: early-returned whenever `markdown_render.is_some()` ->
  killed by `editing_keys_still_reach_the_raw_buffer_while_the_render_pane_is_visible`.
- `Effect::ToggleMarkdownRender`: reset scroll to 0 on toggle-off -> killed by
  `toggling_the_render_pane_leaves_the_source_window_exactly_as_editing_left_it`
  (also confirms the 40-line fixture was long enough to make scroll non-zero:
  observed 23 vs 0).
- `refresh_markdown_pane`: same scroll-reset, moved into the refresh path
  itself rather than the toggle effect -> killed by the same test.
- Added a 7th `Focus` variant (`Preview`) -> does not compile
  (`handle_key`'s pre-existing exhaustive match fails first, then this task's
  new `focus_variant_name` match). Confirms the grep guard alone is
  insufficient here: `! grep -A9 'pub enum Focus' app.rs | grep -qi
  markdown|render` still *exits 0* (passes) with `Preview` added, exactly the
  gap the previous reviewer flagged. The compile-time match is the sensor that
  actually catches it.

Went further, probing code I did not write, as instructed:
- Re-ran `app.windows.layout(text_area)` a second time in `ui.rs`, right
  after the render-pane narrowing (simulating "something later overwrites
  `window.area`"). **None of this task's three new tests caught it** — they
  don't re-inspect window width after the edit/focus steps. It WAS caught by
  task-03's pre-existing `the_render_pane_appears_beside_the_raw_text_and_leaves_again`
  (`area.width < full_width` assertion). Reporting this as a genuine gap in my
  own tests' reach rather than hiding it: REQ-003/004 as I tested them do not
  depend on the pane's width narrowing at all (only on focus routing and on
  cursor/scroll/buffer state), so a regression that only affects narrowing
  is outside what task-06's tests are built to catch — it is caught by
  task-03/04's suite instead, which remains green and in the workspace.
- Whether `in_direction` could ever pick the pane if its rect overlapped a
  real window's: not independently mutation-tested, because it is not
  reachable through any code path — `in_direction` only ever iterates
  `Windows::iter()`, which walks the split tree, and the pane's `Rect` in
  `ui.rs` (the local `pane` binding) is never written into any `Window.area`;
  only the narrowed `raw` rect is (`app.windows.get_mut(id)?.area = raw`).
  There is no line to mutate that would give `in_direction` visibility into
  the pane's geometry without first making the pane a member of the tree,
  which is exactly what REQ-003/the Assumptions section forbid. Structural,
  not sensor-backed beyond what `focus_still_has_exactly_its_six_pre_existing_variants`
  and `directional_focus_never_reaches_the_render_pane` already cover.
- Whether any key path reaches the pane's columns: covered by the same
  reasoning — `handle_key` dispatches solely on `Focus`, `Focus` cannot gain a
  pane-facing variant without breaking compilation (mutant above), and
  `markdown_render` is read only for drawing/refreshing, never consulted by
  `handle_key`/`handle_editor_key`. No survivor found here either.
