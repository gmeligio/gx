## Context

The project has 3 test suites: unit (`cargo test --lib`), integration (`tests/integ_*.rs`), and e2e (`tests/e2e_pipeline.rs`). The e2e suite is the only one that receives `GITHUB_TOKEN` via mise. Two tests in `src/infra/github.rs` call the real GitHub API but live in the unit test module with a silent runtime skip guard.

## Goals / Non-Goals

**Goals:**
- Ensure GITHUB_TOKEN-dependent tests actually run in CI (in the e2e suite)
- Remove silent skip pattern that hides test coverage gaps

**Non-Goals:**
- Changing test behavior or assertions
- Refactoring GithubRegistry API surface
- Adding new tests

## Decisions

**Move tests to `tests/e2e_github.rs` rather than adding to `tests/e2e_pipeline.rs`**

Rationale: `e2e_pipeline.rs` tests the full init/tidy/upgrade pipeline. The two GitHub API tests are isolated registry method tests. A separate file keeps concerns clear and follows the existing naming pattern (`e2e_*.rs`).

**Reuse the `github_registry()` helper pattern from `e2e_pipeline.rs`**

Rationale: Same token-from-env pattern already proven in the e2e suite. The new file duplicates this small helper rather than extracting to `common/` since it's a 3-line function used in only 2 files.

**Update mise task to use for-loop pattern like `mise run integ`**

Rationale: Matches the existing `integ` task pattern (`cargo test $(for f in tests/e2e_*.rs; do echo --test $(basename $f .rs); done)`). Automatically picks up any future `e2e_*.rs` files without manual edits.

## Risks / Trade-offs

- [Duplication of `github_registry()` helper] → Acceptable for 3 lines; extract to `common/` if a third e2e file appears
- [New test file increases e2e suite scope] → Minimal; only 2 focused tests added
