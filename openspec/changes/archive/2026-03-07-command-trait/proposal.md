## Why

The four commands (tidy, init, upgrade, lint) have drifted into inconsistent shapes: lint uses `run_command()` while others use `run()`, lint borrows `&Config` while others consume `Config`, lint lacks `on_progress`, and lint's exit-code logic is special-cased in `main.rs`. There's no compile-time enforcement that commands follow the same contract, so every new command or refactor risks further drift.

## What Changes

- **Introduce a `Command` trait** that all commands implement, enforcing a uniform `run()` signature with `&self`, `&Path`, `Config`, and `&mut dyn FnMut(&str)`.
- **Introduce a `CommandReport` trait** with `render() -> Vec<OutputLine>` and `exit_code() -> i32` (default `0`), replacing the free `render_*` functions and eliminating lint's special `process::exit(1)` in main.
- **Each command becomes a struct** — unit structs for Tidy/Init/Lint, `Upgrade { request: UpgradeRequest }` carries its command-specific args via `&self`.
- **Align lint with the other commands** — takes owned `Config`, accepts `on_progress`, renames inner `run()` to a domain-specific name (e.g. `collect_diagnostics`), the trait `run()` becomes the entry point.
- **Remove `LintError::ViolationsFound`** — violation detection moves to `LintReport::exit_code()`.
- **Simplify `main.rs`** — each match arm calls `cmd.run(...)`, then `report.render()`, then checks `report.exit_code()`. Uniform pattern, no special cases.

## Capabilities

_(No user-facing behavior changes. Same commands, same output, same exit codes.)_

## Impact

- **New file `src/command/traits.rs`** (or in `mod.rs`): `Command` and `CommandReport` trait definitions.
- **`src/command/tidy.rs`**: Add `struct Tidy;` + `impl Command for Tidy`. Existing `run()` becomes the trait impl.
- **`src/command/init.rs`**: Add `struct Init;` + `impl Command for Init`. Existing `run()` becomes the trait impl.
- **`src/command/upgrade.rs`**: Add `struct Upgrade { pub request: UpgradeRequest }` + `impl Command for Upgrade`. Existing `run()` becomes the trait impl.
- **`src/command/lint/mod.rs`**: Add `struct Lint;` + `impl Command for Lint`. Rename current `run()` → `collect_diagnostics()`. Remove `run_command()`. Remove `LintError::ViolationsFound`. Add `on_progress` support.
- **`src/output/render.rs`**: Move `render_*` functions into `CommandReport` impls on each report type. This file may shrink significantly or be removed.
- **`src/output/report.rs`**: Each report type gains `impl CommandReport`.
- **`src/command/app.rs`**: Remove `Lint(LintError)` variant's `ViolationsFound` arm from error display if present.
- **`src/main.rs`**: Uniform dispatch pattern per arm — no more lint special case.
- **Tests**: Existing integration tests unchanged (they call the binary). Unit tests for lint that match on `ViolationsFound` will need updating.
