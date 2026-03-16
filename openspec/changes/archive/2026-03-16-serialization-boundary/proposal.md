# Proposal: Fix domain/infra serialization boundary

## Why

This is an internal refactoring that fixes an architecture violation where YAML serialization logic leaks into the domain layer. It does not warrant a spec — no user-facing behavior changes.

## Problem

The domain layer currently constructs `"SHA # version"` strings (YAML comment syntax) and passes them as `String` through the `Updater` trait to the infra layer. This is a serialization concern leaking upward:

1. **`format!("{} # {}", sha, comment)` appears in 3 domain/tidy/upgrade files** — `tidy/patches.rs`, `upgrade/plan.rs`, and `domain/lock/mod.rs` (`build_update_map`). Each also duplicates the `if specifier.is_sha()` branch for bare SHA handling.
2. **`WorkflowPatch.pins` is `Vec<(ActionId, String)>`** where `String` is the pre-formatted `"SHA # version"`. A domain plan struct carries a serialization artifact.
3. **The `Updater` trait accepts `HashMap<ActionId, String>`** — its signature forces callers to pre-format. The trait exists in `domain/workflow.rs` but has exactly one implementation (`FileUpdater`), is never mocked in tests, and its `String` value type is a serialization leak.
4. **`Lock::build_update_map()`** is dead code — defined and tested but never called from production code. It also produces the leaked `"SHA # version"` format.

## Solution

1. **Create `ResolvedAction` struct** in `domain/action/resolved.rs`:
   ```
   ResolvedAction { id: ActionId, sha: CommitSha, version: Option<Version> }
   ```
   This is the domain's answer to "what did this action resolve to?" — pure data, no formatting. `version` is `None` for bare SHA specifiers (no annotation needed).

2. **Change `WorkflowPatch.pins`** from `Vec<(ActionId, String)>` to `Vec<ResolvedAction>`. The domain plan speaks domain types.

3. **Delete the `Updater` trait** from `domain/workflow.rs`. Replace with direct use of `WorkflowWriter` (renamed from `FileUpdater`) in infra, which accepts `&[WorkflowPatch]`.

4. **Create `format_uses_ref()`** in `infra/workflow_update.rs` — the single place where `ResolvedAction` → `"SHA # version"` serialization happens. The `#` YAML comment syntax appears exactly once in the codebase.

5. **Delete `Lock::build_update_map()`** — dead code in production that also leaked serialization. The method is used in `tests/integ_upgrade.rs` — that test must be updated to use the new workflow output path.

6. **Update `patches.rs`** to produce `Vec<ResolvedAction>` instead of formatted strings. The `build_file_update_map` function becomes `build_action_pins` (or similar), returning domain types.

7. **Update `upgrade/plan.rs`** (`apply_upgrade_workflows`) to produce `Vec<ResolvedAction>` from the lock diff and pass to `WorkflowWriter`.

## Dependencies

- **`resolved-version-comment` must apply first** — it removes `to_workflow_ref()` (which also leaks `"SHA # comment"` formatting). This proposal assumes that method no longer exists.
- **`rename-resolved` applies after** — it renames the existing `Resolved`/`ResolvedAction` (registry result) to `RegistryResolution`, resolving the temporary name clash with the new `ResolvedAction` (workflow output) introduced here.

## Non-goals

- Changing what data the workflow comment shows (done in `resolved-version-comment` proposal).
- Renaming `Resolved` → `RegistryResolution` (separate proposal).
- Introducing mock implementations of the writer (not needed; tests use real temp files).

## Outcome

- Domain layer has zero knowledge of YAML comment syntax (`#`).
- `WorkflowPatch` carries `Vec<ResolvedAction>` — domain types, not strings.
- `Updater` trait deleted — was unused indirection with a leaked serialization signature.
- `format_uses_ref()` in infra is the single serialization point for `@SHA # version`.
- `Lock::build_update_map()` deleted (dead code).

## Delta specs

- **resolution**: Clarify that the new `ResolvedAction` (workflow output) is distinct from the existing `ResolvedAction` (registry result), which is renamed in `rename-resolved`.
- **domain-composition**: Add `WorkflowPatch` domain type requirement, `Updater` trait deletion, `Lock::build_update_map()` deletion.

## Order

Apply after `resolved-version-comment`. Apply before `rename-resolved`.
