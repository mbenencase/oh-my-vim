## TypeScript conventions

Feedforward controls. Anything here a machine could check instead is already a
sensor — see `.the-office/harness.md`.

- No `any`. Where a type is genuinely unknown, use `unknown` and narrow at the
  boundary. `any` disables the single most valuable sensor in this stack.
- Types at module boundaries are explicit. Inference inside a function is good;
  inference across an export is a silent contract change waiting to happen.
- Discriminated unions over optional-field soup. `{kind: 'ok', value} | {kind:
  'err', error}` beats `{value?, error?, ok?}` — the compiler can exhaust the
  first and cannot exhaust the second.
- Errors are values at boundaries the caller is expected to handle, and thrown
  exceptions only for genuinely exceptional states. Pick one per module and
  stay with it.
- No barrel files (`index.ts` re-exporting a whole directory). They defeat
  tree-shaking and turn every module boundary into a cycle risk.
- `async` functions always have their rejections handled. A floating promise is
  a silent failure — enable `no-floating-promises` once type-aware linting lands.
- Tests colocate as `*.test.ts` beside the unit under test, or in `tests/`
  mirroring `src/`. Pick one per repo; do not mix.
