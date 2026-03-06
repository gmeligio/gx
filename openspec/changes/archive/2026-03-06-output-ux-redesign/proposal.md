## Why

All user-facing output currently goes through `log`/`env_logger` with `[INFO]` prefixes, making output noisy and impossible to test. Commands print directly via `info!()` scattered throughout logic, so there's no way to assert on what the user sees. The output also duplicates information (upgrades listed twice), shows unnecessary status messages ("Checking for upgrades..." when the user typed `upgrade`), and lacks progress feedback during slow GitHub API calls (~10s).

## What Changes

- **BREAKING**: Remove `log` and `env_logger` dependencies entirely. All user-facing output moves to a structured rendering system.
- Add `indicatif` for spinner progress during network operations.
- Add `console` for colors and terminal detection (TTY, `NO_COLOR`).
- Commands return report structs instead of printing — separating logic from presentation.
- Introduce a `Printer` abstraction with `TermPrinter` (production) and test-friendly rendering.
- Structured `OutputLine` enum for rendering, with colors applied at the final print boundary.
- Spinner progress via `Fn(&str)` callbacks so command logic stays pure and testable.
- Always write a detailed log file to `{tmp}/gx/{command}/{RFC-date}.log` for local debugging.
- Auto-detect CI environments (`$CI` env var) and run verbose inline output automatically.
- Minimal emoji-based default output: `↑` upgrades, `✓` success, `✗` errors, `⚠` warnings.

## Capabilities

### New Capabilities

- `output-rendering`: Structured output system — report structs, OutputLine enum, Printer trait, color application at boundary, log file writing, CI detection.
- `spinner-progress`: Ephemeral spinner during long operations (GitHub API calls) using indicatif, with callback pattern for testability.

### Modified Capabilities

_(No existing spec-level requirements change. This is a presentation layer change — the same data is produced, just rendered differently.)_

## Impact

- **Dependencies**: Remove `log` (0.4), `env_logger` (0.11). Add `indicatif`, `console`.
- **All command files**: `upgrade.rs`, `tidy.rs`, `lint/mod.rs`, `app.rs` — remove all `info!()`, `warn!()`, `debug!()` calls, replace with report struct returns.
- **`main.rs`**: Replace `env_logger` init with Printer creation, spinner lifecycle, log file setup.
- **`infrastructure/manifest.rs`, `infrastructure/lock.rs`**: Remove `info!()` calls for file updates (move to report data or log file).
- **Tests**: New unit tests for rendering functions. Existing integration tests unchanged (they test file artifacts, not output).
