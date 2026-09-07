---
id: markdown-rendering-toggle/task-02
task_no: 2
title: toggle_markdown_render action, effect, :md command, and the editor-wide flag
spec_required: true
requirements: [REQ-007]
acceptance_criteria: [AC-007]
verification_mode: acceptance
depends_on: []
status: completed
tier: standard
scope:
  - crates/omv-core/src/action.rs
  - crates/omv-core/src/editor.rs
  - crates/omv-core/src/lib.rs
  - crates/omv-config/assets/default.yaml
  - examples/config.yaml
  - crates/omv/src/app.rs
  - crates/omv/src/main.rs
checks:
  - cargo fmt --all --check
  - ./.github/scripts/arch-check.py
  - cmp crates/omv-config/assets/default.yaml examples/config.yaml
  - grep -q toggle_markdown_render crates/omv-config/assets/default.yaml
  - cargo run -p omv --quiet -- --list-actions | grep -q toggle_markdown_render
  - cargo run -p omv --quiet -- --list-keys | grep -q toggle_markdown_render
  - cargo test -p omv-core --lib -- --list | grep -q tests::toggle_markdown_render_asks_the_ui_to_toggle
  - cargo test -p omv --bin omv -- --list | grep -q tests::leader_m_toggles_the_markdown_render_flag
  - cargo test -p omv --bin omv -- --list | grep -q tests::the_md_command_toggles_the_same_flag
  - cargo test -p omv --bin omv -- --list | grep -q tests::toggling_on_records_the_focused_window_and_off_clears_it
  - cargo test -p omv --bin omv
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo test --workspace
sensors_added: []
dod: |
  A new `toggle_markdown_render` action exists in the `actions!` macro under the
  "Panels" category, is bound to `<leader>m` in normal mode in both
  `crates/omv-config/assets/default.yaml` and `examples/config.yaml` (still
  byte-identical), and is reachable as `:md` / `:markdown`. Dispatching it
  returns a single `Effect::ToggleMarkdownRender`, which `App::apply_effects`
  turns into `app.markdown_render = Some(focused window id)` when hidden and
  `None` when visible, regardless of which window is focused at the time.
  `--list-actions` and `--list-keys` both show it. Nothing is drawn yet.
attempts: 1
max_attempts: 3
base_commit: f13dcba4db5ca727542538673d1f721ca914195c
branch: development
commit: 2a535e78ab1cf85571753ba88931dde0b9f6ab1b
---

## Context

This is the plumbing task: action, effect, command, state. It deliberately
draws nothing, so that `F4 keymap-sync` / `F5 unreachable-action` and the
`--list-actions` contract are proved green on their own before any renderer
work lands on top.

Follow the four-step recipe in CLAUDE.md ("Adding a key binding or action")
exactly. Two of the four steps are checked by `arch-check.py` and will fail the
commit if skipped: the two YAML files must stay byte-identical, and an action
bound to no key and dispatched by no `:command` is a violation. `<leader>m` is
free in the default keymap today; `m` is the mnemonic the spec settled on.

State shape, per the approved spec's Assumptions: **one editor-wide flag that
remembers which window it is attached to**. `pub markdown_render:
Option<WindowId>` on `App` is exactly that — `Some(id)` means visible and
attached to window `id`. Do not add a `Focus` variant; the render pane is never
focused and captures no keys, so `Focus` must stay at its six variants
(task-06 locks that). Do not add a node to the `Windows` tree.

`Action` declaration order is display order in `:keys` — put the new entry in
the Panels block next to `toggle_diagnostics` and `toggle_terminal`.

## Approach

1. `crates/omv-core/src/action.rs`: one line in the Panels block of `actions!`,
   e.g. `ToggleMarkdownRender => "toggle_markdown_render", "Panels", "Show/hide
   a rendered Markdown view beside the buffer"`.
2. `crates/omv-core/src/editor.rs`: dispatch arm returning
   `vec![Effect::ToggleMarkdownRender]`, a new `Effect` variant next to
   `ToggleDiagnostics`, and `"md" | "markdown" => self.dispatch(...)` in the
   `match cmd` table (this is what `F5` reads).
3. `crates/omv/src/app.rs`: the `markdown_render: Option<WindowId>` field,
   initialised `None` in `App::new`, and the `apply_effects` arm that flips it.
4. Both keymap YAML files get `"<leader>m": toggle_markdown_render` in the
   panels group of `normal:`. Edit one and copy it over the other — `cmp` is a
   check for a reason.

Tests: one in `omv-core` (`crates/omv-core/src/lib.rs` tests module) proving the
action yields the effect and that `:md` and `:markdown` both reach it; three in
`crates/omv/src/main.rs` driving the real keymap with the existing `press_ctrl`
/ `press` / `ex` helpers — `<Space>m` toggles on then off, `:md` does the same,
and toggling on records the focused window's id while toggling off clears it
even after focus has moved to a different window (split first, then toggle off).

## Notes
