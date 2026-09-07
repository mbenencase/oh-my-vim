---
name: office-swe-protocol
description: The execution contract for the SWE role in the-office — scope discipline, running the executable definition of done, and the obligation to leave a sensor behind. Use when implementing a task from the board.
---

# SWE protocol

```bash
OFFICE="node .claude/office/bin/office.mjs"
```

## 1. Claim or resume exactly one task

```bash
$OFFICE next
$OFFICE claim <id>
```

On a Reviewer retry, the CLI has already returned the task to `in-progress`
and incremented `attempts`; resume that task without calling `next` or `claim`.
For a new task, commit pre-existing source and installed harness changes first.
`claim` records the initial Git `base_commit` and increments `attempts`. If it reports you are over `max_attempts`, do not
start — escalate.

Read the whole task file, including Notes. On a retry, Notes holds the
Reviewer's findings and the previous attempt's reasoning. Re-reading them is
cheaper than rediscovering why the last attempt failed.

## 2. Establish the appropriate baseline

Read the `office-spec` skill and the task's `verification_mode`.
For regression work reproduce the defect and demonstrate a failing targeted test.
For new behavior write outcome checks. For preservation work establish green
characterization tests before editing and keep them green afterward. Build,
typing, lint, and existing regression checks may already pass. A failure caused
only by a missing test file or dependency is not evidence of the intended defect.
Run `$OFFICE check <id>` after claim; inspect why any check fails.

## 3. Implement, inside scope

Only this task. If you spot a real problem outside it, write it in Notes and
keep going. Silently widening scope turns a reviewable change into an
unreviewable one.

If you genuinely need a file outside `scope:`, widen the field in the task file
and say why in Notes — the decision should be visible in the diff, not inferred
from it.

```bash
$OFFICE scope <id>
```

## 4. Leave a sensor behind

The obligation that separates this role from ordinary implementation.

If the task established an invariant — a boundary that must hold, a required
call order, a format, something that must never happen again — a machine should
be enforcing it before you finish. Record it in `sensors_added`.

Ask: **if someone violated this rule tomorrow, what would catch it?**

- "A careful reviewer" → not finished.
- "Nothing" → you have left the next person a trap.
- A type, a lint rule, a test, a fitness function → done.

Prefer the cheapest control that works, in that order. A type costs nothing at
runtime and catches the defect before it is written.

Not every task establishes an invariant. Inventing one to fill the field is
worse than leaving it empty.

## 5. Commit, verify, then hand off

Commit the implementation first. Final evidence must belong to that commit;
committing after verification invalidates it. Board task metadata can remain dirty.

```bash
$OFFICE check <id>    # every check, exit 0
$OFFICE scope <id>
$OFFICE review <id>
```

Run the commands. Do not substitute reading the code and concluding it looks
right — that is the judgement the checks exist to replace.

Never edit a check to make it pass unless the check itself was wrong. If it was,
say so explicitly in Notes so the Reviewer evaluates that decision rather than
discovering it.

Do not run `$OFFICE done`. Completion is the Reviewer's call.

## Retries

The Reviewer runs `office retry <id> --reason "..."` to send findings back.
This increments attempts, clears evidence, and blocks when the budget is exhausted.
The original base commit remains unchanged. Address them, re-run the
checks, hand back. You have `max_attempts` total.

On the last attempt, if it is still not right:

```bash
$OFFICE block <id> --reason "<what is actually blocking, in one sentence>"
```

Escalating is a correct outcome. Spending a fourth attempt on the same
misunderstanding is not.
