---
id: markdown-rendering-toggle/task-05
task_no: 5
title: Window-tree actions keep the render pane consistent
spec_required: true
requirements: [REQ-008]
acceptance_criteria: [AC-008]
verification_mode: acceptance
depends_on: [markdown-rendering-toggle/task-04]
status: completed
tier: standard
scope:
  - crates/omv/src/app.rs
  - crates/omv/src/main.rs
checks:
  - cargo fmt --all --check
  - ./.github/scripts/arch-check.py
  - cargo test -p omv --bin omv -- --list | grep -q tests::splitting_the_source_window_leaves_one_render_pane_on_the_original
  - cargo test -p omv --bin omv -- --list | grep -q tests::closing_the_source_window_removes_its_render_pane
  - cargo test -p omv --bin omv -- --list | grep -q tests::window_only_keeps_the_render_pane_only_when_the_survivor_is_the_source
  - cargo test -p omv --bin omv -- --list | grep -q tests::toggling_the_render_pane_never_changes_the_window_count
  - cargo test -p omv --bin omv window::tests
  - cargo test -p omv --bin omv
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo test --workspace
sensors_added:
  - "test: splitting_the_source_window_leaves_one_render_pane_on_the_original — locks the render pane out of the split tree"
  - "test: toggling_the_render_pane_never_changes_the_window_count — locks REQ-008's window-count invariant"
dod: |
  `split_vertical` / `split_horizontal` on the source window leave exactly one
  render pane, still attached to the window that kept the original id, and raise
  `windows.count()` by exactly one — the same as splitting with no pane visible.
  `window_close` on the source window clears `markdown_render`; closing some
  other window leaves it alone. `window_only` keeps the pane when the survivor
  is the source window and clears it otherwise. Toggling the pane on or off
  never changes `windows.count()`. `crates/omv/src/window.rs` is untouched and
  its existing test suite still passes unchanged.
attempts: 1
max_attempts: 3
base_commit: ce800ab717ba0ad21647288ca441fc76dfce42d2
branch: development
commit: 952a516025fb990d430d4a9b73e679ca70d063b6
---

## Context

The render pane is not in the `Windows` tree, so the tree needs no changes at
all — `crates/omv/src/window.rs` is deliberately **outside this task's scope**.
What needs handling is the flag on `App` going stale when the window it names
disappears.

The tree's own semantics do most of the work already, and you should verify
rather than reimplement them: `Windows::split` gives the **new** half a fresh id
and leaves the original id on the window it was cut from (then focuses the new
one), so a flag holding the original id stays attached to "whichever half kept
the source window's identity" with no code. `Windows::close` refuses on the last
window and returns `false`; `Windows::only(id)` keeps exactly the window `id`.

The spec's Harness impact section suggests `window.rs`'s suite as the place for
this coverage, but `Windows` has no knowledge of the render pane and must not
gain any — so the new tests live at the `App` level in `crates/omv/src/main.rs`,
and `window.rs`'s existing suite is run unchanged as a preservation check.

Task-03 already makes the renderer draw nothing when the flag names a missing
window; that is defence in depth, not a substitute for clearing the flag here.

## Approach

In `App::apply_effects`:

- `Effect::CloseWindow`: after a successful `windows.close(id)`, clear
  `markdown_render` if it held `id`.
- `Effect::OnlyWindow`: after `windows.only(focused)`, clear `markdown_render`
  if it does not hold the surviving id.
- `Effect::SplitWindow`: nothing to do, but assert it in a test rather than
  leaving it to a reader's trust.

A single helper — "forget the render pane if its window is no longer in the
tree" — called after the tree mutates is fine and probably clearer than three
separate conditions; either way it must not clear the flag when an *unrelated*
window closes.

Tests in `crates/omv/src/main.rs`, using the `ex(&mut app, "vsp")` /
`ex(&mut app, "close")` / `ex(&mut app, "only")` helpers the existing window
tests already use. `window_only_keeps_the_render_pane_only_when_the_survivor_is_the_source`
should cover both directions in one test: `:only` from the source window keeps
it, `:only` from the sibling clears it. Assert on `app.markdown_render`, on
`app.windows.count()`, and — for the split case — that exactly one render pane
is drawn by checking the screen contains a single `Markdown` panel title.

## Notes

- Depends on task-04 rather than task-03 for a mechanical reason, not a
  conceptual one: the two tasks declare the same scope
  (`crates/omv/src/app.rs`, `crates/omv/src/main.rs`), both add arms to
  `App::apply_effects`, and both add tests to the same `mod tests` in
  `main.rs`. Left as siblings off task-03 they would be ready at the same
  moment and would collide. Nothing in this task needs task-04's live refresh.
