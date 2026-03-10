## Context

The infra layer provides I/O adapters for manifest, lock, GitHub API, and workflow files. Four files have grown past the 500-line budget enforced by `code_health.rs`. Each file mixes multiple concerns: serialization types, conversion logic, file I/O, surgical TOML editing, and tests.

The project already has precedent for this split pattern: `infra/workflow.rs` was previously split into `workflow_scan.rs` and `workflow_update.rs`, and `infra/lock_migration.rs` was extracted from `lock.rs`.

## Goals / Non-Goals

**Goals:**
- Every infra `.rs` file under 500 lines
- Each file has a single clear responsibility
- Public API surface unchanged — only internal reorganization
- Tests co-located with the code they test (in-module `#[cfg(test)]`)

**Non-Goals:**
- Changing any public function signatures
- Moving logic between layers (that's the domain change)
- Refactoring test logic or reducing test count

## Decisions

### 1. Use subdirectories with `mod.rs` for files that split into 3+ parts

**Decision**: `infra/manifest.rs` → `infra/manifest/mod.rs` + siblings. Same for `lock/` and `github/`.

**Rationale**: Flat sibling files (`manifest_convert.rs`, `manifest_patch.rs`) pollute the `infra/` directory and don't express hierarchy. Subdirectories group related concerns. The `mod.rs` re-exports everything so callers don't change.

### 2. Split by concern, not by struct

**Decision**: Group code by what it does (conversion, patching, I/O) rather than by which struct it touches.

**Rationale**: A file like `manifest_patch.rs` containing `apply_manifest_diff` + all its override helpers is cohesive — they're all about surgical TOML editing. Splitting by struct would scatter the patching logic.

### 3. Tests stay with their code

**Decision**: Each new submodule carries its own `#[cfg(test)] mod tests` block with the tests relevant to that module's functions.

**Rationale**: Co-located tests are easier to maintain and make file sizes more honest (a 400-line file with 200 lines of tests is fine — it's a 200-line module with good coverage).

### 4. `workflow_scan.rs` — extract tests only

**Decision**: The code portion (~280 lines) is well under budget. Extract the test module (~347 lines) into a sibling `workflow_scan_tests.rs` or inline test submodule.

**Rationale**: The scan logic is cohesive and doesn't benefit from further splitting. Only the tests push it over budget.

## Target Structure

```
src/infra/
  manifest/
    mod.rs          ~250 lines  — ManifestError, FileManifest, parse_manifest,
                                   create_manifest, parse_lint_config, re-exports
    convert.rs      ~220 lines  — TOML wire types, manifest_from_data,
                                   manifest_to_data, format_manifest_toml
    patch.rs        ~200 lines  — apply_manifest_diff, override helpers
    tests.rs        ~500 lines  — all manifest tests (or split further if needed)
  lock/
    mod.rs          ~300 lines  — LockFileError, FileLock, parse_lock,
                                   create_lock, apply_lock_diff
    convert.rs      ~120 lines  — wire types, lock_from_data, serialize_lock,
                                   build_lock_inline_table
    tests.rs        ~465 lines  — all lock tests
  github/
    mod.rs          ~350 lines  — GithubError, GithubRegistry struct, constructor,
                                   HTTP helpers, VersionRegistry impl, Default
    responses.rs    ~70 lines   — all DTO structs
    resolve.rs      ~300 lines  — resolve_ref, fetch_ref, get_tags_for_sha,
                                   get_version_tags, fetch dates, resolve_version_for_sha
    tests.rs        ~110 lines  — all github tests
  lock_migration.rs             — unchanged
  workflow.rs                   — unchanged (re-export stub)
  workflow_scan.rs  ~280 lines  — code only
  workflow_scan/tests.rs ~347   — or keep inline if we can trim
  workflow_update.rs            — unchanged
  mod.rs                        — updated re-exports
  repo.rs                       — unchanged
```

## Risks / Trade-offs

- **Import churn**: Internal `use` paths change within infra. Callers outside infra should be unaffected since `infra/mod.rs` re-exports the public API.
- **Test file at budget edge**: `manifest/tests.rs` at ~500 lines is right at the limit. If it grows, it'll need further splitting. Acceptable for now.
- **`workflow_scan.rs` borderline**: At 627 lines, it's only 127 over. Extracting just the test module is sufficient. No need for a full directory split.
