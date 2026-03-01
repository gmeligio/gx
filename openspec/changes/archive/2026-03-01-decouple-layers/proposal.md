# Proposal: Decouple Layers

## Problem

The codebase has several coupling violations that make it harder to read, test, and evolve:

1. **Circular dependency**: `config.rs` imports `AppError` from `commands/app.rs`, while commands depend on config — creating a cycle between layers.
2. **Command handlers depend on infrastructure**: `tidy.rs`, `upgrade.rs`, and `lint/mod.rs` import error types from `crate::infrastructure`, bypassing the domain layer boundary.
3. **I/O errors live in domain**: `WorkflowError` in `domain/workflow.rs` contains filesystem-specific variants (`Glob`, `Read`, `Write`, `Parse`, `Regex`) that belong in infrastructure.
4. **Dead error variants**: `TidyError` and `UpgradeError` wrap `ManifestError` and `LockFileError` but never produce them — leftovers from when commands did their own saving.
5. **Code duplication**: `find_workflows()` is implemented identically in both `FileWorkflowScanner` and `FileWorkflowUpdater`.
6. **Infrastructure leak**: `tidy::run()` takes a `manifest_path: &Path` parameter only to display it in a log message inside a dead code branch.
7. **Presentation in dispatcher**: `app.rs::lint()` contains diagnostic formatting logic (level strings, location prefixes, count summaries) that belongs in the lint module.
8. **Mega file**: `domain/action.rs` is 550+ lines with 13+ types serving distinct responsibilities.
9. **Monolithic function**: `tidy::run()` is 200+ lines with `#[allow(clippy::too_many_lines)]`, mixing scanning, syncing, resolving, and updating.

## Solution

A sequence of 8 targeted refactorings that restore clean layer boundaries:

1. Introduce `ConfigError` to break the config→commands cycle
2. Remove dead error variants from `TidyError` and `UpgradeError`
3. Make `WorkflowError` domain-semantic (Option 3: `ScanFailed`/`ParseFailed`/`UpdateFailed`); infrastructure maps I/O errors into these
4. Extract shared `find_workflow_files()` helper
5. Remove `manifest_path` parameter and dead code branch from `tidy::run()`
6. Move lint diagnostic formatting from `app.rs` into the lint module
7. Split `domain/action.rs` into focused submodules (`identity`, `uses_ref`, `spec`, `resolved`, `upgrade`)
8. Extract named phases from `tidy::run()` into composable, testable functions

## Non-goals

- Changing any user-facing behavior or CLI interface
- Adding new features or capabilities
- Restructuring the overall module hierarchy (commands/domain/infrastructure)
- Modifying test coverage (tests move with their code, no tests removed)

## Outcome

After this change:
- `commands/` only imports from `domain/` (never from `infrastructure/`)
- `config.rs` has no dependency on `commands/`
- Every error variant in command errors is reachable
- `tidy::run()` is a readable pipeline of named phases
- `domain/action.rs` is split into files of ~60-200 lines each
