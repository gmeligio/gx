## 1. Rename `plan.rs` → `diff.rs`

- [x] 1.1 Rename `src/domain/plan.rs` to `src/domain/diff.rs`
- [x] 1.2 Update `src/domain/mod.rs`: `pub mod plan;` → `pub mod diff;`
- [x] 1.3 Update all `use crate::domain::plan::` imports to `use crate::domain::diff::` (10 files; note `infra/workflow_update.rs` has two import sites)
- [x] 1.4 Run `cargo check` to verify no broken imports

## 2. Merge `workflow_action.rs` into `workflow_actions.rs`

- [x] 2.1 Copy `WorkflowAction` struct and its imports from `workflow_action.rs` into the top of `workflow_actions.rs`
- [x] 2.2 Update `workflow_actions.rs` internal import: `use super::workflow_action::WorkflowAction` → remove, use local type
- [x] 2.3 Update all external `use crate::domain::workflow_action::` imports to `use crate::domain::workflow_actions::` (4 files)
- [x] 2.4 Delete `src/domain/workflow_action.rs`
- [x] 2.5 Update `src/domain/mod.rs`: remove `pub mod workflow_action;`
- [x] 2.6 Run `cargo check` to verify no broken imports

## 3. Flatten `domain/lock/` → `domain/lock.rs`

- [x] 3.1 Create `src/domain/lock.rs` with contents of `lock/mod.rs`
- [x] 3.2 Append `lock/tests.rs` as an inline `#[cfg(test)] mod tests` block in `lock.rs`
- [x] 3.3 Remove the `#[path = "tests.rs"]` attribute and adjust test module declaration
- [x] 3.4 Delete `src/domain/lock/` directory
- [x] 3.5 Run `cargo check` to verify module resolution works

Note: Step 3 depends on step 1 — `lock/mod.rs` imports `use super::plan::LockDiff`, which becomes `use super::diff::LockDiff` after the rename.

## 4. Final verification

- [x] 4.1 Run `cargo test -p gx -- domain::lock` and `cargo test -p gx -- domain::diff` — lock and diff tests pass after restructuring
- [x] 4.2 Run `cargo test` — all unit tests pass (e2e failures are pre-existing, network-dependent)
- [x] 4.3 Run `cargo clippy` — no new warnings
