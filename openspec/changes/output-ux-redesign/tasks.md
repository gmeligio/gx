## 1. Dependencies and Module Setup

- [x] 1.1 Replace `log` and `env_logger` with `indicatif` and `console` in `Cargo.toml`
- [x] 1.2 Create `src/output/` module with `mod.rs`, `printer.rs`, `lines.rs`, `log_file.rs`, `render.rs`
- [x] 1.3 Define `OutputLine` enum in `src/output/lines.rs` with all variants (Upgraded, Added, Removed, Changed, Skipped, Warning, LintDiag, Summary, LogPath, CiNotice)

## 2. Report Structs

- [x] 2.1 Define `UpgradeReport` struct (upgrades, skipped, warnings, workflows_updated count, up_to_date flag)
- [x] 2.2 Define `TidyReport` struct (removed, added, upgraded, corrections, workflows_updated count)
- [x] 2.3 Define `LintReport` struct (diagnostics, error_count, warning_count) — wraps existing `Vec<Diagnostic>`
- [x] 2.4 Define `InitReport` struct (actions_discovered count, created flag)

## 3. Render Functions

- [x] 3.1 Implement `render_upgrade(report: &UpgradeReport) -> Vec<OutputLine>` with tests
- [x] 3.2 Implement `render_tidy(report: &TidyReport) -> Vec<OutputLine>` with tests
- [x] 3.3 Implement `render_lint(report: &LintReport) -> Vec<OutputLine>` with tests
- [x] 3.4 Implement `render_init(report: &InitReport) -> Vec<OutputLine>` with tests

## 4. Printer and LogFile

- [x] 4.1 Implement `Printer` struct with `new()` (CI detection, TTY detection, NO_COLOR), `spinner()`, `print_lines()` methods
- [x] 4.2 Implement `LogFile` struct with `new(command: &str)`, `write(msg: &str)`, `path()` methods
- [x] 4.3 Implement color application in `Printer::print_lines()` using `console` crate — map each `OutputLine` variant to its colored representation
- [x] 4.4 Add tests for `Printer::new()` CI/TTY/NO_COLOR detection logic

## 5. Refactor Commands to Return Reports

- [x] 5.1 Add `on_progress: impl Fn(&str)` parameter to `upgrade::plan()` and replace all `info!`/`warn!` calls with callback invocations or report data collection
- [x] 5.2 Refactor `upgrade::apply_upgrade_workflows()` to return `UpgradeReport` instead of printing. Remove `print_update_results()` function
- [x] 5.3 Add `on_progress: impl Fn(&str)` parameter to `tidy::plan()` and refactor output calls to callback/report
- [x] 5.4 Refactor tidy apply functions (`sync_manifest_actions`, `upgrade_sha_versions_to_tags`, `sync_overrides`, `prune_stale_overrides`, `apply_workflow_patches`) to return data for `TidyReport` instead of printing
- [x] 5.5 Refactor `lint::format_and_report()` to return `LintReport` instead of printing. Keep `LintError::ViolationsFound` return for exit code
- [x] 5.6 Remove all `info!`, `warn!`, `debug!` calls from `infrastructure/manifest.rs`, `infrastructure/lock.rs`, `infrastructure/workflow.rs` — move relevant messages to log file via progress callback or report data

## 6. Orchestrator Wiring (app.rs + main.rs)

- [x] 6.1 Refactor `app::upgrade()` to create spinner, wire progress callback (spinner + log file), collect report, return it
- [x] 6.2 Refactor `app::tidy()` similarly — spinner, callback, report
- [x] 6.3 Refactor `app::init()` similarly — spinner, callback, report
- [x] 6.4 Refactor `app::lint()` to return `LintReport` (no spinner needed — lint is local, no network calls)
- [x] 6.5 Rewrite `main.rs`: remove `init_logging()`, create `Printer` and `LogFile`, match commands, render reports, print lines, show log path. Handle CI mode (verbose inline, no spinner, no log file)

## 7. Remove Old Logging

- [x] 7.1 Remove `use log::*` from all files, remove `log` and `env_logger` from `Cargo.toml` dependencies
- [x] 7.2 Run `cargo build` to verify no compilation errors from removed logging
- [x] 7.3 Run existing test suite (`cargo test`) to verify no regressions — existing tests check file artifacts, not output

## 8. Integration Verification

- [ ] 8.1 Manual test: `gx upgrade --latest` shows minimal emoji output with spinner and log file path
- [ ] 8.2 Manual test: `CI=true gx upgrade --latest` shows verbose inline output with CI notice
- [ ] 8.3 Manual test: `gx lint` shows emoji-formatted diagnostics or clean message
- [ ] 8.4 Manual test: `gx tidy` shows minimal output with correct symbols
- [ ] 8.5 Manual test: pipe `gx upgrade 2>/dev/null` — verify no ANSI codes in stdout when piped
