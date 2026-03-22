# Design: Tidy Module Colocation

## File Layout (Before → After)

### Before

```
src/tidy/
  mod.rs          ← 265 lines: Error, Plan, Tidy, Command impl, plan(), apply_workflow_patches()
  tests.rs        ← 429 lines: integration tests for plan() and apply_workflow_patches()
  lock_sync.rs
  manifest_sync.rs
  patches.rs
  report.rs
```

### After

```
src/tidy/
  mod.rs          ← reexports only (~10 lines)
  command.rs      ← Error, Plan, Tidy, Command impl, plan(), apply_workflow_patches() (~265 lines)
  command_tests.rs ← tests for plan() and apply_workflow_patches() (~429 lines, owned by command.rs via #[path])
  lock_sync.rs    (unchanged)
  manifest_sync.rs (unchanged)
  patches.rs      (unchanged)
  report.rs       (unchanged)
```

`tests.rs` is renamed to `command_tests.rs` and owned by `command.rs` via `#[path]`. This keeps both files within the 550-line budget while colocating test ownership with the code under test.

## mod.rs (After)

```rust
mod command;
mod lock_sync;
mod manifest_sync;
mod patches;
pub mod report;

pub use command::{Error, Plan, Tidy};
```

Public API paths (`crate::tidy::Error`, `crate::tidy::Plan`, `crate::tidy::Tidy`) remain unchanged.

## command.rs Structure

```rust
// --- imports ---

#[derive(Debug)]
pub struct Plan { ... }

#[derive(Debug, Error)]
pub enum Error { ... }

pub fn plan<R, P>(...) -> Result<Plan, Error> { ... }

pub fn apply_workflow_patches<W>(...) -> Result<usize, Error> { ... }

#[derive(Debug, Error)]
pub enum RunError { ... }

pub struct Tidy;

impl Command for Tidy {
    fn run(...) -> Result<Report, RunError> { ... }
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
```

**Line budget**: `command.rs` is ~265 logic lines (within the 300-line target). Tests live in `command_tests.rs` (~429 lines), keeping both files within the 550-line budget.

## Import Changes

All external consumers import from `crate::tidy::*` which is unchanged. Internal changes:

- `tests.rs` → renamed to `command_tests.rs` (no import changes needed — `use super::*` still resolves correctly since the test module is still a child of the module containing `plan()` and `apply_workflow_patches()`)
- `mod.rs` `mod tests;` → removed (test module now declared in `command.rs` via `#[path]`)

## Risks

- **Mechanical change**: Rust compiler catches any missed imports. No behavioral change.
