---
name: office
description: The routing contract for the-office — an AI-assisted development workflow built on harness engineering. Use when running a request through the-office pipeline, when deciding which role handles what, or when you need to know where the human approval gates are.
---

# the-office

A set of roles that build software through a harness rather than around one.

`Agent = Model + Harness`. The harness is everything that regulates the model:
**guides** that steer before it acts, **sensors** that catch it after. Each is
either **computational** (deterministic, cheap, runs every change) or
**inferential** (a model's judgement — richer, slower, probabilistic).

The roles below are inferential controls. Their primary output is meant to be
*computational* controls. A pipeline of review agents that produces no linters,
no types, and no tests has rebuilt the problem it was meant to solve.

## The CLI

Everything deterministic lives in one zero-dependency Node CLI. Use it rather
than reasoning about board state from file contents:

```bash
OFFICE="node .claude/office/bin/office.mjs"
$OFFICE help
```

If a question can be answered by the CLI, it is not a question for a model.

## Routing

```
request
  └─ office-judge ──┬─ harness  → office-manager ─────────────────── GATE 3
                    ├─ trivial  → PO + compact spec → GATE 1 → linked fast task
                    └─ feature  → office-product-owner ── GATE 1
                                     └─ office-planner ⇄ office-devils-advocate
                                            (≤ plan_iterations)     GATE 2
                                                └─ per task ─┐
                                                             ▼
                          office-swe{,-fast,-deep} ⇄ office-reviewer
                                     (≤ review_iterations)
                                             └─ recurrence → office-janitor → GATE 3
```

Pick the SWE variant from the task's `tier:` field — `fast` → `office-swe-fast`,
`standard` → `office-swe`, `deep` → `office-swe-deep`. That is what makes `tier`
mean something.

## The three gates

Read them from `.the-office/config.yml`; all default to on.

**Gate 1 — after clarification.** The Product Owner states its understanding and
stops. A misunderstanding caught here costs one message; the same one caught
after the plan converges costs the whole plan.

**Gate 2 — after the plan converges.** Planner and Devil's Advocate iterate
unattended, then stop and show the board before any code is written.

**Gate 3 — before any harness change.** Nothing installs without approval. A new
hook changes every contributor's workflow, not just the agent's.

Gate 3 is the one worth defending. If the other two decay into reflexive
approvals they cost a little time; if Gate 3 does, the repo acquires controls
nobody agreed to.

## Execution

Sequential. The CLI enforces one task in flight, completed dependencies, and
the task attempt budget. Commit implementation before final check + scope;
review and done reject uncommitted source or stale evidence.
One task in flight at a time, in `depends_on` order:

```bash
$OFFICE next          # the next ready task, or exit 1
$OFFICE claim <id>    # pending → in-progress, increments attempts
$OFFICE check <id>    # run the executable DoD
$OFFICE scope <id>    # committed + working changes since base_commit
$OFFICE review <id>   # hand to Reviewer; current check + scope evidence required
$OFFICE retry <id> --reason "..." # review -> in-progress, or blocked at the cap
$OFFICE done <id>     # Reviewer only — records branch and commit
$OFFICE block <id> --reason "..."   # escalate to a human
```

## Loop bounds

`plan_iterations` and `review_iterations` (default 3) bound the two loops. On
exceeding one, stop and escalate with both positions stated. A fourth pass at
the same disagreement is not converging, it is spinning.

## Specifications

Read `office-spec` for every new bug, feature, or refactor, including compact
trivial requests. The Product Owner turns informal input into overview.md, obtains
Gate 1 approval tied to its hash, and the Planner links tasks to requirement and
criterion IDs. `claim`, `check`, `review`, and `done` reject stale specs. Preserve
legacy boards explicitly; do not remove spec markers to bypass approval.
