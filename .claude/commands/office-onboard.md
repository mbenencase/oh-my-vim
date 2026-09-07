---
description: Audit this repository's harness and propose the controls it is missing.
argument-hint: [optional focus, e.g. "just the test story"]
---

Onboard this repository. Focus: **$ARGUMENTS** (if empty, do a full audit.)

1. If `.the-office/` does not exist, run `node .claude/office/bin/office.mjs init`.
2. Delegate to `office-manager`. It should load the `office-harness` skill, run the audit, do the archaeology the audit cannot do, score harnessability, and produce a proposal.
3. **Stop at Gate 3.** Present the proposal — each control with its cell in the matrix, its cost per commit, its false-positive count on this codebase, and what it would have caught — and wait for explicit approval before anything is installed.
4. On approval, install what was approved, write `.the-office/harness.md`, and merge the pack's `GUIDE.md` into this repo's `CLAUDE.md`.

For a legacy repo, follow the strangler ordering from `office pack show <stack>`. Do not enable strict enforcement repo-wide on arrival — pin thresholds to current measured values and record them as ratchets.
