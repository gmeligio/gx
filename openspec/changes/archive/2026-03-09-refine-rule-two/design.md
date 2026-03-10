## Context

Rule 2 of `import_path_hygiene` ("no `crate::<parent>::` when `super::` suffices") was implemented using a file-path-based parent prefix. Two workarounds were added to avoid false positives:

1. **Skip `tests.rs` files entirely** — because `infra/github/tests.rs` is included via `#[path = "tests.rs"] mod tests;` inside `resolve.rs`, making its actual module `infra::github::resolve::tests`, not `infra::github::tests`.
2. **Skip indented `use` statements** — because inline `#[cfg(test)] mod tests {}` blocks sit one module level deeper than the file path suggests. An import like `use crate::infra::lock::{FileLock}` inside `infra::lock::convert::tests` is correct (not replaceable by `super::`) but was incorrectly flagged by the file-path-based check.

These workarounds leave blind spots: rule 2 violations inside inline test modules and standalone `tests.rs` files are never checked.

### `tests.rs` inclusion map

| File | Included by | Actual module path |
|------|-------------|-------------------|
| `infra/lock/tests.rs` | `infra/lock/mod.rs` | `infra::lock::tests` |
| `infra/manifest/tests.rs` | `infra/manifest/mod.rs` | `infra::manifest::tests` |
| `infra/workflow_scan/tests.rs` | `infra/workflow_scan/mod.rs` | `infra::workflow_scan::tests` |
| `tidy/tests.rs` | `tidy/mod.rs` | `tidy::tests` |
| `infra/github/tests.rs` | `infra/github/resolve.rs` | `infra::github::resolve::tests` |

All but the last are included by their directory's `mod.rs`, so their file-path-derived module path is correct. The `infra/github/tests.rs` case is the outlier — its includer is `resolve.rs`, placing it one level deeper.

## Goals / Non-Goals

**Goals:**
- Eliminate both workarounds (no more blanket skipping of `tests.rs` or indented lines)
- Correctly compute the parent prefix for inline `mod` blocks (depth-aware)
- Correctly compute the parent prefix for `tests.rs` files by resolving the includer
- Zero false positives — only flag imports truly replaceable by `super::`

**Non-Goals:**
- Catching ALL theoretically replaceable imports inside inline test modules. Specifically: if `domain/plan.rs` imports `Specifier` from `super::`, then `use crate::domain::Specifier` inside its `mod tests {}` is technically replaceable by `super::Specifier` — but detecting this requires resolving namespace inheritance, which is out of scope. The file-level violations are already caught, and inline test modules will get the stricter check (violations against the file's own module path).

## Decisions

### 1. Depth-aware parent prefix via indentation

**Decision**: Use two tiers of parent prefix per file, selected by indentation level.

- **Indent 0 (file-level code)**: parent prefix = file's parent module (from file path). Same as today.
- **Indent 4+ (inside inline mod blocks)**: parent prefix = file's own module. This is one level more specific, matching the fact that inline `mod tests {}` adds a nesting level.

**Why**: Brace-tracking across `mod`, `fn`, `impl`, `match`, etc. is fragile. Indentation is reliable because `rustfmt` enforces consistent formatting in this project. A file-level `use` is always at column 0; an inline-module `use` is always indented.

**Examples**:

| File | Line | Indent | Parent prefix | Flagged? |
|------|------|--------|---------------|----------|
| `domain/plan.rs` | `use crate::domain::X` | 0 | `crate::domain` | Yes — use `super::X` |
| `domain/plan.rs::tests` | `use crate::domain::plan::X` | 4+ | `crate::domain::plan` | Yes — use `super::X` |
| `domain/plan.rs::tests` | `use crate::domain::X` | 4+ | `crate::domain::plan` | No — needs `super::super::X` (rule 1 territory) |
| `infra/lock/convert.rs::tests` | `use crate::infra::lock::{FileLock}` | 4+ | `crate::infra::lock::convert` | No — correct, not replaceable |
| `infra/lock/convert.rs` | `use crate::infra::lock::X` | 0 | `crate::infra::lock` | Yes — use `super::X` |

**Alternative considered**: Brace-depth tracking to compute exact module path. Rejected — too fragile for a code-health test. String literals, macros, and nested braces in match arms make accurate tracking unreliable without a proper parser.

### 2. Resolve `tests.rs` includer for correct module path

**Decision**: For each `tests.rs` file, search sibling `.rs` files in the same directory for a `mod tests;` declaration. Use the includer's identity to derive the actual module path:

- If the includer is `mod.rs` → the `tests.rs` module path matches the directory (standard case). Compute parent prefix from file path as normal.
- If the includer is another file (e.g., `resolve.rs`) → the actual module path includes the includer as an extra segment. Adjust parent prefix accordingly.

**Algorithm**:
1. For a `tests.rs` file at `src/<a>/<b>/tests.rs`, list all `.rs` files in `src/<a>/<b>/`.
2. Search each for a line matching `mod tests;` (ignoring comments and `cfg` attributes).
3. If the includer is `mod.rs`: file-level parent prefix = parent of directory module (e.g., `crate::<a>` for `<a>/<b>/tests.rs`).
4. If the includer is `<other>.rs`: file-level parent prefix = directory module + includer stem (e.g., `crate::<a>::<b>::<other>` for `tests.rs` included by `<other>.rs`).

For indented code inside the `tests.rs` file, the prefix adds one more segment as in decision 1.

**Why not skip `tests.rs` entirely**: The current blanket skip means NO rule 2 checking for standalone test files. With this approach, all `tests.rs` files are checked with the correct parent prefix.

### 3. Compute indented parent prefix from file module path

**Decision**: The helper `module_parent_prefix` is replaced by a function that returns BOTH the file-level and indented-level parent prefixes:

```
fn parent_prefixes(file_path, src_dir) -> (Option<String>, Option<String>)
```

Returns `(file_level_prefix, indented_prefix)`:
- `file_level_prefix`: parent of the file's module (same as today's `module_parent_prefix`)
- `indented_prefix`: the file's own module as a `crate::` path (for use inside inline `mod` blocks)

For top-level modules (parent is crate root): `file_level_prefix = None`, `indented_prefix = None`.

For `tests.rs` files: the includer resolution adjusts both prefixes accordingly.

## Risks / Trade-offs

**[Missed inline violations]** → Acceptable. `use crate::domain::X` inside `domain/plan.rs::tests` is technically replaceable by `super::X`, but the refined test won't flag it. Detecting this would require namespace analysis. File-level violations (the primary source) are fully covered, and the existing fixes prevent regression.

**[Indentation heuristic]** → Robust in practice. The project uses `rustfmt`, so all file-level `use` is at indent 0 and all inline-module `use` is at indent 4+. If formatting changes, the test may produce false positives or miss violations — but that's also true of any line-based heuristic and the test would fail visibly, prompting a fix.

**[Single includer assumption for tests.rs]** → Safe. Rust's module system requires exactly one `mod tests;` per test module. Multiple declarations would be a compile error.
