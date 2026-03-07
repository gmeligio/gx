## Phase 1: Define traits

- [x] 1.1 Create `src/command/traits.rs` with `CommandReport` trait (`render() -> Vec<OutputLine>`, `exit_code() -> i32` with default `0`) and `Command` trait (`type Report: CommandReport`, `fn run(&self, &Path, Config, &mut dyn FnMut(&str)) -> Result<Self::Report, AppError>`)
- [x] 1.2 Re-export `Command` and `CommandReport` from `src/command/mod.rs`

## Phase 2: Implement `CommandReport` on existing report types

- [x] 2.1 `impl CommandReport for TidyReport` — move body of `render_tidy()` into `render()` method
- [x] 2.2 `impl CommandReport for InitReport` — move body of `render_init()` into `render()` method
- [x] 2.3 `impl CommandReport for UpgradeReport` — move body of `render_upgrade()` into `render()` method
- [x] 2.4 `impl CommandReport for LintReport` — move body of `render_lint()` into `render()` method, implement `exit_code()` returning `1` when `error_count > 0`
- [x] 2.5 Remove the free `render_*` functions from `output/render.rs`, delete the file, update `output/mod.rs` re-exports
- [x] 2.6 Move render tests from `render.rs` to `report.rs` (adjust imports)

## Phase 3: Implement `Command` on each command module

- [x] 3.1 `command::tidy` — add `pub struct Tidy;`, implement `Command for Tidy` delegating to existing `run()` logic, then remove the standalone `run()` function
- [x] 3.2 `command::init` — add `pub struct Init;`, implement `Command for Init` delegating to existing `run()` logic, then remove the standalone `run()` function
- [x] 3.3 `command::upgrade` — add `pub struct Upgrade { pub request: UpgradeRequest }`, implement `Command for Upgrade` delegating to existing `run()` logic, then remove the standalone `run()` function
- [x] 3.4 `command::lint` — add `pub struct Lint;`, implement `Command for Lint`. Rename inner `run()` → `collect_diagnostics()`. Add `on_progress` parameter to `collect_diagnostics()`. Remove `run_command()`. Remove `LintError::ViolationsFound` variant. Simplify `format_and_report()` to always return `Ok(LintReport)`

## Phase 4: Update main.rs

- [x] 4.1 Update all match arms to use the trait: `XCmd.run(&repo_root, config, &mut cb)?` → `report.render()` → check `report.exit_code()`
- [x] 4.2 Remove `use gx::command::lint::LintError` import (no longer needed for matching)
- [x] 4.3 Remove the lint special-case `process::exit(1)` — replaced by generic `exit_code()` check

## Phase 5: Update tests and verify

- [x] 5.1 Update `tests/e2e_test.rs` — rename `lint::run()` calls to `lint::collect_diagnostics()`
- [x] 5.2 Run `cargo test` — all tests pass
- [x] 5.3 Run `cargo clippy` — no warnings
