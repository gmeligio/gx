# Code Health Gates

## Summary

Add new code health gates to enforce colocation principles and structural rules before starting file restructuring. Uses the existing budget-and-ratchet pattern from `code_health.rs`.

## Motivation

The codebase is about to undergo a series of restructuring changes (dissolve `upgrade/types.rs`, extract logic from `mod.rs` files, merge small files, etc.). Without gates in place first, restructuring could introduce new violations or regress in ways that aren't caught until later.

Two structural rules emerged from analysis that have no enforcement today:

1. **`mod.rs` should be reexports only** — currently 11 `mod.rs` files contain business logic (up to 354 lines). Nothing prevents adding more logic to `mod.rs` during restructuring.
2. **File names should be semantic** — `types.rs`, `utils.rs`, `helpers.rs` don't describe what the code does. One violation exists (`upgrade/types.rs`).

Additionally, the existing `file_size_budget` gate counts tests against the budget, which conflates "file is doing too much" with "file has thorough tests." A logic-only budget would be more precise.

## Spec gate

Skipped — internal tooling with no user-visible change.

## Changes

### 1. Add `logic_line_budget` gate

Count lines per `.rs` file excluding `#[cfg(test)]` blocks and standalone `tests.rs` files. Set initial budget to **440** (current max is 438 in `infra/github/resolve.rs`). Target: **300**.

Rationale: 90% of production files already have < 270 logic lines. The 300 target matches the natural breakpoint. The 440 budget gives 2 lines of headroom so minor edits don't immediately break the gate.

The counting must handle two patterns:
- **Inline tests**: `#[cfg(test)]` to end-of-module — subtract these lines
- **Standalone `tests.rs`**: Files included via `#[path = "tests.rs"]` or `mod tests;` under `#[cfg(test)]` — exclude entirely from the logic budget

Current files that would exceed a 300-line target (these are the restructuring candidates):

| File | Logic lines | Why it's large |
|------|------------|----------------|
| `infra/github/resolve.rs` | 438 | GitHub API + version resolution in one file |
| `lint/mod.rs` | 354 | Command + rule dispatch + types in mod.rs |
| `infra/workflow_scan/mod.rs` | 300 | Scanner logic in mod.rs |
| `domain/action/identity.rs` | 276 | 6 newtypes — cohesive, borderline |

### 2. Add `mod_rs_reexports_only` gate

Scan all `mod.rs` files. Count lines that are not:
- `mod` / `pub mod` / `pub(crate) mod` declarations
- `pub use` / `use` reexports
- Attributes (`#[...]`)
- Doc comments (`///`, `//!`, `//`)
- Blank lines
- `#[cfg(test)]` block (allowed for `mod tests;`)

Set initial budget to **360** (current max: `lint/mod.rs` at 354, plus headroom for multi-line `use` block continuation lines that the heuristic may miscount). Budget applies **per-file** (the single worst `mod.rs` must not exceed it). Target: **0**.

Current offenders:

| File | Logic in mod.rs |
|------|----------------|
| `lint/mod.rs` | 354 |
| `infra/workflow_scan/mod.rs` | 300 |
| `tidy/mod.rs` | 259 |
| `domain/manifest/mod.rs` | 211 |
| `infra/manifest/mod.rs` | 193 |
| `infra/github/mod.rs` | 263 |
| `domain/lock/mod.rs` | 136 |
| `infra/lock/mod.rs` | 125 |
| `upgrade/mod.rs` | 107 |
| `init/mod.rs` | 81 |
| `domain/mod.rs` | 6 (Parsed<T> — borderline) |

### 3. Add `no_generic_file_names` gate

Denylist: `types.rs`, `utils.rs`, `helpers.rs`, `common.rs`, `misc.rs`, `consts.rs`, `constants.rs`. Set budget to **1** (current: `upgrade/types.rs`). Target: **0**.

### 4. Keep existing `file_size_budget` as-is

The total-lines gate (550, target 500) still catches files where even tests have grown excessively. It complements the logic-only budget.

## Ordering

All four changes are independent and can land in a single commit. The gates should use the budget pattern: set budgets to current-state maximums, document the target in comments, and ratchet down as restructuring proposals land.

## Risks

- **Logic line counting** is the trickiest part. The `#[cfg(test)]` marker isn't always at the same indentation level, and standalone `tests.rs` files need special handling. Existing helpers (`find_path_includer`, `find_tests_rs_includer`) can be reused.
- **`mod_rs_reexports_only` heuristic** could have false positives for small glue types like `Parsed<T>` in `domain/mod.rs` (6 lines). The budget approach handles this — it's under any reasonable budget.
- **No restructuring in this change** — gates only. All current code must pass. If a gate fails on the current codebase, the budget is wrong.
