# Tasks: Decouple Layers

## Step 1: Introduce ConfigError

- [x] Add `ConfigError` enum to `config.rs` with variants `Manifest(ManifestError)` and `Lock(LockFileError)`
- [x] Change `Config::load()` return type from `Result<Self, AppError>` to `Result<Self, ConfigError>`
- [x] Remove `use crate::commands::app::AppError` from `config.rs`
- [x] Add `Config(ConfigError)` variant to `GxError` in `main.rs`
- [x] Update `main.rs` to handle `ConfigError` (the `?` on `Config::load()` now converts to `GxError::Config`)
- [x] Run `cargo test` — all tests pass

## Step 2: Remove dead error variants

- [x] Remove `Manifest(ManifestError)` and `Lock(LockFileError)` from `TidyError` in `commands/tidy.rs`
- [x] Remove `use crate::infrastructure::{LockFileError, ManifestError, ...}` — keep only `WorkflowError` import
- [x] Remove `Manifest(ManifestError)` and `Lock(LockFileError)` from `UpgradeError` in `commands/upgrade.rs`
- [x] Remove `use crate::infrastructure::{LockFileError, ManifestError, ...}` — keep only `WorkflowError` import
- [x] Run `cargo test` — all tests pass

## Step 3: Domain-semantic WorkflowError

- [x] Replace `WorkflowError` variants in `domain/workflow.rs` with `ScanFailed { reason }`, `ParseFailed { path, reason }`, `UpdateFailed { path, reason }`
- [x] Remove `glob`, `serde_saphyr`, `regex`, `std::io` imports from `domain/workflow.rs`
- [x] Add internal `IoWorkflowError` enum to `infrastructure/workflow.rs` with the original I/O variants (`Glob`, `Read`, `Parse`, `Write`, `Regex`)
- [x] Implement `From<IoWorkflowError> for WorkflowError` in `infrastructure/workflow.rs`
- [x] Update `FileWorkflowScanner` and `FileWorkflowUpdater` methods to use `IoWorkflowError` internally and convert at the return boundary
- [x] Remove `pub use crate::domain::{UpdateResult, WorkflowError}` re-export from `infrastructure/mod.rs`
- [x] Update `commands/tidy.rs` import: change `use crate::infrastructure::WorkflowError` to `use crate::domain::WorkflowError`
- [x] Update `commands/upgrade.rs` import: change `use crate::infrastructure::WorkflowError` to `use crate::domain::WorkflowError` (if remaining after step 2)
- [x] Update `commands/lint/mod.rs` import: change `use crate::infrastructure::WorkflowError` to `use crate::domain::WorkflowError`
- [x] Run `cargo test` — all tests pass

## Step 4: Deduplicate find_workflows

- [x] Extract `fn find_workflow_files(workflows_dir: &Path) -> Result<Vec<PathBuf>, IoWorkflowError>` as a shared function in `infrastructure/workflow.rs`
- [x] Update `FileWorkflowScanner::find_workflows` to delegate to `find_workflow_files`
- [x] Update `FileWorkflowUpdater::find_workflows` to delegate to `find_workflow_files`
- [x] Run `cargo test` — all tests pass

## Step 5: Remove manifest_path from tidy::run

- [x] Remove the `manifest_path: &Path` parameter from `tidy::run()` signature
- [x] Remove the dead `if manifest.is_empty()` branch (lines 183-186)
- [x] Update call sites in `commands/app.rs`: `tidy()` and `init()` — remove `&config.manifest_path` argument
- [x] Remove `#[allow(clippy::too_many_arguments)]` if argument count is now within Clippy's limit (max 7)
- [x] Run `cargo test` — all tests pass

## Step 6: Extract lint output formatting

- [x] Add `pub fn format_and_report(diagnostics: &[Diagnostic]) -> Result<(), LintError>` to `commands/lint/mod.rs`
- [x] Move the diagnostic formatting logic from `app.rs::lint()` (lines 167-210) into the new function
- [x] Simplify `app.rs::lint()` to call `super::lint::run()` then `super::lint::format_and_report()`
- [x] Run `cargo test` — all tests pass

## Step 7: Split domain/action.rs

- [x] Create `domain/action/` directory
- [x] Move `ActionId`, `Version`, `CommitSha`, `VersionPrecision` and their impls/tests to `domain/action/identity.rs`
- [x] Move `UsesRef`, `InterpretedRef`, `RefType` and their impls/tests to `domain/action/uses_ref.rs`
- [x] Move `ActionSpec`, `LockKey` and their impls/tests to `domain/action/spec.rs`
- [x] Move `ResolvedAction`, `VersionCorrection` and their impls/tests to `domain/action/resolved.rs`
- [x] Move `UpgradeAction`, `UpgradeCandidate`, `find_upgrade_candidate` and their tests to `domain/action/upgrade.rs`
- [x] Create `domain/action/mod.rs` with `pub use` re-exports matching the current `domain/mod.rs` surface
- [x] Update `domain/mod.rs` — change `pub mod action;` to `pub mod action;` (no change needed, it's already a module)
- [x] Verify all `pub use` paths in `domain/mod.rs` still resolve
- [x] Run `cargo test` — all tests pass

## Step 8: Break tidy::run into composable steps

- [x] Extract `sync_manifest_actions()` — removes unused, adds missing actions (currently inline ~60 lines in `run()`)
- [x] Extract `upgrade_sha_versions_to_tags()` — upgrades bare SHA manifest entries (currently inline ~40 lines in `run()`)
- [x] Extract `update_workflow_files()` — builds per-file maps and calls updater (currently inline ~30 lines in `run()`)
- [x] Extract `print_corrections()` — logs version corrections (currently inline ~6 lines in `run()`)
- [x] Verify `run()` is now a readable pipeline of named function calls
- [x] Remove `#[allow(clippy::too_many_lines)]` from `run()`
- [x] Run `cargo test` — all tests pass
- [x] Run `cargo clippy` — no warnings
