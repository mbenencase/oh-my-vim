---
name: office-spec
description: Turn an informal request into a typed bug, feature, or refactor specification, obtain content-bound human approval, and link its requirements and acceptance criteria to office tasks.
---

# Typed specifications

Users may describe requests informally. Investigate repository context and fill
`.the-office/features/<slug>/overview.md`; do not ask the user to author a template.
Use `office feature new <slug> --type bug|feature|refactor --title "..."`.
Keep one overview as the specification. Optional `design.md` holds technical
choices; task files hold implementation details and plan iteration notes.

## Contract

Frontmatter uses the CLI's YAML subset: maps, scalar values, and scalar lists.
Use stable, feature-local requirement IDs and criterion IDs:

```yaml
spec_version: 1
id: orders-filter
type: feature
requirements:
  REQ-001: Users can filter their own orders by status.
acceptance_criteria:
  AC-001:
    requirement: REQ-001
    description: Filtering by pending returns only the authenticated user's pending orders.
open_questions: []
```

Fill every template section. Use `None` or a reasoned `Not applicable` where
appropriate. Preserve the original request, observable outcomes, scope exclusions,
constraints, and assumptions with their basis. Ask only about ambiguities that
materially change work. Resolved questions become decisions or accepted assumptions;
do not silently delete unresolved concerns to obtain approval.

- Bug: record current/expected behavior, reproduction, environment/evidence,
  and hypotheses separately from confirmed causes. Unknown causes are allowed;
  plan bounded investigation rather than inventing a diagnosis.
- Feature: cover user flows, business rules, data/integration contracts, and
  relevant failure/empty states. Frontend and backend share one outcome spec.
- Refactor: state the structural problem, change boundary, preserved behaviors,
  and migration strategy. Avoid inventing product functionality.

## Approval and planning

Run `office spec validate <slug>`, then `office spec status <slug>` to obtain the
content hash. Show the spec to the human at existing Gate 1. Only after confirmation,
record `office spec approve <slug> --hash <shown-hash> --by <human-reference>`.
The hash must identify the exact reviewed content. The CLI records confirmation;
it does not authenticate the named person or independently prove consent.

Planner adds `requirements: [REQ-001]`, `acceptance_criteria: [AC-001]`, and
`verification_mode: acceptance|regression|preservation` to each task. Cover every
criterion across the plan and run `office spec validate <slug> --plan` plus
`office validate` before Gate 2. Both requirement and criterion references must
agree. Supporting tasks can contribute to a criterion without proving it alone.

Source/contract verification includes the spec hash. Editing the spec invalidates
its approval and existing task evidence. Obtain renewed confirmation, reassess
completed and pending tasks affected by the change, and rerun verification for
active tasks. Completed tasks remain historical records; add correction tasks
when necessary. Do not silently treat old completion as proof of a changed spec.

## Evidence by work type

- Regression: reproduce the defect; a targeted test fails for that defect before
  the fix and passes after. Missing dependencies/test files are not reproduction.
- Acceptance: demonstrate new behavior with targeted outcome assertions.
- Preservation: characterize existing behavior and keep those checks green before
  and after. A new structural constraint may have its own failing-then-passing test.

Build/type/lint/regression gates can already be green; do not force artificial
failures. Every task still needs executable checks. The Reviewer judges whether
those checks actually support the declared criteria, including manual evidence
where appropriate. Structural traceability does not prove semantic completeness.
