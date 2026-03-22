## Context

`upgrade/types.rs` is a grab-bag file containing five types (`Plan`, `Scope`, `Mode`, `Request`, `Error`) whose only relationship is "lives in the upgrade module." Meanwhile `upgrade/mod.rs` has 107 lines of command orchestration logic. The "mod.rs = reexports only" rule applied across all other modules should apply here too.

## Goals / Non-Goals

**Goals:**
- Dissolve `types.rs` — every type moves to a file named after the concept it serves
- `mod.rs` becomes reexports only — no struct defs, no impl blocks, no error enums
- Maintain identical public API surface
- Mechanical extraction, independently verifiable

**Non-Goals:**
- Changing any behavior, error messages, or public API
- Refactoring the upgrade command logic itself
- Splitting `plan.rs` or `cli.rs` further

## Decisions

### 1. Create `upgrade/command.rs`

Move from `mod.rs`: all `use` statements, `RunError` enum, `Upgrade` struct, and `impl Command for Upgrade`. The name `command` matches the dominant concept — this file implements the `Command` trait.

### 2. Move `Plan` to `plan.rs`

`Plan` is produced by `plan::plan()` and consumed only by `command.rs`. It belongs in the file that constructs it.

### 3. Move `Request`, `Scope`, `Mode` to `cli.rs`

These types are constructed by `cli::resolve_upgrade_mode()`. They describe CLI-level concepts (what the user asked for). `cli.rs` is their natural home.

### 4. Move `Error` to `plan.rs` (not `command.rs`)

The proposal identified a circular-import risk: `plan.rs` uses `Error` and `command.rs` uses `Plan`. If `Error` goes to `command.rs`, both files import from each other.

Resolution: `Error` moves to `plan.rs` alongside `Plan`. The planning phase is what produces these errors (`ActionNotInManifest`, `TagNotFound`, `TagFetchFailed`, `Workflow`). Then `command.rs` imports from `plan` only — no cycle.

### 5. Move tests with their types

The `#[cfg(test)] mod tests` block in `types.rs` tests `Request`, `Scope`, and `Mode`. It moves to `cli.rs` alongside those types.

### 6. Reduce `mod.rs` to reexports

After extraction:

```rust
pub mod cli;
mod command;
mod plan;
pub mod report;

pub use command::Upgrade;
```

`plan` becomes private (`mod plan` instead of `pub mod plan`) since `Plan` is only used internally by `command.rs`. `types` module declaration is removed entirely.

## Risks / Trade-offs

**[Circular imports]** → Solved by decision #4. `plan.rs` owns both `Plan` and `Error`; `command.rs` imports from `plan` only.

**[Visibility change]** → `plan` and `types` become private. Any external callers of `crate::upgrade::plan` or `crate::upgrade::types` will break at compile time. If needed, add targeted `pub use` reexports — but current codebase inspection shows no external callers.

**[Mechanical change]** → Rust compiler catches all missed imports.
