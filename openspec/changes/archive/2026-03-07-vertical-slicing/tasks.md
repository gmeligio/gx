# Tasks: Vertical Slicing

## Step 1: Rename infrastructure/ → infra/

- [x] `git mv src/infrastructure src/infra`
- [x] Update `src/lib.rs`: `pub mod infrastructure` → `pub mod infra`
- [x] Find-and-replace all `crate::infrastructure::` → `crate::infra::` across the codebase
- [x] Find-and-replace all `use crate::infrastructure` → `use crate::infra` across the codebase
- [x] Update `config.rs` imports
- [x] Run `cargo check` — compiles clean

## Step 2: Move Command + CommandReport traits to domain/

- [x] `git mv src/command/traits.rs src/domain/command.rs`
- [x] Add `pub mod command;` to `domain/mod.rs`
- [x] Update re-exports in `domain/mod.rs`: `pub use command::{Command, CommandReport}`
- [x] Update all `use crate::command::traits::` and `use crate::command::{Command, CommandReport}` → `use crate::domain::{Command, CommandReport}`
- [x] Run `cargo check` — compiles clean

## Step 3: Move AppError to domain/

- [x] `git mv src/command/app.rs src/domain/error.rs`
- [x] Add `pub mod error;` to `domain/mod.rs`
- [x] Update re-exports: `pub use error::AppError`
- [x] Update all `use crate::command::app::AppError` → `use crate::domain::AppError` (or `crate::domain::error::AppError`)
- [x] Run `cargo check` — compiles clean

## Step 4: Split plan.rs

- [x] Move `TidyPlan`, `WorkflowPatch`, `LockEntryPatch` into `src/command/tidy.rs` (will move to tidy/ later)
- [x] Move `UpgradePlan` into `src/command/upgrade.rs` (will move to upgrade/ later)
- [x] Keep `ManifestDiff` and `LockDiff` in `domain/plan.rs` (shared diff types)
- [x] Update all imports
- [x] Run `cargo check` — compiles clean
- [x] Run `cargo test` — all tests pass

## Step 5: Split output/report.rs into feature reports

- [x] Create `src/command/tidy_report.rs` — move `TidyReport` + its `CommandReport` impl
- [x] Create `src/command/init_report.rs` — move `InitReport` + its `CommandReport` impl
- [x] Create `src/command/upgrade_report.rs` — move `UpgradeReport` + its `CommandReport` impl
- [x] Create `src/command/lint_report.rs` — move `LintReport` + its `CommandReport` impl
- [x] Delete `output/report.rs`
- [x] Update `output/mod.rs` — remove report re-exports
- [x] Update all imports in command files
- [x] Run `cargo check` — compiles clean
- [x] Run `cargo test` — all tests pass

## Step 6: Promote features to top-level modules

- [x] `git mv src/command/tidy.rs src/tidy/mod.rs` (create dir first)
- [x] Move `src/command/tidy_report.rs` → `src/tidy/report.rs`
- [x] `git mv src/command/init.rs src/init/mod.rs` (create dir first)
- [x] Move `src/command/init_report.rs` → `src/init/report.rs`
- [x] `git mv src/command/upgrade.rs src/upgrade/mod.rs` (create dir first)
- [x] Move `src/command/upgrade_report.rs` → `src/upgrade/report.rs`
- [x] `git mv src/command/lint src/lint` (already a dir)
- [x] Move `src/command/lint_report.rs` → `src/lint/report.rs`
- [x] Update `src/lib.rs`: remove `pub mod command`, add `pub mod tidy`, `pub mod init`, `pub mod upgrade`, `pub mod lint`
- [x] Update all imports
- [x] Run `cargo check` — compiles clean

## Step 7: Inline command/common.rs into main.rs

- [x] Move `make_cb`, `finish_spinner`, `append_log_path` from `command/common.rs` into `main.rs`
- [x] Delete `src/command/common.rs`
- [x] Delete `src/command/mod.rs` (command/ dir is now empty)
- [x] Remove `src/command/` directory
- [x] Update `main.rs` imports
- [x] Run `cargo check` — compiles clean

## Step 8: Update main.rs imports and dispatch

- [x] Update all `use gx::command::*` → `use gx::{tidy, init, upgrade, lint}`
- [x] Update command dispatch to use `tidy::Tidy`, `init::Init`, etc.
- [x] Verify `CommandReport` and `Command` are imported from `gx::domain`
- [x] Run `cargo check` — compiles clean
- [x] Run `cargo test` — all tests pass

## Step 9: Final verification

- [x] Run `mise run lint` — no warnings
- [x] Run `mise run test` — all tests pass
- [x] Verify no remaining references to `crate::infrastructure` or `crate::command`
- [x] Verify the `command/` and `infrastructure/` directories no longer exist
