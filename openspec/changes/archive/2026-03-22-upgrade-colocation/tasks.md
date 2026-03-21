## 1. Move `Plan` and `Error` to `plan.rs`

- [x] 1.1 Move `Plan` struct and its `is_empty()` impl from `types.rs` to `plan.rs`. Update imports in `plan.rs` to use local `Plan` instead of `super::types::Plan`.
- [x] 1.2 Move `Error` enum from `types.rs` to `plan.rs`. Update imports: `plan.rs` uses `Error` locally; `mod.rs` imports `Error` from `plan`.
- [x] 1.3 Verify: `mise run build` passes.

## 2. Extract `upgrade/command.rs`

- [x] 2.1 Create `src/upgrade/command.rs`. Move from `mod.rs`: all `use` statements, `RunError` enum, `Upgrade` struct, and `impl Command for Upgrade`. Update `mod.rs` import of `Error` to come from `plan` (since `types` alias moved in step 1).
- [x] 2.2 Update `src/upgrade/mod.rs` to declare modules and reexport only: `pub mod cli; mod command; mod plan; pub mod report; pub use command::Upgrade;`.
- [x] 2.3 Verify: `mise run build` passes.

## 3. Move `Request`, `Scope`, `Mode` and tests to `cli.rs`

- [x] 3.1 Move `Scope`, `Mode`, `Request` (including `Request::new()`) from `types.rs` to `cli.rs`. Update imports in `cli.rs` to use local types.
- [x] 3.2 Move `#[cfg(test)] mod tests` block from `types.rs` to `cli.rs`. Update `use super::` paths.
- [x] 3.3 Update `command.rs` to import `Request` from `cli` instead of `types`.
- [x] 3.4 Delete `types.rs`. Remove `pub mod types;` from `mod.rs`.
- [x] 3.5 Verify: `mise run build` and `mise run test` pass.

## 4. Final verification

- [x] 4.1 Run `mise run clippy` — no new warnings.
- [x] 4.2 Grep for `crate::upgrade::plan` and `crate::upgrade::types` outside `src/upgrade/` to confirm no external callers exist.
- [x] 4.3 Confirm `upgrade/mod.rs` contains only `mod` declarations and `pub use` reexports.
- [x] 4.4 Confirm `types.rs` no longer exists.
