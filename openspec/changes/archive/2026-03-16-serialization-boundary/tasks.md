# Tasks: Fix domain/infra serialization boundary

Assumes `resolved-version-comment` has been applied.

## 1. Create ResolvedAction struct
- [x] Add `ResolvedAction { id: ActionId, sha: CommitSha, version: Option<Version> }` to `domain/action/resolved.rs`

## 2. Create format_uses_ref function
- [x] Add private `format_uses_ref(action: &ResolvedAction) -> String` to `infra/workflow_update.rs`
- [x] Handles `Some(version)` → `"{sha} # {version}"` and `None` → `"{sha}"`

## 3. Change WorkflowPatch.pins type
- [x] Update `WorkflowPatch.pins` from `Vec<(ActionId, String)>` to `Vec<ResolvedAction>` in `domain/plan.rs`

## 4. Refactor tidy/patches.rs
- [x] Rename `build_file_update_map()` → `build_pins()` (or similar)
- [x] Return `Vec<ResolvedAction>` instead of `HashMap<ActionId, String>`
- [x] Replace `format!("{} # {}", ...)` with `ResolvedAction` construction
- [x] Update `compute_workflow_patches()` to collect into `WorkflowPatch.pins`

## 5. Refactor upgrade/plan.rs
- [x] `apply_upgrade_workflows()`: Build `Vec<ResolvedAction>` from `lock_diff.added` instead of formatted strings
- [x] Remove generic `<W: WorkflowUpdater>` bound, accept `&WorkflowWriter` directly
- [x] Remove `Updater as WorkflowUpdater` import

## 6. Rename FileUpdater to WorkflowWriter
- [x] Rename struct in `infra/workflow_update.rs`
- [x] Update all imports across the codebase:
  - `tidy/mod.rs`, `upgrade/mod.rs`, `init/mod.rs`
  - `tests/integ_upgrade.rs`, `tests/integ_tidy.rs`, `tests/common/setup.rs`
  - `src/tidy/tests.rs`, `src/infra/workflow_update.rs` test module

## 7. Refactor WorkflowWriter API
- [x] Change `update_all()` to accept `&[WorkflowPatch]` instead of `&HashMap<ActionId, String>`
- [x] Change `update_file()` / `update_workflow_internal()` to use `format_uses_ref()` internally
- [x] Delete `impl Updater for FileUpdater` block

## 8. Delete Updater trait
- [x] Delete `Updater` trait from `domain/workflow.rs`
- [x] Remove `Updater` from all imports

## 9. Update tidy/mod.rs
- [x] `apply_workflow_patches()`: Remove generic bound, accept `&WorkflowWriter`
- [x] Remove `HashMap` conversion from `patch.pins`
- [x] Pass patches directly to writer

## 10. Delete Lock::build_update_map
- [x] Delete method from `domain/lock/mod.rs` (lines 173-183)
- [x] Delete tests in `domain/lock/tests.rs`
- [x] Refactor `tests/integ_upgrade.rs:288` to use new workflow output path

## 11. Verify
- [x] `cargo check` passes
- [x] `cargo clippy` passes
- [x] `cargo test` passes
- [x] No `"# "` string in domain layer (only in `infra/workflow_update.rs`)
