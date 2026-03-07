## 1. Create `commands/common.rs` with shared helpers

- [x] 1.1 Create `src/commands/common.rs` with `make_cb`, `finish_spinner`, `append_log_path` moved from `main.rs`
- [x] 1.2 Add `pub mod common;` to `src/commands/mod.rs`
- [x] 1.3 Update `main.rs` to call `commands::common::make_cb`, etc. instead of local functions
- [x] 1.4 `cargo test` passes

## 2. Extract `init` into its own command module

- [x] 2.1 Create `src/commands/init.rs` with `pub fn run()` — move `app::init()` logic here
- [x] 2.2 Add `pub mod init;` to `src/commands/mod.rs`
- [x] 2.3 Remove `init()` function from `app.rs`
- [x] 2.4 Update `main.rs` `Commands::Init` arm to call `commands::init::run()`
- [x] 2.5 `cargo test` passes

## 3. Move tidy orchestration into `commands/tidy.rs`

- [x] 3.1 Add `pub fn run()` to `commands/tidy.rs` — move `app::tidy()` logic here
- [x] 3.2 Remove `tidy()` function from `app.rs`
- [x] 3.3 Update `main.rs` `Commands::Tidy` arm to call `commands::tidy::run()`
- [x] 3.4 `cargo test` passes

## 4. Move upgrade orchestration into `commands/upgrade.rs`

- [x] 4.1 Add `pub fn run()` to `commands/upgrade.rs` — move `app::upgrade()` logic here
- [x] 4.2 Move `resolve_upgrade_mode()` from `main.rs` into `commands/upgrade.rs`
- [x] 4.3 Remove `upgrade()` function from `app.rs`
- [x] 4.4 Update `main.rs` `Commands::Upgrade` arm to call `commands::upgrade::run()` and `upgrade::resolve_upgrade_mode()`
- [x] 4.5 `cargo test` passes

## 5. Move lint orchestration into `commands/lint/mod.rs`

- [x] 5.1 Add `pub fn run_command()` to `commands/lint/mod.rs` — move `app::lint()` logic here
- [x] 5.2 Remove `lint()` function from `app.rs`
- [x] 5.3 Update `main.rs` `Commands::Lint` arm to call `commands::lint::run_command()`
- [x] 5.4 `cargo test` passes

## 6. Clean up `app.rs` and `main.rs`

- [x] 6.1 Remove unused imports from `app.rs` (should only have error-related imports left)
- [x] 6.2 Remove unused imports from `main.rs`
- [x] 6.3 Verify `app.rs` contains only `AppError` enum and its `#[cfg(test)]` block
- [x] 6.4 `cargo clippy` passes with no warnings
- [x] 6.5 `cargo test` passes — full suite, no regressions
