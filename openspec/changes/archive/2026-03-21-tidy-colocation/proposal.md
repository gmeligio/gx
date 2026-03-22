# Tidy Module Colocation

## Summary

Extract command logic from `tidy/mod.rs` into `tidy/command.rs` and inline `tidy/tests.rs` into the files that define the functions they test.

## Motivation

`tidy/mod.rs` has ~265 lines of command orchestration logic (the `Tidy` struct, `Command` impl, `plan()`, `apply_workflow_patches()`). Under the "mod.rs = reexports only" rule, this logic needs a semantic home.

`tidy/tests.rs` (429 lines) is an integration test file exercising `plan()` and `apply_workflow_patches()`. Once those functions move to `command.rs`, the tests should follow — colocating tests with the code they exercise.

## Spec gate

Skipped — internal restructuring with no user-visible change.

## Changes

### 1. Create `tidy/command.rs`

Move from `mod.rs`:
- `Error` enum
- `Tidy` struct
- `impl Command for Tidy`
- `plan()` function
- `apply_workflow_patches()` function

### 2. Inline `tidy/tests.rs` into `tidy/command.rs`

The tests exercise `plan()` and `apply_workflow_patches()`, which now live in `command.rs`. Add them as a `#[cfg(test)] mod tests` block at the bottom of `command.rs`.

This increases `command.rs` to ~694 total lines (265 logic + 429 tests), but **only 265 logic lines** — well within the 300 target. The total exceeds the 550-line budget, so the tests stay in a separate file owned by `command.rs` via `#[cfg(test)] #[path = "command_tests.rs"] mod tests;`.

### 3. Update `tidy/mod.rs`

Reduce to reexports only:

```rust
mod command;
mod lock_sync;
mod manifest_sync;
mod patches;
pub mod report;

pub use command::{Error, Plan, Tidy};
```

## Risks

- `tidy/tests.rs` is the largest test file (429 lines). Combined with ~265 logic lines, `command.rs` would be ~694 lines — exceeding the 550-line budget. The tests therefore stay in a separate file (`command_tests.rs`) owned by `command.rs` via `#[cfg(test)] #[path = "command_tests.rs"] mod tests;`. This keeps file sizes within budget while colocating test ownership with the code under test.
