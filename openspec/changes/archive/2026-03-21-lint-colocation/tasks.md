## 1. Extract `lint/command.rs`

- [x] 1.1 Create `src/lint/command.rs`. Move from `mod.rs`: all `use` statements (lines 11–28), `RuleName` enum + `Display`/`FromStr` impls, `Error` enum, `Diagnostic` struct + impls, `Context` struct, `Rule` trait, `matches_ignore`, `matches_ignore_action`, `is_ignored`, `format_and_report`, `collect_diagnostics`, `Lint` struct + `impl Command for Lint`, and `#[cfg(test)] mod tests`.
- [x] 1.2 In `command.rs`, update rule type imports to use `super::` paths: `super::sha_mismatch::ShaMismatchRule`, `super::stale_comment::StaleCommentRule`, `super::unpinned::UnpinnedRule`, `super::unsynced_manifest::UnsyncedManifestRule`. Also update `report::Report` to `super::report::Report`.
- [x] 1.3 Update `src/lint/mod.rs` to declare `mod command;` alongside existing rule module declarations and `pub mod report;`. Add reexports: `pub use command::{collect_diagnostics, format_and_report, Context, Diagnostic, Error, Lint, Rule, RuleName};`. Remove all logic and imports.
- [x] 1.4 Verify: `mise run build` and `mise run test` pass.

## 2. Final verification

- [x] 2.1 Run `mise run clippy` — no new warnings.
- [x] 2.2 Confirm `lint/mod.rs` contains only `mod` declarations and `pub use` reexports — no struct defs, no impl blocks, no functions.
