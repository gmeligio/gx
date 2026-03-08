## ADDED Requirements

### Requirement: GITHUB_TOKEN-dependent tests run in e2e suite
Tests that call the real GitHub API and require `GITHUB_TOKEN` SHALL live in the `tests/e2e_*.rs` files and run under `mise run e2e`, not in unit tests.

#### Scenario: e2e suite includes github registry tests
- **WHEN** `mise run e2e` is executed with a valid `GITHUB_TOKEN`
- **THEN** `test_resolve_ref_returns_release_for_tag_with_release` and `test_get_tags_for_sha_includes_annotated_tags` SHALL execute and pass

#### Scenario: unit tests do not silently skip token-dependent tests
- **WHEN** `mise run test` is executed without `GITHUB_TOKEN`
- **THEN** no test SHALL contain a runtime guard that silently skips when the token is absent
