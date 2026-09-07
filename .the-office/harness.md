# Harness manifest

<!--
  Written by the Office Manager, maintained by the Janitor.
  This file is the single place that answers "what regulates this repo, and why".
  Every row names which cell of the control matrix it occupies, because a
  harness made only of inferential sensors is the failure mode this exists to
  prevent.
-->

**Repo class:** legacy  ·  **Stacks:** rust
**Harnessability:** 87/100 (high)
**Last audited:** 2026-08-26 · `office audit`

## Score components

| Component | Score | Max | Reading |
|---|---|---|---|
| Typing | 25 | 25 | How much a compiler proves before a test runs |
| Boundaries | 14 | 20 | Whether module structure is legible to an agent |
| Tests | 25 | 25 | Whether behaviour is verifiable at all |
| Build | 15 | 15 | Whether a checkout reproduces |
| Controls | 8 | 15 | Share of the stack's expected controls present |

The audit's "controls 8/15" undercounted on arrival: it looks for `clippy.toml`
and found none, while `.github/workflows/ci.yml` had been running `cargo fmt
--check`, `cargo clippy -D warnings` and `cargo test` since `cc705d7`. The real
finding was not missing strictness. It was that five load-bearing architectural
invariants were documented in prose and enforced by nothing.

## Controls in place

| Control | Cell | Check command | Installed |
|---|---|---|---|
| `F1 dep-direction` — workspace graph matches the documented one | computational-sensor | `./.github/scripts/arch-check.py` | 2026-08-26 |
| `F2 async-quarantine` — only `omv-lsp` may declare `tokio` | computational-sensor | `./.github/scripts/arch-check.py` | 2026-08-26 |
| `F3 core-purity` — no terminal/process/net/env/LSP in `omv-core` or `omv-syntax`; `std::fs` only in `omv-core`'s `buffer.rs`, never in `omv-syntax` | computational-sensor | `./.github/scripts/arch-check.py` | 2026-08-26, widened to `omv-syntax` 2026-09-07 |
| `F4 keymap-sync` — `assets/default.yaml` byte-equals `examples/config.yaml` | computational-sensor | `./.github/scripts/arch-check.py` | 2026-08-26 |
| `F5 unreachable-action` — every `actions!` entry is key-bound or `:command`-reachable | computational-sensor | `./.github/scripts/arch-check.py` | 2026-08-26 |
| Lint policy in the manifest, not CI YAML (`clippy::all = deny`, `unsafe_code = forbid`) | computational-guide | `cargo clippy --workspace --all-targets` | 2026-08-26 |
| Cast lints denied in `omv-core` only, existing sites grandfathered with `#[expect]` | computational-sensor | `cargo clippy -p omv-core` | 2026-08-26 |
| rustfmt, pinned to edition 2024 | computational-sensor | `cargo fmt --all --check` | pre-existing; `rustfmt.toml` added 2026-08-26 |
| clippy thresholds (cognitive complexity, arg count, type complexity) | computational-sensor | `cargo clippy --workspace --all-targets` | `clippy.toml` added 2026-08-26 |
| Lockfile freshness | computational-sensor | `cargo metadata --locked --format-version 1` | 2026-08-26 |
| `--locked` on every CI build | computational-sensor | `.github/workflows/ci.yml`, `release.yml` | 2026-08-26 |
| CI runs on pushes to `development`, not only on PRs | computational-sensor | `.github/workflows/ci.yml` | 2026-08-26 |
| PR title must be a conventional commit (**fails** the build) | computational-sensor | `.github/workflows/ci.yml` | 2026-08-26 |
| Per-commit subjects advisory (`::warning::`) | computational-sensor | `.github/workflows/ci.yml` | pre-existing |
| `commit-msg` hook rejects unclassifiable subjects locally | computational-sensor | `.githooks/commit-msg` | 2026-08-26 |
| `pre-commit`: fmt + fitness functions + clippy | computational-sensor | `.githooks/pre-commit` | 2026-08-26 |
| `pre-push`: tests + test-count ratchet | computational-sensor | `.githooks/pre-push` | 2026-08-26 |
| Test suite | computational-sensor | `cargo test --workspace --locked` | pre-existing |
| Test-count ratchet (floor 83) | computational-sensor | `.github/workflows/ci.yml`, `.githooks/pre-push` | 2026-08-26 |
| Advisories / licences / sources | computational-sensor | `cargo deny check` | 2026-08-26, **scheduled weekly, not a PR gate** |
| `CLAUDE.md` | inferential-guide | — | pre-existing; merged with the rust pack 2026-08-26 |

**Hooks are not active until a human opts in**, once per clone:

```bash
git config core.hooksPath .githooks
```

This was left as a deliberate manual step. A hook changes every contributor's
workflow, and the tooling's permission system declined to set it on their behalf.

## Gaps, and why they are still gaps

<!--
  A gap left open on purpose is a decision; a gap left open by accident is a
  bug. Record which each one is. "Not worth it for this repo" is a legitimate
  entry — an unexplained blank is not.
-->

| Gap | Cost | Why not yet |
|---|---|---|
| **Coverage floor** | `cargo-llvm-cov` install + a CI job | **Decision.** Coverage has never been measured here. A floor pinned to an unknown baseline blocks the next commit. Measure first, then set one. |
| **MSRV 1.90 is declared but never verified** | one CI job on a pinned toolchain | **Decision, pending measurement.** `rust-version = "1.90"` is in `Cargo.toml`; CI uses `@stable` (1.97.1). Only stable is installed locally, so it was not possible to check whether the tree still compiles on 1.90. Until someone runs it once, a CI job could be red on arrival. |
| **Newtypes for the three coordinate systems** (`CharIdx` / `ByteIdx` / `Utf16Col`) | a refactor across `omv-core`, `omv-syntax`, `omv-lsp` | **Decision.** This is the highest-value computational *guide* still available — it would move the repo's worst documented bite into the type system. But it is a feature-sized change, not a harness install. The cast lints in `omv-core` are the cheap partial sensor standing in for it. |
| **"One channel, many producers"** | — | **Decision.** No cheap computational form exists. Grepping for a second `mpsc::channel` in the render loop would be mostly false positives. Stays prose in `CLAUDE.md`, and `CLAUDE.md` says so explicitly. |
| **`clippy::unwrap_used`** | 12 call sites | **Decision.** Would fail on arrival. Recorded as a ratchet below and written into `CLAUDE.md` as a convention instead. |
| **`clippy::pedantic`** | 170 warnings | **Decision.** Measured, deliberately not enabled. 65 of the 170 are `must_use` suggestions and 11 are missing `# Errors` doc sections — low signal for this codebase, and enabling it repo-wide is exactly the error-wall failure mode. |
| **`publish = false` on the six crates** | 6 one-line manifest edits | **Decision deferred to a human.** It would let `cargo deny`'s `wildcards` check move from "warn" to "deny". It is accurate today (internal path deps carry no version, so crates.io would reject them) but it forecloses publishing, which is a project decision rather than a harness one. |
| **Doc freshness** (README/CLAUDE claims vs reality) | — | **Partly closed.** Two real drifts were found and fixed (a `61 tests` claim that was 83, and a `**No splits.**` limitation contradicting the `## Windows` section added by `f06df4c`). General prose-vs-code drift stays an inferential-sensor job for a review agent. |
| **A silent new *external* dependency** | — | **Open, and not closed by the F3 widening.** `F1 dep-direction` intersects each package's deps with the in-workspace set, so an external crate (`pulldown-cmark`, say) is invisible to it *by construction*. A task can promise "no new crate dependency" and nothing reads it. It held for `omv-syntax`'s parser only because that task's `office scope` glob was `crates/omv-syntax/src/**`, which excludes `Cargo.toml` — incidental to how one task declared scope, not a durable guarantee. No cheap fix proposed; naming it beats implying the F3 widening covers it. |
| **Mutation testing as a sensor** (`one-sided-test-coverage`, 5 recurrences) | `cargo-mutants`, gated or scheduled | **Decision: measured, then rejected.** `cargo-mutants 25.0.1` was installed and run in disposable worktrees, not theorised about. Whole-file on `app.rs`: 9m05s, 178 mutants, **106 survivors** — unactionable. Scoped `--in-diff`: 1m45s, 8 mutants, 1 survivor — affordable. But against the five findings that motivated it, it reproduces **one**, cannot model **two** (swapping `window.buffer` for `editor.current` is an identifier substitution; no standard genome has that operator), and reports **two as caught while the defect survives** — its whole-function mutant `replace App::activate_payload with ()` is killed by a test exercising a *different* `Payload` variant, so the specific dead call stays dead and the function reads as covered. A green run would launder false confidence exactly where the reviewer says none should be trusted, which is worse than no control. The reviewer's working method is hypothesis-driven single-statement mutation, finer-grained than any off-the-shelf genome — that is a category difference, not a tuning problem. Stays an inferential sensor (Reviewer, Behaviour lens). |
| **`check-does-not-cover-dod`** (3 recurrences) | — | **Decision: stays inferential.** The Janitor built the obvious sensor — flag any `dod:` clause sharing no identifier token with `checks:` — ran it against four task files, and rejected it. It false-positives on covered prose (it cannot tell sentence-initial English from identifiers without becoming an LLM judge) and false-negatives on 2 of the 3 real instances, which were sub-clauses inside longer sentences that also contained covered content. The three instances share a symptom, not a mechanism: crate purity, structural placement, cursor behaviour. Only the first had a cheap computational form, and that is the F3 widening above. The inferential control is not failing — the Reviewer caught all three; what recurred is authoring discipline, which a reviewer sensor catches rather than prevents. |
| **`.editorconfig`** | one file | **Decision, not approved in this round.** Still reported missing by `office audit`. `rustfmt.toml` already fixes the only thing that matters for `.rs` files; an `.editorconfig` would cover YAML and Markdown. Cheap, low value, not installed without approval. |
| **`office audit` still reports `rust:tests` missing** | — | **Audit artifact, not a real gap.** The detector looks for a top-level `tests/` directory; this workspace puts unit tests in `#[cfg(test)]` modules and its integration test in `crates/omv-lsp/tests/client.rs`. 83 tests run in CI. |
| **Duplicate dependency versions** | upstream | **Decision.** 5 duplicates today (`bitflags`, `hashbrown`, `syn`, `thiserror`, `thiserror-impl`). `multiple-versions = "warn"` — resolving them means waiting on upstream, not on this repo. |

## Adoption order

<!--
  Legacy repos only. Cheap high-signal controls first, broad enforcement last.
-->

Installed in this order, each verified green before the next:

1. **Fitness functions** (`F1`–`F5`). 0.10 s, zero false positives on arrival,
   and each mutation-tested to confirm it can actually fail. Highest value in
   the repo: they were prose, and prose had already drifted twice.
2. **Lint policy relocation** into `[workspace.lints]`. No new strictness —
   `-D warnings` already ran in CI. It just now also runs on a laptop.
3. **`rustfmt.toml` / `clippy.toml`**, adapted from the pack. The pack ships
   `edition = "2021"`; this workspace is edition 2024 and was corrected, or
   `cargo fmt` would have reformatted against the wrong edition's rules.
4. **Lockfile + `--locked` + `development` branch trigger.** Green on arrival.
5. **Cast lints in `omv-core` only**, with 5 sites grandfathered.
6. **Commit-message gating**, title-only. New-code-only by construction.
7. **Hooks**, last, and inert until a human enables them.
8. **`cargo deny`**, scheduled rather than gating.

## Ratchets

<!--
  Any control whose threshold is pinned to a current measured value rather than
  a target. Record the current number and the target, so the Janitor can raise
  it as backlog clears instead of it silently staying at the floor forever.
-->

| Control | Current | Target |
|---|---|---|
| Passing tests (`cargo test --workspace`) | **83** | monotonically up; raise the floor in `ci.yml` and `.githooks/pre-push` when it rises |
| `#[expect(clippy::cast_*)]` grandfathers in `omv-core` | **3 statements, 5 cast sites** | 0. Self-cleaning: `#[expect]` un-fulfils itself once a cast is removed, so a stale grandfather fails the build. Count with `grep -rn 'expect(clippy::cast' crates/omv-core/src` |
| Cast-lint warnings across the whole workspace | **37** (`omv/src/ui.rs` 9, `omv/src/app.rs` 9, `omv-core` 5, others 14) | 0, then promote the lints from `omv-core`-only to workspace-wide |
| `clippy::pedantic` warnings | **170** | not a target; recorded so a future decision has a number |
| `unwrap()`/`expect()` in library code | **12** (`omv-lsp/src/client.rs` 6, `omv/src/window.rs` 3, `omv/src/event.rs` 1, `omv-config/src/config.rs` 1, `omv-core/src/editor.rs` 1) | 0, then enable `clippy::unwrap_used` |
| `cargo deny` wildcard warnings | **3** (all internal path deps) | 0 only if the crates are marked `publish = false` — see gaps |
| Duplicate crate versions | **5** | no target; upstream-driven |
| Coverage | **unmeasured** | measure once, then set a floor |
