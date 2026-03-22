# Upgrade Module Colocation

## Summary

Dissolve `upgrade/types.rs` and extract logic from `upgrade/mod.rs` so that `mod.rs` becomes reexports only and every file has a semantic name.

## Motivation

`types.rs` is a non-semantic file name — it says *what kind of syntax* is inside, not *what concept* it represents. Its contents (Plan, Scope, Mode, Request, Error) are the core domain model of the upgrade command, and each type has a natural home in the file that produces or consumes it.

`upgrade/mod.rs` contains 107 lines of command orchestration logic (the `Upgrade` struct and `Command` impl). Under the "mod.rs = reexports only" rule, this logic should move to a semantically-named file.

## Spec gate

Skipped — internal restructuring with no user-visible change.

## Changes

### 1. Create `upgrade/command.rs`

Move from `mod.rs`:
- `RunError` enum
- `Upgrade` struct
- `impl Command for Upgrade`

### 2. Dissolve `upgrade/types.rs`

Move each type to the file that produces or consumes it:

| Type | Destination | Rationale |
|------|------------|-----------|
| `Plan` | `plan.rs` | Produced by `plan::plan()`, only consumed by `command.rs` |
| `Request`, `Scope`, `Mode` | `cli.rs` | Constructed by `cli::resolve_upgrade_mode()` |
| `Error` | `command.rs` | Domain error used by both `plan.rs` and `command.rs`; lives alongside `RunError` |

### 3. Update `upgrade/mod.rs`

Reduce to reexports only:

```rust
pub mod cli;
mod command;
mod plan;
pub mod report;

pub use command::Upgrade;
```

## Risks

- **Circular imports**: `plan.rs` uses `Error` (moving to `command.rs`) and `command.rs` uses `Plan` (moving to `plan.rs`). This creates a cycle. Alternative: keep `Error` in `plan.rs` alongside `Plan`, since `plan()` is what produces errors. Then `command.rs` imports from `plan.rs` only — no cycle.
- Mechanical change — Rust compiler catches all missed imports.
