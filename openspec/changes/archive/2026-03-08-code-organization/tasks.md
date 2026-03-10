## 1. Architecture Guardrails (independent — can run as subagent)

- [x] 1.1 Add layer dependency direction test to `tests/code_health.rs`: scan `src/domain/**/*.rs` for forbidden imports (`crate::tidy`, `crate::upgrade`, `crate::lint`, `crate::init`, `crate::infra`)
- [x] 1.2 Add duplicate function detection test to `tests/code_health.rs`: scan command module files (`src/tidy/`, `src/upgrade/`, `src/lint/`, `src/init/`) for non-`pub` `fn` names that appear in more than one command module
- [x] 1.3 Add file size budget test to `tests/code_health.rs`: fail if any `.rs` file in `src/` exceeds 500 lines
- [x] 1.4 Add folder file count budget test to `tests/code_health.rs`: fail if any directory in `src/` contains more than 8 `.rs` files
- [x] 1.5 Add `lint:size` mise task to `.config/mise.toml` and wire `clippy` to depend on it
- [x] 1.6 Verify all guardrail tests pass on current codebase (file size test will fail on existing large files — set initial budgets to current max + margin, with a comment noting the target)

## 2. Domain Enrichment: Diff Methods (can run as subagent)

- [x] 2.1 Add `Manifest::diff(&self, other: &Manifest) -> ManifestDiff` method to `src/domain/manifest.rs` — unify logic from tidy's and upgrade's `diff_manifests`, using upgrade's richer SHA-replacement detection
- [x] 2.2 Add unit tests for `Manifest::diff` covering: added, removed, updated actions, override added/removed
- [x] 2.3 Add `Lock::diff(&self, other: &Lock) -> LockDiff` method to `src/domain/lock.rs` — unify logic from tidy's and upgrade's `diff_locks`, treating same-key-different-SHA as replacement
- [x] 2.4 Add unit tests for `Lock::diff` covering: added, removed, SHA-replaced entries
- [x] 2.5 Replace `diff_manifests()` call in `src/tidy/mod.rs` with `manifest.diff(&planned_manifest)`, delete the private function
- [x] 2.6 Replace `diff_manifests()` and `diff_locks()` calls in `src/upgrade/mod.rs` with domain methods, delete the private functions

## 3. Domain Enrichment: Manifest Sync Methods (can run as subagent, after group 2)

- [x] 3.1 Add `Manifest::sync_overrides(&mut self, located: &[LocatedAction], action_set: &WorkflowActionSet)` to `src/domain/manifest.rs` — move logic from `tidy/mod.rs::sync_overrides`
- [x] 3.2 Add `Manifest::prune_stale_overrides(&mut self, located: &[LocatedAction])` to `src/domain/manifest.rs` — move logic from `tidy/mod.rs::prune_stale_overrides`
- [x] 3.3 Add `Manifest::lock_keys(&self) -> Vec<LockKey>` to `src/domain/manifest.rs` — move logic from `tidy/mod.rs::build_keys_to_retain`
- [x] 3.4 Add unit tests for `sync_overrides`, `prune_stale_overrides`, and `lock_keys`
- [x] 3.5 Update `src/tidy/mod.rs` to call the new domain methods, delete the private functions

## 4. Domain Enrichment: SyncEvent (can run as subagent, after group 3)

- [x] 4.1 Create `src/domain/event.rs` with `SyncEvent` enum: `ActionAdded`, `ActionRemoved`, `VersionCorrected`, `ShaUpgraded`, `ResolutionSkipped`, `RecoverableWarning`
- [x] 4.2 Implement `Display` for `SyncEvent` with human-readable messages matching current `on_progress` output
- [x] 4.3 Export `SyncEvent` from `src/domain/mod.rs`
- [x] 4.4 Refactor `sync_manifest_actions` in tidy to return `Vec<SyncEvent>` instead of taking `on_progress`. Update `plan()` to collect events and forward to `on_progress` in the orchestrator
- [x] 4.5 Refactor `upgrade_sha_versions_to_tags` in tidy to return `Vec<SyncEvent>`
- [x] 4.6 Refactor `update_lock` / `populate_lock_entry` in tidy to return `Vec<SyncEvent>` for skip/warning events
- [x] 4.7 Add unit tests verifying `SyncEvent` Display output matches expected messages

## 5. Split tidy/mod.rs (depends on groups 2-4)

- [x] 5.1 Extract `src/tidy/manifest_sync.rs`: `sync_manifest_actions`, `upgrade_sha_versions_to_tags`, `select_version`, `select_dominant_version` with their unit tests
- [x] 5.2 Extract `src/tidy/lock_sync.rs`: `update_lock`, `populate_lock_entry` (and `build_keys_to_retain` wrapper if still needed) with their unit tests
- [x] 5.3 Extract `src/tidy/patches.rs`: `compute_workflow_patches`, `build_file_update_map` with their unit tests
- [x] 5.4 Move integration tests from `tidy/mod.rs` to `src/tidy/tests.rs`
- [x] 5.5 Update `src/tidy/mod.rs`: keep `TidyPlan`, `TidyError`, `plan()`, `apply_workflow_patches()`, `Tidy` Command impl, and `mod` declarations
- [x] 5.6 Verify `mise run test` and `mise run clippy` pass

## 6. Split infra files (independent — can run as subagent)

- [x] 6.1 Extract `src/infra/lock_migration.rs` from `src/infra/lock.rs`: move `migrate_v1`, `migrate_v1_3`, `migrate_key`, `derive_comment_from_v1_key` and their tests
- [x] 6.2 Split `src/infra/workflow.rs` into `src/infra/workflow_scan.rs` (`FileWorkflowScanner`, YAML parsing, `WorkflowScanner` trait impl) and `src/infra/workflow_update.rs` (`FileWorkflowUpdater`, `WorkflowUpdater` trait impl)
- [x] 6.3 Update `src/infra/mod.rs` re-exports for new file layout
- [x] 6.4 Verify `mise run test` and `mise run clippy` pass

## 7. Final Verification

- [x] 7.1 Verify all `code_health` guardrail tests pass (including file size budgets with new split files)
- [x] 7.2 Lower file size budget to target (500 lines) now that files have been split
- [x] 7.3 Run full test suite: `mise run test && mise run integ && mise run clippy`
