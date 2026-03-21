# Lint Module Colocation

## Summary

Extract command logic from `lint/mod.rs` into `lint/command.rs`. This is the largest `mod.rs` offender at 354 logic lines.

## Motivation

`lint/mod.rs` contains the `Lint` command struct, `Command` impl, `RuleName` enum, `Diagnostic` struct, `Rule` trait, and rule dispatch logic — all mixed together. At 354 logic lines, it's the largest `mod.rs` in the project and the furthest from the "reexports only" target.

## Spec gate

Skipped — internal restructuring with no user-visible change.

## Changes

### 1. Create `lint/command.rs`

Move from `mod.rs`:
- `Error` enum
- `RuleName` enum + `Display`/`FromStr` impls
- `Diagnostic` struct
- `Rule` trait
- `Lint` struct + `Command` impl
- `Context` struct, ignore-matching helpers, `collect_diagnostics`, `format_and_report`

The lint rules themselves (`sha_mismatch.rs`, `stale_comment.rs`, `unpinned.rs`, `unsynced_manifest.rs`) remain as separate files — they're already well-named and colocated.

### 2. Update `lint/mod.rs`

Reduce to reexports only:

```rust
mod command;
pub mod report;
mod sha_mismatch;
mod stale_comment;
mod unpinned;
mod unsynced_manifest;

pub use command::{
    collect_diagnostics, format_and_report, Context, Diagnostic, Error, Lint, Rule, RuleName,
};
```

### 3. Consider splitting further

At 354 logic lines, `command.rs` is above the 300 target. A natural split:

- `lint/rules.rs` — `RuleName` enum, `FromStr`, `Rule` trait, `Diagnostic` (the rule framework)
- `lint/command.rs` — `Lint` struct, `Command` impl, dispatch logic

This would put both files well under 200 logic lines. However, this split is optional — 354 is within the initial 400 budget and can be addressed later.

## Risks

- The `Rule` trait is implemented by the rule files (`sha_mismatch.rs`, etc.), which use `super::` imports. Moving the trait from `mod.rs` to `command.rs` changes the import path from `super::Rule` to `super::command::Rule`. The `mod.rs` reexport (`pub use command::Rule`) makes `super::Rule` continue to work.
