## Why

Rule 2 of import path hygiene ("no `crate::` when `super::` suffices") currently has two blind spots: it skips `tests.rs` files entirely, and it only checks file-level (non-indented) `use` statements. This means violations inside inline `#[cfg(test)] mod tests {}` blocks and standalone test files go undetected. The root cause is that the file-path-based parent prefix doesn't account for actual Rust module nesting depth — inline test modules and `#[path = "..."]`-included files sit one level deeper than the file path suggests.

## What Changes

- **Rewrite rule 2 detection** in `import_path_hygiene` to track actual module depth within each file, so it correctly handles inline `mod` blocks (including `mod tests`).
- **Resolve the rule 1 ↔ rule 2 conflict** for inline test modules accessing grandparent items: when `super::super::` is banned (rule 1) and `crate::<parent>::` is the only correct form, rule 2 must not flag it.
- **Handle `tests.rs` files** included via `#[path = "tests.rs"] mod tests;` in non-`mod.rs` files (e.g., `resolve.rs`) by deriving the actual module path from the `mod` declaration site, not the filesystem path.
- **Update the architecture-guardrails spec** to reflect the refined rule 2 semantics.

## Capabilities

### New Capabilities

_(None — this is a refinement of an existing capability.)_

### Modified Capabilities

- `architecture-guardrails`: Rule 2 scenarios updated to cover inline test modules and `#[path]`-included test files.

## Impact

- **`tests/code_health.rs`**: `import_path_hygiene` test rewritten — `module_parent_prefix` replaced with depth-aware detection that tracks `mod {}` nesting.
- **`openspec/specs/architecture-guardrails/spec.md`**: Rule 2 scenarios updated.
- **No source file changes expected** — all current imports already satisfy the refined rule. This change makes the test more precise, not more restrictive.
