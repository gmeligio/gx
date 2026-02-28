## Why

The `gx lint` command (from `lint-command` change) is functional but has three rough edges that prevent production use:

1. **Error messaging hack**: Lint failures are reported as `AppError::Manifest(ManifestError::Validation(...))` — semantically wrong and produces ugly output like `Error: App(Manifest(Validation("lint check failed with errors")))`.
2. **Stale-comment rule is a stub**: Returns `Vec::new()` despite the data being available in `LocatedAction` (the scanner already extracts SHA + version comment).
3. **Integration tests are shallow**: Only 5 tests vs tidy's 17. No sha-mismatch coverage, no mixed-level tests, no edge cases.

## What Changes

- New `LintError` error type with a `ViolationsFound` variant for policy failures (distinct from I/O errors)
- `stale-comment` rule fully implemented using `LocatedAction.sha` + lock reverse lookup
- Comprehensive integration tests matching tidy test depth

## Capabilities

### New Capabilities

- `lint-error-type`: Proper error handling for lint command with clean CLI output
- `lint-stale-comment`: Working stale-comment rule detecting mismatched version comments

### Modified Capabilities

- `lint-command`: Clean error output, proper exit codes without error wrapper noise

## Impact

- `crates/gx-lib/src/commands/lint/mod.rs`: Add `LintError` enum, update `run()` return type
- `crates/gx-lib/src/commands/lint/stale_comment.rs`: Full implementation using lock reverse lookup
- `crates/gx-lib/src/commands/app.rs`: Use `LintError` instead of `ManifestError::Validation` hack, clean output formatting
- `crates/gx/src/main.rs`: Handle `LintError::ViolationsFound` for exit code 1 without error message noise
- `crates/gx-lib/tests/lint_test.rs`: Expand from 5 to ~15 tests
- No changes to tidy, upgrade, init, or domain layer
