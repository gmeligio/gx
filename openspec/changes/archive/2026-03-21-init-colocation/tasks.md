## 1. Extract `init/command.rs`

- [x] 1.1 Create `src/init/command.rs`. Move from `mod.rs`: all `use` statements (lines 3–14), `Error` enum, `Init` struct, and `impl Command for Init`.
- [x] 1.2 Update `src/init/mod.rs` to declare `mod command;` and `pub mod report;`, then reexport: `pub use command::{Error, Init};`. Remove all logic and imports.
- [x] 1.3 Verify: `mise run build` and `mise run test` pass.

## 2. Final verification

- [x] 2.1 Run `mise run clippy` — no new warnings.
- [x] 2.2 Confirm `init/mod.rs` contains only `mod` declarations and `pub use` reexports — no struct defs, no impl blocks, no functions.
