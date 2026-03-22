# Domain Module Colocation

## Summary

Three small structural improvements in `src/domain/`: flatten `domain/lock/` to a single file, merge `workflow_action.rs` into `workflow_actions.rs`, and rename `plan.rs` to `diff.rs`.

## Motivation

- `domain/lock/` is a directory with one concept (`Lock` + `LockEntry`). It doesn't need to be a directory.
- `domain/workflow_action.rs` is 16 lines — a single struct. It's imported by `workflow_actions.rs` which defines the collection types that operate on it. These belong together.
- `domain/plan.rs` contains `ManifestDiff`, `LockDiff`, and `WorkflowPatch` — these are diffs/patches, not plans. The name misleads.

## Spec gate

Skipped — internal restructuring with no user-visible change.

## Changes

### 1. Flatten `domain/lock/` → `domain/lock.rs`

Merge `domain/lock/mod.rs` (136 logic lines) + `domain/lock/tests.rs` (275 lines) into a single `domain/lock.rs` with an inline `#[cfg(test)] mod tests` block.

Total: ~411 lines (136 logic + 275 tests). Within the 550-line total budget, and 136 logic lines is well under the 300 target.

Update `domain/mod.rs`: `pub mod lock;` stays unchanged (Rust resolves `lock.rs` instead of `lock/mod.rs`).

### 2. Merge `workflow_action.rs` into `workflow_actions.rs`

Move the `WorkflowAction` struct (16 lines) into the top of `workflow_actions.rs`, which already imports it via `use super::workflow_action::WorkflowAction`.

Remove `domain/workflow_action.rs`. Update `domain/mod.rs` to remove `pub mod workflow_action;`. All external imports change from `crate::domain::workflow_action::WorkflowAction` to `crate::domain::workflow_actions::WorkflowAction`.

`workflow_actions.rs` grows from 342 → ~355 lines (207 logic + ~148 tests). Well within budgets.

### 3. Rename `domain/plan.rs` → `domain/diff.rs`

The file contains:
- `ManifestDiff` — changes to apply to a manifest
- `LockDiff` — changes between two lock states
- `WorkflowPatch` — pin map for workflow files

All are diff/patch types. "plan" is misleading — `upgrade::Plan` (in `upgrade/types.rs`) is an actual plan. These are diffs.

Update `domain/mod.rs`: `pub mod plan;` → `pub mod diff;`. Update all `use crate::domain::plan::` → `use crate::domain::diff::` (10 files; `infra/workflow_update.rs` has two import sites — module-level and inside `#[cfg(test)]`).

## Ordering

Changes 1 and 2 are independent. Change 3 depends on change 1 because `lock/mod.rs` imports `use super::plan::LockDiff` — after the rename, the flattened `lock.rs` must use `super::diff::LockDiff`. Recommend landing in sequence:

1. Rename `plan.rs` → `diff.rs` (pure rename, smallest blast radius)
2. Merge `workflow_action.rs` into `workflow_actions.rs` (small, mechanical)
3. Flatten `domain/lock/` (larger, involves moving tests)

## Risks

- **Flattening `domain/lock/`**: The `#[path = "tests.rs"]` pattern in `mod.rs` becomes an inline `#[cfg(test)] mod tests` block. Test helper functions (`make_key`, `make_commit`, `set_action`) move inline. No functional change.
- **Renaming `plan.rs` → `diff.rs`**: Touches every file that imports `crate::domain::plan::`. Mechanical but wide blast radius (10 files). Rust compiler catches all misses.
- **`domain/manifest/mod.rs`** also has 211 logic lines but is NOT included here — it's a multi-concept module (Manifest struct + parsing) that needs the `infra-mod-colocation` treatment, not flattening.
