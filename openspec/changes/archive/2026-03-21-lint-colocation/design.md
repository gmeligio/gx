## Context

`lint/mod.rs` has 354 logic lines: the `RuleName` enum, `Error` enum, `Diagnostic` struct, `Context` struct, `Rule` trait, `Lint` struct, `impl Command for Lint`, ignore-matching helpers (`matches_ignore`, `matches_ignore_action`, `is_ignored`), `collect_diagnostics`, `format_and_report`, and unit tests. The module also contains `report.rs` and four rule files. Following the "mod.rs = reexports only" principle, all logic should move to a semantically-named file.

## Goals / Non-Goals

**Goals:**
- `lint/mod.rs` becomes reexports only — no struct defs, no impl blocks, no error enums, no functions
- Maintain identical public API surface (`crate::lint::*`)
- Mechanical extraction, independently verifiable

**Non-Goals:**
- Changing any behavior, error messages, or public API
- Refactoring the lint logic itself
- Splitting into multiple files (e.g. separating rules framework from command) — 354 lines is within the initial 400 budget

## Decisions

### 1. Target file named `command.rs`

Create `src/lint/command.rs` to hold all logic currently in `mod.rs`. The name `command` matches the dominant concept — this file implements the `Command` trait, same as `init/command.rs`.

**Alternative considered:** `rules.rs` + `command.rs` split. Rejected because 354 lines is within budget, and the split can be done later if needed.

### 2. All imports move with the logic

The `use` statements in `mod.rs` (lines 11–28) are consumed exclusively by the types and functions being extracted. They all move to `command.rs`. `mod.rs` retains no imports.

### 3. Rule module declarations stay in `mod.rs`

The four rule submodules (`sha_mismatch`, `stale_comment`, `unpinned`, `unsynced_manifest`) remain declared in `mod.rs` since they are peer modules, not children of `command.rs`. `command.rs` imports the rule types via `super::`.

### 4. `mod.rs` reexports public types from `command`

After extraction, `mod.rs` becomes:

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

This keeps all existing public paths working. The rule files use `super::` imports (e.g. `super::RuleName`), which resolve through the `pub use` reexport — no changes needed in rule files.

### 5. Tests move with the logic

The `#[cfg(test)] mod tests` block in `mod.rs` moves to `command.rs`. Tests reference `super::` types which will resolve correctly in the new location.

## Risks / Trade-offs

**[Largest command extraction]** → At 354 lines this is larger than init (81 lines), but the extraction is still mechanical — cut and paste with import path adjustments.

**[Rule file imports via `super::`]** → Rule files import types like `super::Diagnostic` and `super::RuleName`. The `pub use` reexport in `mod.rs` ensures `super::Diagnostic` continues to resolve. No changes needed in rule files.

**[`command.rs` imports rule types]** → `command.rs` needs to import `ShaMismatchRule`, `UnpinnedRule`, etc. These become `super::sha_mismatch::ShaMismatchRule` (via the parent module's `mod` declarations).
