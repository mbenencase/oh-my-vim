---
description: Show the-office kanban board and its integrity status.
argument-hint: [feature slug]
---

Run these and report the result. Do not read task frontmatter directly — the CLI is the source of truth.

```bash
node .claude/office/bin/office.mjs board $ARGUMENTS
node .claude/office/bin/office.mjs validate
node .claude/office/bin/office.mjs findings recur
```

Summarise: what is in flight, what is blocked and why, whether the board is valid, and whether any finding class has crossed the recurrence threshold. If something is blocked, say what a human needs to decide.
