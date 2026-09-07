---
name: office-harness
description: How to audit a repository's harness, score its harnessability, and install missing controls in an order a legacy codebase can absorb. Use when onboarding a repo, proposing harness changes, or converting a recurring defect into a computational control.
---

# Harness procedure

## The matrix

Every control occupies one cell. Name the cell when you propose one — it is how
you notice you have built four inferential sensors and no guides.

|  | **Guides** (before) | **Sensors** (after) |
|---|---|---|
| **Computational** | types, schemas, scaffolds, scope allowlists | linters, tests, coverage, hooks, fitness functions |
| **Inferential** | CLAUDE.md, skills, specs | review agents, LLM-as-judge |

Computational controls are cheap, deterministic, and run on every change.
Inferential ones are expensive and probabilistic. Prefer up and left. A rule
that could be a type should not be a paragraph in CLAUDE.md.

## Audit

```bash
$OFFICE audit           # human-readable
$OFFICE audit --json    # for reasoning over
$OFFICE pack list
$OFFICE pack show <stack>    # controls in strangler order, with notes
```

The audit is the computational half: stacks, existing controls, harnessability
score, greenfield vs legacy. It is never wrong about what files exist and it
cannot see anything else.

The inferential half is yours — read the code for boundaries that exist only in
people's heads, conventions followed everywhere and written nowhere, and bug
classes that have been fixed more than once. Each is a control waiting to be
made computational.

## Harnessability

The score (0–100) is a pace decision, not a go/no-go:

| Component | Max | Proxy |
|---|---|---|
| Typing | 25 | How much a compiler proves before a test runs |
| Boundaries | 20 | Whether module structure is legible |
| Tests | 25 | Whether behaviour is verifiable at all |
| Build | 15 | Whether a checkout reproduces (lockfile present) |
| Controls | 15 | Share of the stack's expected controls present |

A low score means the repo needs a harness most and can absorb it least. Slow
down; do not skip.

## Installing on a legacy repo

`office pack show <stack>` prints the order. Follow it. The failure mode is
specific:

> Enable strict checking repo-wide → four thousand errors → nobody triages them
> → the first person who hits the wall disables the harness → the repo is now
> worse than when you found it.

Four rules that prevent it:

1. **Cheap, already-nearly-clean controls first.** Formatters, `go vet`-class
   checks. These usually pass on arrival.
2. **Pin thresholds to the current measured value, not the target.** A coverage
   floor above current coverage blocks the next commit. Record current and
   target in the Ratchets table; raising it is a task on the board.
3. **New-code-only gating where the tool supports it** — `new-from-rev` in
   golangci-lint, a narrowed `include` in tsconfig, per-module opt-in in mypy.
   Gates what is being written without demanding a backlog rewrite.
4. **Hooks last.** A hook installed before the underlying tools are clean gets
   bypassed with `--no-verify`, which is worse than no hook.

On a greenfield repo none of this applies: install the full pack at maximum
strictness with real thresholds. It will never be this cheap again.

## Gate 3

Propose; do not install. Present each control with: its cell, its cost per
commit, its false-positive count on the current codebase, and what it would have
caught. Wait for explicit approval.

Never install "just the safe ones" to save a round trip.

## Recording

`.the-office/harness.md` is the single answer to "what regulates this repo, and
why". Every control gets a row with its cell and check command. Every gap gets a
row saying why it is still a gap — a gap left open on purpose is a decision, a
gap left open by accident is a bug, and the file has to distinguish them.
