## Why

The test suite has grown organically, resulting in unclear naming (`e2e_test.rs` contains mocked integration tests, not real e2e), duplicated mock registries (7 implementations across files), and no shared test utilities. This makes tests harder to maintain and understand.

## What Changes

- **Rename test files** with `integ_` or `e2e_` prefixes to clearly indicate test type
- **Split `e2e_test.rs`** into mock-based integration tests (`integ_pipeline.rs`) and real e2e tests (`e2e_pipeline.rs`)
- **Consolidate 7 duplicate mock registries** into a shared `tests/common/registries.rs` module with semantic names (`FakeRegistry`, `AuthRequiredRegistry`, etc.)
- **Extract shared test helpers** (repo init, workspace setup) into `tests/common/setup.rs`
- **Convert eligible tests** to real e2e using `GithubRegistry` in `e2e_pipeline.rs`, targeted via `--test` instead of `#[ignore]`
- **Separate CI into 3 jobs**: unit tests (`cargo test --lib`), integration tests (`--test integ_*`), e2e tests (`--test e2e_*` with `GITHUB_TOKEN`)
- **Update mise tasks**: `test` (unit), `integ` (integration), `e2e` (e2e with token)
- **Update `code_health.rs`**: scan both `src/` and `tests/`, set `max_ignored = 0`
- **Remove all `#[ignore]` attributes** — test separation is handled by file naming and `--test` targeting

## Capabilities

### New Capabilities

- `test-structure`: Shared test infrastructure (`tests/common/`) with consolidated mock registries, setup helpers, and clear file naming convention (`integ_*`, `e2e_*`)

### Modified Capabilities

<!-- No spec-level behavior changes — this is a test-only restructuring -->

## Impact

- All files in `tests/` are affected (renamed, split, or modified to use shared code)
- No changes to `src/` — application code is unchanged
- `.config/mise.toml` — tasks split into `test`, `integ`, `e2e`
- `.github/workflows/build.yml` — CI jobs split into unit-tests, integration-tests, e2e-tests (all parallel)
- `code_health.rs` — expanded scope and zero ignore budget
