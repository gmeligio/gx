## Why

Two tests in `src/infra/github.rs` (`test_resolve_ref_returns_release_for_tag_with_release` and `test_get_tags_for_sha_includes_annotated_tags`) require a `GITHUB_TOKEN` but live in the unit test module. They use a runtime guard (`if env var missing, return`) that silently skips them during `mise run test`, making them invisible no-ops in CI unit tests and local runs without a token.

## What Changes

- Move the two GITHUB_TOKEN-dependent tests from `src/infra/github.rs` unit tests to a new `tests/e2e_github.rs` file
- Remove the runtime guard pattern — the e2e suite always provides the token
- Update the `mise run e2e` task to include the new test file

## Capabilities

### New Capabilities

_(none — this is a test reorganization, not a new capability)_

### Modified Capabilities

_(none — no spec-level behavior changes)_

## Impact

- `src/infra/github.rs` — two tests removed from `mod tests`
- `tests/e2e_github.rs` — new e2e test file
- `.config/mise.toml` — e2e task updated to run both `e2e_pipeline` and `e2e_github`
- No production code changes, no API changes, no dependency changes
