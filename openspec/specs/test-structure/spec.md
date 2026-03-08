## ADDED Requirements

### Requirement: Test files SHALL use type-based naming prefixes

All integration test files in `tests/` SHALL be prefixed with `integ_` and all end-to-end test files SHALL be prefixed with `e2e_`. Files that are neither (e.g., `code_health.rs`) retain their current names.

#### Scenario: Integration test file naming
- **WHEN** a test file contains mock-based tests
- **THEN** the file name MUST start with `integ_` (e.g., `integ_tidy.rs`, `integ_upgrade.rs`, `integ_pipeline.rs`, `integ_lint.rs`, `integ_repo.rs`)

#### Scenario: E2e test file naming
- **WHEN** a test file contains tests using real `GithubRegistry`
- **THEN** the file name MUST start with `e2e_` (e.g., `e2e_pipeline.rs`)

### Requirement: Shared test code SHALL live in `tests/common/`

Mock registries and test setup helpers SHALL be consolidated in `tests/common/mod.rs` with submodules. No `#[allow(dead_code)]` SHALL be used.

#### Scenario: Registry imports from common module
- **WHEN** a test file needs a mock registry
- **THEN** it SHALL import from `common::registries` (e.g., `common::registries::FakeRegistry`)

#### Scenario: Setup helper imports from common module
- **WHEN** a test file needs repo setup helpers
- **THEN** it SHALL import from `common::setup` (e.g., `common::setup::create_test_repo`)

### Requirement: FakeRegistry SHALL be the primary mock registry

`FakeRegistry` SHALL replace `E2eRegistry`, `ShaAwareRegistry`, `MockRegistry`, and `MockUpgradeRegistry`. It SHALL use hash-based deterministic SHA generation and support builder methods for configuring tags and SHA-to-tag mappings.

#### Scenario: Default FakeRegistry behavior
- **WHEN** `FakeRegistry::new()` is used without configuration
- **THEN** `lookup_sha` returns deterministic SHAs, `tags_for_sha` returns empty, `all_tags` returns empty, `describe_sha` returns empty tags with a fixed date

#### Scenario: FakeRegistry with tag configuration
- **WHEN** `FakeRegistry::new().with_all_tags("actions/checkout", vec!["v4", "v5"])` is used
- **THEN** `all_tags` for `actions/checkout` returns `[v4, v5]`

#### Scenario: FakeRegistry with SHA-to-tag mapping
- **WHEN** `FakeRegistry::new().with_sha_tags("actions/checkout", sha, vec!["v4", "v4.2.1"])` is used
- **THEN** `tags_for_sha` for that SHA returns `[v4, v4.2.1]` and `describe_sha` includes those tags

### Requirement: AuthRequiredRegistry SHALL replace NoopRegistry

`AuthRequiredRegistry` SHALL return `ResolutionError::AuthRequired` from all `VersionRegistry` methods.

#### Scenario: All methods return auth error
- **WHEN** any `VersionRegistry` method is called on `AuthRequiredRegistry`
- **THEN** it returns `Err(ResolutionError::AuthRequired)`

### Requirement: No `#[ignore]` attributes SHALL exist in the codebase

Test separation SHALL be achieved via file naming and `cargo test --test` targeting. The `code_health.rs` budget test SHALL scan both `src/` and `tests/` directories with `max_ignored = 0`.

#### Scenario: code_health test scans both directories
- **WHEN** the `ignore_attribute_budget` test runs
- **THEN** it scans both `src/` and `tests/` for `#[ignore]` attributes and asserts the count is 0

### Requirement: Mise tasks SHALL separate test categories

Three mise tasks SHALL exist for running different test categories.

#### Scenario: Unit tests task
- **WHEN** `mise run test` is executed
- **THEN** it runs `cargo test --lib` (unit tests in `src/` only)

#### Scenario: Integration tests task
- **WHEN** `mise run integ` is executed
- **THEN** it runs integration tests from all `integ_*.rs` files plus `code_health.rs`

#### Scenario: E2e tests task
- **WHEN** `mise run e2e` is executed
- **THEN** it runs `cargo test` targeting all `e2e_*.rs` files with `GITHUB_TOKEN` set

### Requirement: CI SHALL have three parallel test jobs

The build workflow SHALL have separate jobs for unit tests, integration tests, and e2e tests, all running in parallel.

#### Scenario: CI job structure
- **WHEN** a PR is opened
- **THEN** three test jobs run in parallel: `unit-tests` (runs `mise run test`), `integration-tests` (runs `mise run integ`), `e2e-tests` (runs `mise run e2e` with `GITHUB_TOKEN` secret)
