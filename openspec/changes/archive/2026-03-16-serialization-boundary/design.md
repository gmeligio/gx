# Design: Fix domain/infra serialization boundary

## Approach

Replace stringly-typed workflow pin data with a domain struct, move all YAML comment formatting to a single infra function, and delete the unnecessary `Updater` trait.

## New type

```rust
// domain/action/resolved.rs
pub struct ResolvedAction {
    pub id: ActionId,
    pub sha: CommitSha,
    pub version: Option<Version>,
}
```

`version` is `None` when the specifier is a bare SHA (no `# comment` annotation needed).

## New function

```rust
// infra/workflow_update.rs
fn format_uses_ref(action: &ResolvedAction) -> String {
    match &action.version {
        Some(v) => format!("{} # {v}", action.sha),
        None => action.sha.to_string(),
    }
}
```

This is the **single place** where `"SHA # version"` formatting exists.

## Changes by module

### domain/plan.rs
- `WorkflowPatch.pins`: Change type from `Vec<(ActionId, String)>` to `Vec<ResolvedAction>`.
- Add import for `ResolvedAction`.

### domain/workflow.rs
- Delete the `Updater` trait (lines 63-84). Keep `Scanner` trait and `Error` enum.

### domain/lock/mod.rs
- Delete `build_update_map()` method (lines 173-183) and its tests in `domain/lock/tests.rs`.

### tidy/patches.rs
- `build_file_update_map()` → rename to `build_pins()` (or similar).
- Return `Vec<ResolvedAction>` instead of `HashMap<ActionId, String>`.
- Replace the `format!("{} # {}", ...)` / `is_sha()` branch with `ResolvedAction` construction:
  ```rust
  let version = if spec.version.is_sha() { None } else { Some(res.version.clone()) };
  ResolvedAction { id: spec.id.clone(), sha: commit.sha.clone(), version }
  ```
- `compute_workflow_patches()`: Update to collect `Vec<ResolvedAction>` into `WorkflowPatch.pins`.

### tidy/mod.rs
- `apply_workflow_patches()`: Remove generic `<W: WorkflowUpdater>` bound. Accept `&WorkflowWriter` directly.
- Remove the `HashMap` conversion from `patch.pins`. Pass patches to writer directly.
- Update import: remove `Updater as WorkflowUpdater`, change `FileUpdater` → `WorkflowWriter`.

### upgrade/plan.rs
- `apply_upgrade_workflows()`: Remove generic `<W: WorkflowUpdater>` bound. Accept `&WorkflowWriter` directly.
- Build `Vec<ResolvedAction>` from `lock_diff.added` instead of formatted strings.
- Remove `Updater as WorkflowUpdater` import.

### upgrade/mod.rs
- Update `FileUpdater` → `WorkflowWriter` import and instantiation.

### init/mod.rs
- Update `FileUpdater` → `WorkflowWriter` import and instantiation.

### infra/workflow_update.rs
- Rename `FileUpdater` → `WorkflowWriter`.
- Add `format_uses_ref()` function.
- Delete `impl Updater for FileUpdater` block (lines 139-159).
- Refactor `update_workflow_internal()` to accept `&[ResolvedAction]` (or equivalent from `WorkflowPatch`) and call `format_uses_ref()` for each pin.
- Refactor `update_all()` to accept `&[WorkflowPatch]`.

### Tests
- `domain/lock/tests.rs`: Delete `build_update_map` and `build_update_map_missing_sha_falls_back_to_version` tests.
- `tests/integ_upgrade.rs` (line 288): Refactor to build `WorkflowPatch` with `Vec<ResolvedAction>` instead of calling deleted `build_update_map()`.
- All test files importing `FileUpdater`: Update to `WorkflowWriter`.
- `infra/workflow_update.rs` tests: Update to new API signatures.

## Design decisions

1. **No trait replacement for `Updater`** — `WorkflowWriter` is used directly. If a test trait is ever needed, it can be introduced then. YAGNI.
2. **`format_uses_ref` is a private function** — only called inside `WorkflowWriter` methods. No need to expose it.
3. **`ResolvedAction` goes in `domain/action/resolved.rs`** — alongside the existing `Resolved` (which becomes `RegistryResolution` in the next proposal). Both are "resolution results" at different stages.
