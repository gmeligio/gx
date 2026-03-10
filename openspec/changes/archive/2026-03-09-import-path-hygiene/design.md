## Overview

Add a `import_path_hygiene` test to `tests/code_health.rs` that enforces three rules on `use` statements in `src/`:

1. **No `super::super::`** — use `crate::` instead
2. **No `crate::` when `super::` suffices** — i.e., when the target is in the same parent module, one `super::` hop away
3. **No `self::`** — it's always redundant

## Rule 2 Detection Logic

This is the only non-trivial rule. The algorithm:

1. For each `.rs` file, derive its **module path** from its filesystem path relative to `src/`:
   - `src/domain/mod.rs` → `domain`
   - `src/domain/plan.rs` → `domain::plan`
   - `src/domain/lock/mod.rs` → `domain::lock`
   - `src/domain/lock/entry.rs` → `domain::lock::entry`

2. Derive the **parent module prefix** (strip the last segment):
   - `domain::plan` → parent is `domain`
   - `domain::lock::entry` → parent is `domain::lock`
   - `domain` (top-level) → parent is the crate root

3. For each `use crate::...` line, check if the import path starts with the parent module prefix. If so, it could be rewritten with `super::` and is a violation.

Example: file `src/domain/plan.rs` (parent = `domain`):
- `use crate::domain::Specifier` → starts with `crate::domain` → **violation** (use `super::Specifier`)
- `use crate::config::Config` → starts with `crate::config` → **OK** (different top-level module)

Example: file `src/domain/lock/entry.rs` (parent = `domain::lock`):
- `use crate::domain::lock::LockEntry` → starts with `crate::domain::lock` → **violation** (use `super::LockEntry`)
- `use crate::domain::CommitSha` → starts with `crate::domain` but NOT `crate::domain::lock` → **OK** (would need `super::super::`, which is banned separately)

### Edge cases

- **`mod.rs` files**: `src/domain/mod.rs` has parent = crate root. A `use crate::domain::*` import from here is actually accessing its own children, reachable via bare path or `self::`. Since we ban `self::` (rule 3), the bare path is correct. We should flag `crate::domain::*` from `domain/mod.rs` as a violation — the fix is to use the bare child module name directly.
- **Test modules (`#[cfg(test)] mod tests`)**: These are conceptually inside the same file/module. `super::` from a test module refers to the enclosing module, which is correct. `crate::` should follow the same rules — if the test's enclosing module is `domain::plan`, then `use crate::domain::*` inside the test is still a violation.
- **Comments**: Lines starting with `//` are skipped (already done by existing tests).
- **`main.rs`**: Uses `gx::*` — excluded from scanning (already excluded as it's not under `src/` module tree in the same way, but to be safe, skip non-`lib` entry points).

## Violation Inventory

### Rule 1 — `super::super::` (7 occurrences, 4 files)

| File | Context |
|------|---------|
| `infra/manifest/patch.rs:237` | test module |
| `infra/github/tests.rs:1` | test helper |
| `lint/stale_comment.rs:117,145,176` | test module |
| `infra/lock/convert.rs:124` | test module |
| `infra/manifest/convert.rs:237` | test module |

Fix: replace `super::super::X` with `crate::<module>::X`.

### Rule 2 — `crate::` when `super::` suffices (~20 occurrences, 6 files)

All in `domain/` — files at depth 1 (`domain/plan.rs`, `domain/event.rs`, `domain/resolution.rs`) and `mod.rs` files (`domain/lock/mod.rs`, `domain/manifest/mod.rs`).

Files at depth 2+ (like `domain/action/tag_selection.rs`, `domain/lock/entry.rs`, `domain/manifest/overrides.rs`) use `crate::domain::*` correctly — because reaching those targets via `super::` would require `super::super::` which violates rule 1.

Fix: replace `crate::<parent_module>::X` with `super::X`.

### Rule 3 — `self::` (7 occurrences, 4 files)

| File | Lines |
|------|-------|
| `init/mod.rs` | 1 |
| `upgrade/mod.rs` | 4 |
| `tidy/mod.rs` | 1 |
| `lint/mod.rs` | 1 |

Fix: remove `self::` prefix (e.g., `use self::report::X` → `use report::X`).

## Test Implementation

Single test function `import_path_hygiene` that:

1. Iterates all `.rs` files in `src/` via `collect_rs_files`
2. For each file, derives the parent module prefix from the file path
3. For each non-comment line containing `use `:
   - Check for `super::super::` → rule 1 violation
   - Check for `use crate::<parent_prefix>::` → rule 2 violation
   - Check for `use self::` or `use self::{` → rule 3 violation (also check `super::` re-exports like `pub use self::`)
4. Collect all violations with file path and line content
5. Assert empty with descriptive message

Helper function `module_parent_prefix(file_path, src_dir) -> Option<String>` extracts the `crate::` prefix that corresponds to the file's parent module.
