## Context

The command layer contains `tidy/`, `upgrade/`, `lint/`, and `init/` modules. Two files exceed the 500-line budget. The prior `code-organization` change moved business logic into the domain layer, but the tests that exercise that logic still live in `tidy/tests.rs`, inflating it to 1156 lines with 6 mock registries.

## Goals / Non-Goals

**Goals:**
- Every command-layer `.rs` file under 500 lines
- Tests live at the layer they actually test
- Command tests only exercise orchestration (calling domain methods + applying I/O)
- Domain tests exercise domain logic directly

**Non-Goals:**
- Rewriting test logic (move tests, adjust setup, don't rewrite assertions)
- Changing the tidy or upgrade public API
- Adding new test coverage

## Decisions

### 1. Split `upgrade/mod.rs` by concern

**Decision**: Split into:
- `upgrade/mod.rs`: `mod` declarations, re-exports, `Upgrade` Command impl
- `upgrade/types.rs`: `UpgradePlan`, `UpgradeScope`, `UpgradeMode`, `UpgradeRequest`, `UpgradeError`
- `upgrade/plan.rs`: `plan()`, `determine_upgrades()`, `resolve_and_store()`, `apply_upgrade_workflows()`
- `upgrade/cli.rs`: `ResolveError`, `resolve_upgrade_mode()`

**Rationale**: Each file has a single responsibility. The Command impl is a thin orchestrator calling `plan()` → `apply_*()` → `report()`. Tests follow their functions.

### 2. Push domain-logic tests from `tidy/tests.rs` to domain

**Decision**: Identify tests in `tidy/tests.rs` that test pure domain behavior and move them to the appropriate domain module's test block. Specifically:

**Tests that should move to domain:**
- Lock completeness tests (`test_lock_completeness_*`) → `domain/lock/entry.rs` tests (they test `LockEntry::is_complete` behavior)
- Override sync/prune tests (`test_plan_multiple_versions_produces_override_diff`, `test_plan_stale_override_produces_override_removal`) → `domain/manifest/overrides.rs` tests (they test `sync_overrides` / `prune_stale_overrides`)
- Manifest authority test (`test_manifest_authority_not_overwritten_by_workflow_sha`) → domain test (tests version resolution priority)

**Tests that should stay in tidy:**
- `test_tidy_records_minority_version_as_override_and_does_not_overwrite_file` — full integration test exercising the pipeline
- `test_plan_empty_workflows_returns_empty_plan` — tests `plan()` orchestration
- `test_plan_one_new_action_produces_added_entries` — tests `plan()` orchestration
- `test_plan_removed_action_produces_removed_entries` — tests `plan()` orchestration
- `test_plan_everything_in_sync_returns_empty_plan` — tests `plan()` orchestration
- `test_update_lock_recoverable_errors_are_skipped` — tests error handling orchestration
- SHA-to-tag and resolution tests — these test tidy-specific orchestration of the registry

**Rationale**: The moved tests create mock data and assert domain behavior without needing the tidy `plan()` function. They currently go through `plan()` as a shortcut, but the logic they test now lives on domain types. Moving them makes the tests simpler (no mock registry needed, just domain method calls) and shrinks `tidy/tests.rs`.

### 3. Mock registries indicate wrong test layer

**Decision**: The 6 mock `VersionRegistry` implementations in `tidy/tests.rs` (`NoopRegistry`, `TagUpgradeRegistry`, `MetadataOnlyRegistry`, `TaggedShaRegistry`, `SimpleRegistry`, `MixedRegistry`) should be evaluated during migration. Tests moved to domain won't need registries at all (domain methods are pure). Tests staying in tidy that need registries keep them.

**Rationale**: If a test needs a mock registry to exercise domain logic, it's testing at the wrong layer. Domain methods like `Manifest::sync_overrides()` take data, not registries. Only tidy's orchestration layer calls the registry.

## Target Structure

```
src/upgrade/
  mod.rs        ~80 lines   — Upgrade Command impl, mod declarations
  types.rs      ~120 lines  — UpgradePlan, UpgradeScope, UpgradeMode,
                               UpgradeRequest, UpgradeError
  plan.rs       ~280 lines  — plan(), determine_upgrades(), resolve_and_store(),
                               apply_upgrade_workflows() + tests
  cli.rs        ~70 lines   — ResolveError, resolve_upgrade_mode() + tests
  report.rs                 — unchanged

src/tidy/
  mod.rs                    — unchanged
  manifest_sync.rs          — unchanged
  lock_sync.rs              — unchanged
  patches.rs                — unchanged
  tests.rs     ~500 lines   — orchestration tests only (down from 1156)
```

## Risks / Trade-offs

- **Test migration requires rewriting setup**: Tests moving from tidy to domain will need their setup simplified — instead of building full `plan()` inputs, they call domain methods directly. This is mechanical but touches many tests.
- **Some tests are borderline**: A test like `test_sha_to_tag_upgrade_via_registry` exercises both domain resolution and tidy orchestration. Judgment call on where it belongs. When in doubt, leave in tidy.
- **Ordering dependency**: Tidy test migration depends on the domain split (Change 2) being complete, so domain modules have their test blocks ready. The upgrade split is fully independent.
