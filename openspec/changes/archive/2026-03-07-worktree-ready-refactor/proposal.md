## Why

The codebase has bottleneck files — `app.rs` (280 lines, 4 command orchestrations), `main.rs` (258 lines, CLI definitions + dispatch + helpers) — that nearly every feature change must touch. When running parallel Claude Code agents in git worktrees (one agent per feature), both agents modify these same files, creating merge conflicts that require manual resolution.

The goal is to decompose these bottleneck files so that parallel agents working on different commands (tidy, upgrade, lint, init) or adding new commands touch **non-overlapping files**.

## What Changes

- **Extract `init` into its own command module** (`commands/init.rs`) — currently lives only in `app.rs`.
- **Move orchestration from `app.rs` into each command module** — `tidy::run()`, `upgrade::run()`, `lint::run_command()`, `init::run()`. Each command owns its full vertical slice from orchestration to plan.
- **Slim `app.rs` down to shared types only** — `AppError` enum and any shared helpers. No command-specific logic.
- **Move CLI helpers from `main.rs` into a shared module** — `make_cb`, `finish_spinner`, `append_log_path` move to `commands/mod.rs` or a `commands/common.rs`.
- **Move `resolve_upgrade_mode` from `main.rs` into `commands/upgrade.rs`** — it's upgrade-specific argument parsing.
- **Keep `main.rs` as a thin dispatcher** — parse CLI, setup printer/log, match command, call `<command>::run()`.

## Capabilities

### Modified Capabilities

_(No user-facing behavior changes. This is a pure internal refactor — same commands, same output, same exit codes.)_

## Impact

- **`src/commands/app.rs`**: Shrinks from ~280 lines to ~60 lines (just `AppError` + shared types).
- **`src/commands/tidy.rs`**: Gains `run()` function (moved from `app::tidy()`).
- **`src/commands/upgrade.rs`**: Gains `run()` function (moved from `app::upgrade()`) and `resolve_upgrade_mode()` (moved from `main.rs`).
- **`src/commands/lint/mod.rs`**: Gains `run_command()` function (moved from `app::lint()`).
- **`src/commands/init.rs`**: New file with `run()` function (extracted from `app::init()`).
- **`src/commands/mod.rs`**: Adds `pub mod init;` and optionally shared helpers.
- **`src/main.rs`**: Shrinks from ~258 lines to ~80 lines (parse + dispatch only).
- **Tests**: No changes — integration tests call the binary, not `app::*` functions.
- **Dependencies**: No changes.
