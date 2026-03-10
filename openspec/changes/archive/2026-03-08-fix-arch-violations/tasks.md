## 1. Move Command/CommandReport traits out of domain

- [x] 1.1 Create `src/command.rs` with `Command` (with `type Error: std::error::Error`) and `CommandReport` traits
- [x] 1.2 Add `pub mod command;` to `src/lib.rs`
- [x] 1.3 Remove `domain/command.rs`, update `domain/mod.rs` (drop `pub mod command;` and re-exports)
- [x] 1.4 Update all imports: `crate::domain::{Command, CommandReport}` → `crate::command::{Command, CommandReport}`

## 2. Create per-command error types and delete AppError

- [x] 2.1 Create `InitError` in `src/init/mod.rs` (AlreadyInitialized + Github, Manifest, Lock, Workflow, Tidy)
- [x] 2.2 Create `TidyRunError` in `src/tidy/mod.rs` (Github, Manifest, Lock, Workflow, Tidy)
- [x] 2.3 Create `UpgradeRunError` in `src/upgrade/mod.rs` (Github, Manifest, Lock, Workflow, Upgrade)
- [x] 2.4 Update each `impl Command` to use its own error type instead of `AppError`
- [x] 2.5 Update `GxError` in `main.rs`: replace `#[from] AppError` with `#[from] InitError`, `#[from] TidyRunError`, `#[from] UpgradeRunError`, `#[from] LintError`
- [x] 2.6 Delete `src/domain/error.rs`, remove from `domain/mod.rs`
- [x] 2.7 Verify `domain_does_not_import_upward` test passes

## 3. Make duplicate-fn test trait-aware

- [x] 3.1 Update the function-scanning loop in `no_duplicate_private_fns_across_command_modules` to track `impl Trait for Type` blocks via brace-depth counting
- [x] 3.2 Skip `fn` lines inside trait impl blocks
- [x] 3.3 Verify `render` and `run` are no longer flagged
- [x] 3.4 Verify the test still catches genuine non-trait duplicates

## 4. Create shared test mocks in domain/resolution.rs

- [x] 4.1 Add `#[cfg(test)] pub(crate) mod testutil` to `src/domain/resolution.rs` with `FakeRegistry` and `AuthRequiredRegistry`
- [x] 4.2 `FakeRegistry` supports: `with_all_tags`, `with_sha_tags`, `with_fixed_sha`, `fail_tags`
- [x] 4.3 Replace `NoopRegistry` in `tidy/manifest_sync.rs` → `AuthRequiredRegistry`
- [x] 4.4 Replace `TagUpgradeRegistry` in `tidy/manifest_sync.rs` → `FakeRegistry`
- [x] 4.5 Replace `MockPlanRegistry` in `upgrade/plan.rs` → `FakeRegistry`
- [x] 4.6 Replace `MetadataOnlyRegistry` and `SimpleShaRegistry` in `tidy/lock_sync.rs` → `FakeRegistry` variants
- [x] 4.7 Replace `TaggedShaRegistry` in `tidy/lock_sync.rs` → `FakeRegistry`
- [x] 4.8 Keep `MixedRegistry` (lock_sync.rs) and `MockRegistry` (resolution.rs) inline
- [x] 4.9 Verify `no_duplicate_private_fns_across_command_modules` passes

## 5. Final verification

- [x] 5.1 Run `cargo test` — all green
- [x] 5.2 Run `cargo clippy` — all green
- [x] 5.3 All 5 code_health tests pass
- [x] 5.4 No file exceeds 500 lines
