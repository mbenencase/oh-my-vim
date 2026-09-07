---
id: markdown-rendering-toggle/task-04
task_no: 4
title: Keep the render live on every edit and follow the source window's buffer
spec_required: true
requirements: [REQ-005]
acceptance_criteria: [AC-005]
verification_mode: acceptance
depends_on: [markdown-rendering-toggle/task-03]
status: pending
tier: standard
scope:
  - crates/omv/src/app.rs
  - crates/omv/src/main.rs
checks:
  - cargo fmt --all --check
  - ./.github/scripts/arch-check.py
  - "! grep -q markdown::render crates/omv/src/ui.rs"
  - cargo test -p omv --bin omv -- --list | grep -q tests::typing_updates_the_render_pane_without_toggling_it
  - cargo test -p omv --bin omv -- --list | grep -q tests::deleting_a_heading_removes_it_from_the_render_pane
  - cargo test -p omv --bin omv -- --list | grep -q tests::the_render_pane_follows_the_source_window_to_another_buffer
  - cargo test -p omv --bin omv -- --list | grep -q tests::the_render_pane_scrolls_with_the_source_window
  - cargo test -p omv --bin omv -- --list | grep -q tests::opening_another_file_outside_the_effect_loop_refreshes_the_render_pane
  - cargo test -p omv --bin omv -- --list | grep -q tests::jumping_into_a_longer_file_does_not_panic_the_render_pane
  - cargo test -p omv --bin omv -- --list | grep -q tests::the_document_is_parsed_once_per_batch_and_never_by_drawing
  - cargo test -p omv --bin omv
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo test --workspace
sensors_added:
  - "test: the_document_is_parsed_once_per_batch_and_never_by_drawing — keeps the parse out of the render loop, which the ui.rs grep alone cannot prove"
  - "test: jumping_into_a_longer_file_does_not_panic_the_render_pane — locks the total-accessor contract against the picker/LSP jump paths"
dod: |
  While the pane is visible, typing into the raw buffer changes what the pane
  draws on the very next frame, with no toggle in between: inserting `# Title`
  makes a level-1 heading appear in the render columns, and deleting that line
  removes it. Switching the source window to another buffer with `:bn`, opening
  a file from the picker or explorer, and jumping to a location in a longer file
  all leave the pane showing the buffer the source window is actually on — the
  last of those without panicking. Scrolling the source window moves the pane's
  content with it. Drawing frames never parses: the parse counter is unchanged
  across repeated `screen()` calls and unchanged entirely while the pane is
  hidden.
attempts: 0
max_attempts: 3
base_commit: null
branch: null
commit: null
---

## Context

Task-03 fills the cache when the pane is toggled on and never again, so the
render goes stale the moment the user types. The tests named in this task's
checks fail before this task and pass after it — that is the point of splitting
it out, and reviewing task-03 should not have treated the staleness as a defect.

**`apply_effects` is not the only path, and assuming it is would ship a
crash.** Three families of buffer switch never reach the effect loop at all:

- `handle_picker_key`'s `Enter` arm (`crates/omv/src/app.rs:332`) calls
  `activate_payload` (`app.rs:496`) directly.
- `handle_explorer_key`'s `Enter`/`o` arm (`app.rs:292`) calls `open_path`
  directly.
- `handle_lsp_event`'s `E::Definition` (`app.rs:903`) calls `open_path` then
  `scroll_to_cursor`, and the LSP formatting path (`app.rs:982`) rewrites buffer
  text and calls `refresh_highlights_for` in a loop.

`open_path` already calls `refresh_highlights()` inline (`app.rs:115-121`)
*because* it bypasses the effect loop — that is the local precedent, and the
markdown cache must follow it rather than contradict it.

This is not hypothetical staleness. `Payload::Location` (`app.rs:499`, produced
by the find-text picker at `app.rs:734` and by LSP references at `app.rs:914`)
calls `open_path` and then `scroll_to_cursor`, so `window.scroll` becomes an
index into the **new** buffer while the cache may still describe the old,
shorter one. Task-03's total accessor is what stops that being a panic; this
task is what stops it being wrong.

Refreshing on `Effect::BufferChanged` alone is also insufficient for a second,
independent reason: `Action::NextBuffer` / `PrevBuffer` (`:bn` / `:bp`) return
`vec![Effect::ScrollToCursor, Effect::Status(..)]` — no `BufferChanged` at all —
and change `editor.current`, which `sync_focused_window()` then writes into the
focused window at the end of the batch.

Within the effect loop, the end of `apply_effects` (after
`sync_focused_window()`) is the right place, and it gets edits for free:
`handle_editor_key` funnels `insert_char`, `insert_newline`, `backspace` and
every dispatched action through it, so "once per effect batch" is once per
keystroke, never once per frame. Recomputing the whole document per keystroke is
the same cost class CLAUDE.md already accepts for tree-sitter and LSP
full-document sync, and it costs nothing while `markdown_render` is `None`.

Scroll alignment is already the identity mapping, because task-01 emits one
rendered line per source line and task-03 draws from `window.scroll`. The scroll
test here is a guard on that, not new machinery.

## Approach

Put a `refresh_markdown_for(index)` call beside **every** `refresh_highlights` /
`refresh_highlights_for` call site — there are five today (`open_path`
`app.rs:120`, `activate_payload`'s `Payload::Buffer` arm `app.rs:510`, the
`Effect::BufferChanged` arm `app.rs:529`, `focus_window` `app.rs:654`, and the
LSP formatting loop `app.rs:982`) — and one more at the end of
`apply_effects`, after `sync_focused_window()`, for the source window's buffer.
Make each call a no-op when `markdown_render` is `None` so the hidden case stays
free, and read the buffer from the window the flag names rather than from
`editor.current`, so a pane attached to an unfocused window keeps showing that
window's buffer.

For the parse counter, add a `#[cfg(test)]` counter to `App` (a `usize` field
incremented in `refresh_markdown_for` is enough) so
`the_document_is_parsed_once_per_batch_and_never_by_drawing` can assert it does
not move across repeated `screen()` calls, does not move at all while the pane
is hidden, and moves exactly once per keystroke while it is visible. Between
that test and task-03's `ui.rs` grep, "the pane is not parsed per frame" stops
being a promise in prose.

Tests in `crates/omv/src/main.rs`, using the `screen()` / styled-cell helpers
from task-03 and the existing `press` / `type_text` / `ex` helpers:

- `typing_updates_the_render_pane_without_toggling_it` — toggle on, enter insert
  mode, type a heading, assert the render columns show it styled as a heading.
- `deleting_a_heading_removes_it_from_the_render_pane` — the reverse, so the
  cache is proved to be rebuilt rather than only appended to.
- `the_render_pane_follows_the_source_window_to_another_buffer` — two buffers
  with different headings, `:bn`, assert the pane shows the second buffer's.
- `the_render_pane_scrolls_with_the_source_window` — a long document, jump to
  the end, assert an early heading is gone from the render columns and a late
  one is present.
- `opening_another_file_outside_the_effect_loop_refreshes_the_render_pane` —
  drive `activate_payload` through the picker (or `open_path` through the
  explorer key path), not through `apply_effects_for_test`, and assert the pane
  shows the newly opened file.
- `jumping_into_a_longer_file_does_not_panic_the_render_pane` — pane visible on
  a short buffer, then a `Payload::Location` into a much longer file at a line
  beyond the short buffer's length; assert the frame renders and shows the new
  file's content. Use a temp file or a second in-memory buffer; do not write
  into the repo.

## Notes
