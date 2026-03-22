# Code Health Gates — Design

## Approach

All three new gates follow the existing pattern in `code_health.rs`: a `#[test]` function that scans `src/`, collects violations, and asserts with a budget. Each gate has a current budget (must pass today) and a documented target (where we want to be after restructuring).

## Gate 1: `logic_line_budget`

### Counting logic lines

A file's "logic lines" = total lines minus lines inside `#[cfg(test)]` blocks.

**Algorithm:**
1. Collect all `.rs` files in `src/`
2. Skip standalone test files — files named `tests.rs` (these are entirely test code regardless of whether `#[cfg(test)]` appears inside them)
3. For remaining files, scan line by line:
   - When `#[cfg(test)]` is found at the start of a trimmed line, mark the start position
   - Track brace depth from that point: `{` increments, `}` decrements
   - When depth returns to 0 (or we hit a new top-level `mod`/`fn`/`struct`/`enum`/`impl` declaration), mark the end
   - Subtract the `#[cfg(test)]` block length from total
4. Edge case: `#[cfg(test)]` followed by `mod tests;` (single line, no braces) — the test code lives in a separate file, so only subtract the declaration lines (the `#[cfg(test)]` + attributes + `mod tests;`)

**Simplification:** In this codebase, `#[cfg(test)]` blocks always appear at the bottom of the file and extend to EOF. So the algorithm simplifies to: find the first `#[cfg(test)]` line, logic lines = that line number - 1. If no `#[cfg(test)]`, logic lines = total lines.

This matches exactly what we validated with the shell analysis. Use the simple approach; add the general approach only if the codebase evolves to have mid-file test blocks.

**Known limitation:** A single-line `#[cfg(test)] use ...` import (test-only import not inside a `mod tests` block) would trigger the simplified cutoff too early. The secondary assertion below catches this — if it ever fires, upgrade to the general brace-tracking algorithm. Document this limitation in the helper's doc comment.

**Assumption validation:** The `logic_line_budget` gate should include a secondary assertion that verifies the "cfg(test) always at bottom" invariant. For every file with `#[cfg(test)]`, check that no production code (non-comment, non-attribute, non-blank lines) appears *after* the first `#[cfg(test)]` line. Under the simplified algorithm, everything from `#[cfg(test)]` to EOF is treated as test code, so the invariant being validated is: "all production code precedes the `#[cfg(test)]` line." If this assertion ever fires, the simple counting approach needs upgrading to the general brace-tracking algorithm.

#### Scenario: Invariant violation detected

- **GIVEN** a `.rs` file where `#[cfg(test)]` appears mid-file with production code after it
- **WHEN** the `logic_line_budget` gate runs
- **THEN** the secondary assertion fails with a message identifying the file and the offending line
- **AND** the message instructs the developer to upgrade to the general brace-tracking algorithm

### Budget

```rust
let max_logic_lines: usize = 440;
// Target: 300 once infra/github/resolve.rs and lint/mod.rs are split.
// Current max: 438 (infra/github/resolve.rs)
```

Why 440 not 438: small headroom so minor edits don't immediately break the gate.

## Gate 2: `mod_rs_reexports_only`

### Counting logic lines in mod.rs

For each `mod.rs` file, count lines that are NOT:
- Blank lines
- Comments (`//`, `///`, `//!`, block comments)
- Attributes (`#[...]`, `#![...]`)
- Module declarations (`mod foo;`, `pub mod foo;`, `pub(crate) mod foo;`)
- Use/reexport statements (`use ...;`, `pub use ...;`)
- `#[cfg(test)]` blocks (same subtraction as gate 1)

Everything else counts as "logic in mod.rs."

**Implementation:** First, call `count_non_test_lines()` from gate 1 to determine the non-test line range (everything before the first `#[cfg(test)]`). Then, for those lines only, use a `ModRsScanner` struct to classify structural vs. logic lines. The scanner tracks two pieces of mutable state: `in_use_block: bool` and `use_brace_depth: usize`, and exposes a `fn is_structural_line(&mut self, line: &str) -> bool` method.

1. **Prefix check** — a line is structural if its trimmed form starts with any of:
   ```
   "", "//", "///", "//!", "/*", "*/",
   "#[", "#![",
   "mod ", "pub mod ", "pub(crate) mod ", "pub(super) mod ",
   "use ", "pub use ", "pub(crate) use ", "pub(super) use "
   ```

   Note: bare `*` is intentionally excluded from the prefix list. While `*` often appears as a block comment continuation, it also appears as a dereference operator in production code.

   **Known limitation (block comments):** The scanner does not track block comment state. Lines inside a `/* ... */` block that don't start with `*`, `/*`, or `*/` will be miscounted as logic. This is rare in practice (most block comment lines start with ` *`), and the budget approach tolerates minor miscounts. **Validated:** the re-measure task must confirm no current `mod.rs` contains block comments that trigger this miscount; if any do, increase the budget accordingly or upgrade the scanner.

2. **Multi-line `use` blocks** — `use` statements with brace-grouped imports span multiple lines:
   ```rust
   use crate::domain::{   // ← structural (starts with "use")
       Lock,               // ← continuation, also structural
       LockEntry,          // ← continuation
   };                      // ← closing brace, structural
   ```
   Track brace depth: when a structural `use` line opens `{` without closing, set `in_use_block = true` and track depth. While inside a use block, all lines are structural. Reset when braces balance.

Lines not matching any prefix and not inside a `use {}` block are "logic lines."

### Budget

The budget applies **per-file** — the single worst `mod.rs` must not exceed it. (Same semantics as `logic_line_budget` and `file_size_budget`.)

```rust
let max_mod_logic: usize = 360;
// Target: 0 (mod.rs should be reexports only).
// Current max: 354 (lint/mod.rs). Headroom for minor multi-line use edge cases.
```

### Reporting

The assertion message should list every `mod.rs` with logic lines > 0 (not just those exceeding the budget), so we can track progress:

```
mod.rs files with logic (budget: 360, target: 0):
  src/lint/mod.rs: 354 logic lines
  src/infra/workflow_scan/mod.rs: 300 logic lines
  ...
```

Only assert on the budget, but print all for visibility.

## Gate 3: `no_generic_file_names`

### Denylist

```rust
let denied = ["types.rs", "utils.rs", "helpers.rs", "common.rs", "misc.rs", "consts.rs", "constants.rs"];
```

Scan all `.rs` files in `src/`. Any file whose name matches the denylist is a violation.

### Budget

```rust
let max_generic_names: usize = 1;
// Target: 0.
// Current violation: upgrade/types.rs
```

## File organization

All three gates go in `tests/code_health.rs` alongside the existing gates. They reuse the existing `collect_rs_files()` helper for recursive `.rs` file collection. No new files needed.

### Section headers

Follow the existing pattern. Task numbers 1.5–1.7 continue the existing sequence (Tasks 1.1–1.4 are already in the codebase). Confirmed no in-flight change claims these numbers.

```rust
// ---------------------------------------------------------------------------
// Task 1.5 — Logic line budget
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Task 1.6 — mod.rs reexports only
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Task 1.7 — No generic file names
// ---------------------------------------------------------------------------
```

## Helper reuse

The `#[cfg(test)]` block detection (needed by gates 1 and 2) should be a shared helper:

```rust
/// Count non-test lines in a file, excluding `#[cfg(test)]` blocks and everything after.
///
/// Uses a simplified algorithm: finds the first `#[cfg(test)]` line and treats
/// everything from that point to EOF as test code. This assumes `#[cfg(test)]`
/// blocks always appear at the bottom of the file — a secondary assertion in
/// `logic_line_budget` validates this invariant.
///
/// Known limitation: a single-line `#[cfg(test)] use ...` would trigger the
/// cutoff too early. The invariant assertion catches this case.
fn count_non_test_lines(content: &str) -> usize
```

Both gates call this. Gate 2 additionally filters structural lines to arrive at "logic in mod.rs."
