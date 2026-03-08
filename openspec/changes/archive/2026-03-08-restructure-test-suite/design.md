## Context

The project has 6 test files in `tests/`:
- `e2e_test.rs` (15 tests, 5 mock registries) — misleadingly named, all use mocks
- `tidy_test.rs` (18 tests, 2 mock registries) — tidy-specific integration tests
- `upgrade_test.rs` (16 tests, 1 mock registry) — upgrade-specific integration tests
- `lint_test.rs` (10 tests, no registry) — lint integration tests
- `repo_test.rs` (2 tests, no registry) — repo detection tests
- `code_health.rs` (1 test) — meta test counting `#[ignore]` attributes

7 mock `VersionRegistry` implementations are duplicated across files: `E2eRegistry`, `ShaAwareRegistry`, `EmptyDateRegistry`, `FailingDescribeRegistry`, `MockRegistry`, `NoopRegistry`, `MockUpgradeRegistry`.

## Goals / Non-Goals

**Goals:**
- Clear test file naming that communicates test type at a glance
- Single source of truth for mock registries
- CI jobs that map 1:1 to test categories (unit, integration, e2e)
- Zero `#[ignore]` attributes — test targeting via `--test` flag

**Non-Goals:**
- Changing test assertions or test logic
- Modifying application code in `src/`
- Adding new tests (only restructuring existing ones)
- Creating a separate test-support crate

## Decisions

### 1. File naming convention: `integ_` and `e2e_` prefixes

All test files get a prefix indicating their type. This enables cargo `--test` targeting by pattern and makes the test type obvious from the filename.

**Alternative**: Subdirectories (`tests/integ/`, `tests/e2e/`). Rejected because cargo treats subdirectories of `tests/` as modules, not test crates — would require a single entry point file per directory.

### 2. Shared code location: `tests/common/mod.rs` with submodules

Following the official Rust convention (The Rust Book ch11-03), shared test code lives in `tests/common/mod.rs`. Split into submodules (`registries.rs`, `setup.rs`) so each test file only imports what it uses, avoiding dead_code warnings without `#[allow(dead_code)]`.

```
tests/common/
├── mod.rs           (re-exports submodules)
├── registries.rs    (FakeRegistry, AuthRequiredRegistry, etc.)
└── setup.rs         (create_test_repo, write_workflow, manifest/lock path helpers)
```

**Alternative**: `#[allow(dead_code)]` on a single `mod.rs`. Rejected per user preference.

### 3. Registry consolidation: `FakeRegistry` as the primary mock

Current registries differ in two dimensions:
1. **SHA generation**: hash-based (`E2eRegistry`, `MockRegistry`) vs concat-based (`MockUpgradeRegistry`)
2. **Capabilities**: basic (`E2eRegistry`) vs SHA-aware (`ShaAwareRegistry`, `MockRegistry`) vs failure modes (`EmptyDateRegistry`, `FailingDescribeRegistry`, `NoopRegistry`)

Consolidation:
- **`FakeRegistry`** — replaces `E2eRegistry`, `ShaAwareRegistry`, `MockRegistry`, `MockUpgradeRegistry`. Uses hash-based SHA generation. Supports `with_all_tags()` and `with_sha_tags()` builder methods. Default behavior: returns deterministic SHAs, empty tags.
- **`AuthRequiredRegistry`** — replaces `NoopRegistry`. All methods return `ResolutionError::AuthRequired`.
- **`EmptyDateRegistry`** — stays as-is (specialized: returns empty date string).
- **`FailingDescribeRegistry`** — stays as-is (specialized: `describe_sha` returns error).

`MockUpgradeRegistry` uses concat-based SHAs (`format!("{}{}", id, version)`). Tests using it will switch to `FakeRegistry` with hash-based SHAs, which means expected SHA values in assertions must be updated. This is acceptable since the SHAs are deterministic — just different values.

### 4. Test targeting via `--test` instead of `#[ignore]`

E2e tests live in `e2e_pipeline.rs` and are targeted with `cargo test --test e2e_pipeline`. No `#[ignore]` needed. Mise tasks enumerate test files explicitly.

```toml
[tasks.test]
run = "cargo test --lib"

[tasks.integ]
shell = ["bash", "-c"]
run = "cargo test $(for f in tests/integ_*.rs; do echo --test $(basename $f .rs); done)"

[tasks.e2e]
env = { GITHUB_TOKEN = "{{exec(command='gh auth token')}}" }
run = "cargo test --test e2e_pipeline"
```

### 5. E2e test scope

From `e2e_test.rs`, most tests can become real e2e tests using `GithubRegistry::new()`. Three tests must stay as integration tests because they test edge cases that require specific mock behavior:
- `test_init_sha_first_describe_sha_no_tags` — needs `E2eRegistry` (empty `describe_sha` tags)
- `test_init_sha_first_describe_sha_empty_date` — needs `EmptyDateRegistry`
- `test_init_sha_first_describe_sha_fails_falls_back_to_resolve` — needs `FailingDescribeRegistry`

These move to `integ_pipeline.rs`.

### 6. CI structure: 3 parallel jobs

```yaml
unit-tests:         mise run test       # cargo test --lib
integration-tests:  mise run integ      # cargo test --test integ_*
e2e-tests:          mise run e2e        # cargo test --test e2e_pipeline (with GITHUB_TOKEN)
```

All three jobs run in parallel.

## Risks / Trade-offs

- **SHA value changes in upgrade tests**: Switching `MockUpgradeRegistry` to `FakeRegistry` changes expected SHA values. All assertions with hardcoded SHAs need updating. → Mitigation: Use `FakeRegistry::fake_sha()` in assertions instead of hardcoded strings.
- **E2e tests depend on GitHub API**: Real e2e tests are non-deterministic (API rate limits, action versions change). → Mitigation: E2e tests use well-known, stable actions (`actions/checkout`, `actions/setup-node`). CI job has `GITHUB_TOKEN` for higher rate limits.
- **Mise task maintenance**: Adding new `integ_*.rs` files requires updating the bash glob (auto-discovery) or explicit `--test` flags. → Mitigation: The bash glob pattern auto-discovers new files.
