## 1. Extract `infra/github/registry.rs`

- [x] 1.1 Create `src/infra/github/registry.rs`. Move from `mod.rs`: `USER_AGENT` and `REQUEST_TIMEOUT_SECS` constants, `Error` enum, `Registry` struct, `impl Registry` (constructor, `authenticated_get`, `check_status`), and `impl VersionRegistry for Registry`.
- [x] 1.2 Update `src/infra/github/mod.rs` to declare `mod registry;` and reexport: `pub use registry::{Error, Registry};`. Keep existing `mod resolve;` and `mod responses;` declarations. Remove all logic — `mod.rs` becomes reexports only.
- [x] 1.3 Inline `src/infra/github/tests.rs` (98 lines) into the bottom of `src/infra/github/resolve.rs` as a `#[cfg(test)] mod tests` block. Delete the standalone `tests.rs` file.
- [x] 1.4 Verify: `cargo check` and `cargo test -p gx --lib` pass.

## 2. Extract `infra/manifest/parse.rs`

- [x] 2.1 Create `src/infra/manifest/parse.rs`. Move from `mod.rs`: `MANIFEST_FILE_NAME` constant, `Error` enum, `Store` struct and its `impl` blocks (`new`, `save`), `parse()`, `parse_lint_config()`, `create()` functions.
- [x] 2.2 Move the `#[cfg(test)]` block (with `#[path = "tests.rs"]`) from `mod.rs` to the bottom of `parse.rs`.
- [x] 2.3 Update `src/infra/manifest/mod.rs` to declare `mod parse;` and reexport: `pub use parse::{Error, Store, parse, parse_lint_config, create, MANIFEST_FILE_NAME};`. Keep existing `mod convert;` and `pub mod patch;`. Remove all logic.
- [x] 2.4 Verify: `cargo check` and `cargo test -p gx --lib` pass.

## 3. Extract `infra/lock/store.rs`

- [x] 3.1 Create `src/infra/lock/store.rs`. Move from `mod.rs`: `LOCK_FILE_NAME` constant, `Error` enum, `Store` struct and its `impl` blocks (`new`, `load`, `save`), `parse_toml()` helper.
- [x] 3.2 Move the `#[cfg(test)]` block (with `#[path = "tests.rs"]`) from `mod.rs` to the bottom of `store.rs`.
- [x] 3.3 Update `src/infra/lock/mod.rs` to declare `mod store;` and reexport: `pub use store::{Error, Store, LOCK_FILE_NAME};`. Keep existing `mod format;` and `mod migration;`. Remove all logic.
- [x] 3.4 Verify: `cargo check` and `cargo test -p gx --lib` pass.

## 4. Extract `infra/workflow_scan/scanner.rs`

- [x] 4.1 Create `src/infra/workflow_scan/scanner.rs`. Move from `mod.rs`: `IoWorkflowError` enum, `impl From<IoWorkflowError> for WorkflowError`, `ExtractedAction` struct, YAML structs (`Workflow`, `Job`, `Step`), `find_workflow_files()` function, `FileScanner` struct and all its `impl` blocks (including the `Scanner` trait impl).
- [x] 4.2 Move the `#[cfg(test)]` block (with `#[path = "tests.rs"]`) from `mod.rs` to the bottom of `scanner.rs`.
- [x] 4.3 Update `src/infra/workflow_scan/mod.rs` to declare `mod scanner;` and reexport: `pub use scanner::FileScanner;`. Remove all logic.
- [x] 4.4 Verify: `cargo check` and `cargo test -p gx --lib` pass.

## 5. Final verification

- [x] 5.1 Run full test suite: `cargo test` and `cargo clippy`
- [x] 5.2 Verify all four `mod.rs` files contain only `mod` declarations and `pub use` reexports — no struct defs, no impl blocks, no functions.
