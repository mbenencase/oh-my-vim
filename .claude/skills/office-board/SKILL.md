---
name: office-board
description: The kanban protocol for the-office — how to read board state, move tasks between statuses, and pick the next task. Use whenever you need board state or need to transition a task, instead of reading or editing task frontmatter directly.
---

# Board protocol

The board is Markdown files with YAML frontmatter, but **do not read state by
reading them and do not write state by editing them**. Use the CLI. It is
deterministic, it enforces legal transitions, and it keeps the human-authored
parts of each file untouched.

```bash
OFFICE="node .claude/office/bin/office.mjs"
```

## Reading

```bash
$OFFICE board              # whole board
$OFFICE board <feature>    # one feature
$OFFICE next               # next ready task id; exit 1 if none
$OFFICE validate           # integrity: schema, dupes, orphans, cycles
```

`next` returns a task only when it is `pending` **and** every `depends_on` is
`completed`. Exit 1 has three distinct meanings and the message says which:
work is in flight, something is blocked and needs a human, or the board is done.

## Transitions

```
pending ──claim──> in-progress ──review──> review ──done──> completed
   ^                    │                     │
   └────────────────────┴─────block───────────┴──> blocked (human required)
```

| Command | From → To | Side effect |
|---|---|---|
| `claim <id>` | pending, blocked → in-progress | increments `attempts` |
| `review <id>` | in-progress → review | — |
| `done <id>` | review, in-progress → completed | records `branch` + `commit` from git |
| `block <id> --reason "..."` | any → blocked | appends the reason to Notes |

Illegal transitions exit 1 rather than doing something surprising. `claim` on an
already-claimed task fails on purpose — under sequential execution that means
two things are running.

Only the Reviewer runs `done`. A SWE that marks its own work complete has
removed the review from the pipeline.

## Verification

```bash
$OFFICE check <id>   # run the task's checks; exits on first failure
$OFFICE scope <id>   # assert the working diff stays inside scope globs
```

Both are gates, not advisories. A task with a failing check is not done no
matter how it looks.

## Escalating

`block` requires `--reason` because a blocked task with no reason is
unactionable by the human who has to pick it up. When `attempts` exceeds
`max_attempts`, block rather than trying again — a fourth attempt at the same
misunderstanding produces the same result.

## Sequential execution

One task in flight. `next` will refuse while something is `in-progress` or
`review`. This is deliberate for v1: the board has no locking, and parallel
writers would corrupt it.
