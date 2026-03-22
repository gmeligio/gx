# Init Module Colocation

## Summary

Extract command logic from `init/mod.rs` into `init/command.rs`.

## Motivation

`init/mod.rs` has 81 lines of logic (the `Init` command struct, `Error` enum, and `Command` impl). Small but still violates "mod.rs = reexports only." Consistent application of the rule across all command modules.

## Spec gate

Skipped — internal restructuring with no user-visible change.

## Changes

### 1. Create `init/command.rs`

Move from `mod.rs`:
- `Error` enum
- `Init` struct
- `impl Command for Init`

### 2. Update `init/mod.rs`

Reduce to reexports only:

```rust
mod command;
pub mod report;

pub use command::{Error, Init};
```

## Risks

None — smallest of the command module extractions.
