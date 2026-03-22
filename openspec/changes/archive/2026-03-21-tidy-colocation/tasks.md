# Tasks: Tidy Module Colocation

## Step 1: Create `tidy/command.rs` with logic from `mod.rs`

- [x] Create `src/tidy/command.rs`
- [x] Move `Plan` struct and its `is_empty()` impl from `mod.rs`
- [x] Move `Error` enum from `mod.rs`
- [x] Move `plan()` function from `mod.rs`
- [x] Move `apply_workflow_patches()` function from `mod.rs`
- [x] Move `RunError` enum from `mod.rs`
- [x] Move `Tidy` struct and `impl Command for Tidy` from `mod.rs`
- [x] Move all associated `use` imports
- [x] Add `pub` visibility to items that need to be re-exported
- [x] Run `mise run build` — compiles

## Step 2: Relocate `tidy/tests.rs` to `tidy/command_tests.rs`

- [x] Rename `tests.rs` to `command_tests.rs`
- [x] Add `#[cfg(test)] #[path = "command_tests.rs"] mod tests;` at the bottom of `command.rs`
- [x] Remove `mod tests;` declaration from `mod.rs`
- [x] No import changes needed — `use super::*` still resolves correctly since the test module remains a child of the module containing `plan()` and `apply_workflow_patches()`
- [x] Run `mise run test` — all tests pass

## Step 3: Reduce `tidy/mod.rs` to reexports

- [x] Replace `mod.rs` contents with module declarations and reexports only:
  ```rust
  mod command;
  mod lock_sync;
  mod manifest_sync;
  mod patches;
  pub mod report;

  pub use command::{Error, Plan, Tidy};
  ```
- [x] Verify no logic or type definitions remain in `mod.rs`
- [x] Run `mise run test` — all tests pass
- [x] Run `mise run clippy` — no warnings
