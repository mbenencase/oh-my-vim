---
description: Execute the next ready task on the board through the SWE and Reviewer loop.
---

Execute the next ready task.

```bash
node .claude/office/bin/office.mjs next
```

If it exits 1, report why — work in flight, something blocked, or the board is complete — and stop.

Otherwise, for that task id:

1. Read the task file. Note its `tier:`.
2. Delegate to the matching SWE — `fast` → `office-swe-fast`, `standard` → `office-swe`, `deep` → `office-swe-deep`. On the first attempt it claims the task; on retry it resumes the already in-progress task. It establishes the baseline appropriate to verification_mode, implements, leaves a sensor behind, commits source, runs check + scope, and hands to review.
3. Delegate to `office-reviewer`. It runs the checks and the scope assertion, reviews through all three lenses, logs findings with a class, and either sends back through `office retry <id> --reason "..."` or marks the task done.
4. Repeat the SWE↔Reviewer loop up to `review_iterations` from `.the-office/config.yml`. On exceeding it, `office block` the task and escalate.

Report what changed and what the checks proved.
