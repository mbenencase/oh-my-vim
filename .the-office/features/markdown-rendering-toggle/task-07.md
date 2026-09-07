---
id: markdown-rendering-toggle/task-07
task_no: 7
title: Document the render pane and raise the test-count ratchet
spec_required: true
requirements: [REQ-007]
acceptance_criteria: [AC-007]
verification_mode: preservation
depends_on: [markdown-rendering-toggle/task-06]
status: completed
tier: fast
scope:
  - README.md
  - CLAUDE.md
  - .github/workflows/ci.yml
  - .githooks/pre-push
  - .the-office/harness.md
checks:
  - ./.github/scripts/arch-check.py
  - cargo run -p omv --quiet -- --list-actions | grep -q toggle_markdown_render
  - grep -q 'leader>m' README.md
  - grep -qF ':md' README.md
  - "test $(grep -oE 'floor=[0-9]+' .githooks/pre-push | cut -d= -f2) -gt 83"
  - "test $(grep -oE 'floor=[0-9]+' .githooks/pre-push | head -1) = $(grep -oE 'floor=[0-9]+' .github/workflows/ci.yml | head -1)"
  - "grep -E '^\| Passing tests' .the-office/harness.md | grep -qF $(grep -oE 'floor=[0-9]+' .githooks/pre-push | cut -d= -f2)"
  - "grep -F 'Test-count ratchet' .the-office/harness.md | grep -qF $(grep -oE 'floor=[0-9]+' .githooks/pre-push | cut -d= -f2)"
  - "test $(grep -cF $(grep -oE 'floor=[0-9]+' .githooks/pre-push | cut -d= -f2) CLAUDE.md) -ge 2"
  - "grep -qF $(grep -oE 'floor=[0-9]+' .githooks/pre-push | cut -d= -f2) README.md"
  - "! grep -qF '83 tests' CLAUDE.md"
  - "! grep -qF '83 tests' README.md"
  - "! grep -qF '83 tests run in CI' .the-office/harness.md"
  - "test $(cargo test --workspace 2>&1 | sed -n 's/^test result: ok\. \([0-9]*\) passed.*/\1/p' | awk '{s+=$1} END {print s+0}') -ge $(grep -oE 'floor=[0-9]+' .githooks/pre-push | cut -d= -f2)"
  - cargo fmt --all --check
  - cargo test --workspace
sensors_added:
  - "test-count ratchet raised from 83 to the measured count, in .github/workflows/ci.yml, .githooks/pre-push and .the-office/harness.md"
dod: |
  The three copies of the test-count floor (CI workflow, pre-push hook, harness
  manifest ratchet table) all name the same number, that number is higher than
  the old 83, and `cargo test --workspace` meets it. The `83 tests` claims in
  `README.md` and `CLAUDE.md` are updated to the same number. `README.md`
  documents `<leader>m` / `:md` in the "What works today" table, the ex-command
  row, and a short section describing the pane. `CLAUDE.md` records the one
  thing a future contributor would otherwise get wrong: the render pane is a
  renderer-level companion that subdivides a window's rect and is never a member
  of the `Windows` tree.
attempts: 1
max_attempts: 3
base_commit: cd725c924900883adabd1362a35fb873c8c7ae43
branch: development
commit: 36f13577b79fb1e8a3079c48ef002b1567f8a7bb
---

## Context

Two features have already shipped from this repo without releasing anything,
because the commit subject decides the release. This task's commit is
documentation and thresholds — `docs:` or `chore:` is the honest subject, and
the feature's own release comes from the earlier commits. Do not retro-label
this one `feat:`.

The floor lives in three files on purpose (CI, the hook, and the manifest that
explains why the number exists), and `.the-office/harness.md`'s Ratchets table
is the one a human reads. All three must move together — the checks here assert
they agree rather than trusting it.

`README.md` claims `cargo test --workspace # 83 tests` in its Testing section
and `CLAUDE.md` says `83 tests, all fast` plus `83 passing at last measurement`;
both are drift the moment this feature lands. `CLAUDE.md`'s doc-freshness drift
is a recorded, recurring finding in the harness manifest — this is the task that
prevents it recurring for this feature.

## Approach

Measure once: `cargo test --workspace` and sum the `test result: ok. N passed`
lines (the same `sed`/`awk` the pre-push hook uses). Then write that number into
`floor=` in `.githooks/pre-push` and `.github/workflows/ci.yml`, into the
Ratchets table row in `.the-office/harness.md`, and into the two prose claims in
`README.md` and `CLAUDE.md`.

`README.md`: a `| **Markdown render** | ... |` row in "What works today", `:md`
added to the Ex commands row, and a short `## Markdown render` section after
`## Terminal` in the same voice as the Terminal and Windows sections — what it
shows, that it is read-only and never takes focus, that it updates as you type,
and that the v1 construct list stops at headings, emphasis, lists, code fences
and rules.

`CLAUDE.md`: one bullet in "Things that bite" — the render pane is drawn by
subdividing the source window's rect in `ui.rs`, is remembered as an editor-wide
`Option<WindowId>` on `App`, is never a `Windows` node and never focusable, and
the parser behind it is `omv-syntax::markdown` with one rendered line per source
line so the two panes share a scroll offset.

No source file changes here — the scope list has no `crates/**` entry, and
`office scope` will reject one.

## Notes

- The floor greps are deliberately targeted rather than bare number matches, and
  the reason is worth knowing before editing them. `.the-office/harness.md`
  mentions `83` in four places today, one of which (`a \`61 tests\` claim that
  was 83`) is *history* about an already-fixed drift and must NOT be rewritten —
  so the checks pin the two rows that are live thresholds, the `| Passing tests`
  ratchet row and the `Test-count ratchet` controls row. `CLAUDE.md` carries the
  number twice (`CLAUDE.md:16` in the commands block and `:214` in the Tests
  section); a single `grep -q` would pass with only one updated, hence the
  `-ge 2` count and the paired `! grep '83 tests'` guards. `README.md:190` is
  the only site there. The fourth `harness.md` site (`:85`, "83 tests run in
  CI", in the `office audit still reports rust:tests missing` row) is a live
  count claim rather than history, so it is pinned by its own guard and must be
  updated with the rest; only the `61 tests` history line stays untouched.

- **Write the MEASURED count, not a remembered one.** The suite was measured at
  **101** at task-02's commit (`2a535e7`) using CI's own summation
  (`cargo test --workspace | sed -n 's/^test result: ok\. \([0-9]*\) passed.*/\1/p' | awk '{s+=$1} END {print s+0}'`).
  Reports during tasks 01 and 02 quoted 97, which was 4 low at both ends. Re-measure
  at the time you run, and write that number into all three floors — the checks
  assert agreement between the files, so a wrong-but-consistent number would pass.
