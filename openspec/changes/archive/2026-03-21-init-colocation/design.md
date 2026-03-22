## Context

`init/mod.rs` has 81 lines of logic: the `Error` enum, `Init` struct, and `impl Command for Init`. The module also contains `report.rs`. Following the "mod.rs = reexports only" principle applied to all other command and infra modules, the logic should move to a semantically-named file.

## Goals / Non-Goals

**Goals:**
- `init/mod.rs` becomes reexports only — no struct defs, no impl blocks, no error enums
- Maintain identical public API surface (`crate::init::Init`, `crate::init::Error`)
- Mechanical extraction, independently verifiable

**Non-Goals:**
- Changing any behavior, error messages, or public API
- Refactoring the init command logic itself
- Splitting the extracted file further

## Decisions

### 1. Target file named `command.rs`

Create `src/init/command.rs` to hold the `Error` enum, `Init` struct, and `impl Command for Init`. The name `command` matches the dominant concept — this file implements the `Command` trait.

**Alternative considered:** Name it `init.rs` (matching the module name). Rejected because `init/init.rs` is redundant and confusing.

### 2. All imports move with the logic

The `use` statements in `mod.rs` (lines 3–14) are consumed exclusively by the `Error` enum and `Init` impl. They all move to `command.rs`. `mod.rs` retains no imports.

### 3. `mod.rs` reexports both `command` and `report`

After extraction, `mod.rs` becomes:

```rust
mod command;
pub mod report;

pub use command::{Error, Init};
```

This keeps `report` public (it's used by the output layer) and reexports `Error` and `Init` so callers don't need to change.

## Risks / Trade-offs

**[Smallest extraction in the set]** → This is the simplest of all the colocation changes (81 lines, no tests, no constants). Risk is near zero.

**[IDE "go to definition"]** → Jumping to `crate::init::Init` lands on the `pub use` in `mod.rs` first. Standard Rust module practice; most IDEs follow through.
