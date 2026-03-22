## Context

Four `infra/` submodules have business logic in their `mod.rs` files: `github` (264 lines), `manifest` (203 lines), `lock` (135 lines), and `workflow_scan` (308 lines). Each contains struct definitions, error enums, impl blocks, and trait implementations that should live in semantically-named files. The pattern is identical across all four — extract logic, leave `mod.rs` as reexports only.

This follows the same colocation principle applied to command modules (init, lint, tidy, upgrade) but targets the infra layer.

## Goals / Non-Goals

**Goals:**
- Every `infra/` `mod.rs` becomes reexports only — no struct defs, no impl blocks, no functions
- Maintain identical public API surface (`crate::infra::github::Registry`, `crate::infra::lock::Store`, etc.)
- Keep each extraction mechanical and independently verifiable

**Non-Goals:**
- Changing any behavior, error messages, or public API
- Reorganizing the infra layer beyond moving code out of `mod.rs`
- Splitting any extracted file further (that's a separate concern)

## Decisions

### 1. Target file names match the primary type

Each new file is named after the dominant concept in that module:

| Module | New file | Primary type moved |
|--------|----------|--------------------|
| `infra/github/` | `registry.rs` | `Registry` struct, `Error` enum, `VersionRegistry` impl |
| `infra/manifest/` | `parse.rs` | `parse()`, `parse_lint_config()`, `create()`, `Store`, `Error` |
| `infra/lock/` | `store.rs` | `Store` struct, `Error` enum, `load()`/`save()`, `parse_toml()` |
| `infra/workflow_scan/` | `scanner.rs` | `FileScanner`, `ExtractedAction`, YAML structs, `IoWorkflowError` |

**Alternative considered:** Name files after the operation (`client.rs`, `io.rs`). Rejected because the type name is more discoverable and matches the existing pattern in the codebase (e.g., `domain/lock/resolution.rs` holds `Resolution`).

### 2. `infra/github/tests.rs` inlines into `resolve.rs`

The 98-line `tests.rs` exercises `resolve.rs` functions exclusively. Rather than keeping a `#[path = "tests.rs"]` include, inline the tests as a `#[cfg(test)] mod tests` block at the bottom of `resolve.rs`. This colocates tests with the code they exercise.

**Alternative considered:** Keep `tests.rs` as a separate file with `#[path]` include from `resolve.rs`. This works but adds indirection for no benefit when the test file is small (98 lines) and tests a single file.

### 3. Other test files stay as `#[path]` includes

`manifest/tests.rs`, `lock/tests.rs`, and `workflow_scan/tests.rs` are larger and test the module's public API rather than a single file's internals. They stay as `#[path = "tests.rs"]` includes from the new extracted file (`parse.rs`, `store.rs`, `scanner.rs` respectively). The `#[cfg(test)]` block moves from `mod.rs` to the extracted file since that's where the tested logic now lives.

### 4. Constants stay with their module

`MANIFEST_FILE_NAME` and `LOCK_FILE_NAME` move into their respective extracted files (`parse.rs`, `store.rs`) since they're used by the logic in those files. They remain `pub(crate)` or `pub` as currently scoped, reexported from `mod.rs`.

### 5. Single commit for all four extractions

All four extractions are mechanical and independent. Landing them in one commit reduces review overhead and keeps the "reexports-only mod.rs" story cohesive.

## Risks / Trade-offs

**[IDE "go to definition" changes]** → After extraction, jumping to `crate::infra::github::Registry` lands on the `pub use` in `mod.rs` first. Most IDEs follow through to the definition. → Acceptable; this is standard Rust module practice.

**[`manifest/mod.rs` has a `Store` struct + standalone functions]** → Unlike the other modules where one struct dominates, manifest has both `Store` and free functions (`parse`, `parse_lint_config`, `create`). All move to `parse.rs` since they're cohesive (all deal with reading/writing manifest files). → If the file grows beyond budget, `Store` could split to `store.rs` later.
