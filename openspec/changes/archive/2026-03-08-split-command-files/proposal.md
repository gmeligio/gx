## Why

Two command-layer files exceed the 500-line budget: `upgrade/mod.rs` (723) and `tidy/tests.rs` (1156). `upgrade/mod.rs` mixes types, planning logic, CLI parsing, and the Command impl in a single file. `tidy/tests.rs` has grown because integration tests re-test business logic that should live in domain — the 6 mock registries and complex setup fixtures are symptoms of testing through the wrong layer.

## What Changes

- **Split `upgrade/mod.rs`** into submodules: types, planning logic, CLI parsing, and the thin Command impl.
- **Shrink `tidy/tests.rs`** by pushing business logic tests down to the domain layer. Tests that exercise pure domain logic (override sync, lock completeness, manifest diff) belong as domain unit tests, not tidy integration tests. The remaining tidy tests should exercise orchestration only.

## Capabilities

### New Capabilities

_(None — purely structural.)_

### Modified Capabilities

_(No existing capabilities are changing — these are internal structural improvements.)_

## Impact

- **Command modules** (`src/upgrade/`, `src/tidy/`): Internal reorganization. Public API unchanged.
- **Domain layer**: Gains tests that were previously in tidy (testing domain logic through the tidy orchestrator). No new code — only test migration.
- **No user-facing changes**.

## Dependencies

- **Depends on `split-domain-files`**: Pushing tidy tests to domain requires the domain subdirectories to exist (e.g., `domain/manifest/overrides.rs` tests). The upgrade split is independent and can proceed in parallel.
