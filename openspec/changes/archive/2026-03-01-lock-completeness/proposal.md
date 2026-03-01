## Why

Lock entries can have missing fields (e.g., empty `specifier`) because the system only populates metadata when creating **new** entries. Pre-existing entries are skipped entirely via a binary `lock.has()` check. This means adding a new field to the lock schema requires manual migration — the system never self-heals. The current code also tangles three distinct operations (SHA resolution, version refinement, specifier derivation) into `populate_resolved_fields`, making the data flow hard to follow and extend.

## What Changes

- **BREAKING**: Remove `specifier` and `resolved_version` from `ResolvedAction`. Resolution returns only what the network gives: SHA, repository, ref_type, date.
- **BREAKING**: Remove `populate_resolved_fields`. Replace with three explicit operations: RESOLVE (network: version → SHA + metadata), REFINE (network: SHA → most specific version tag), DERIVE (local: manifest version → specifier).
- Add `is_complete()` to `LockEntry` so entries can self-report missing fields.
- Replace the binary "exists or skip" lock update logic with field-level completeness checks. Incomplete entries get only the operations they need (e.g., missing specifier triggers DERIVE only — no network call).
- Restructure tidy's `update_lock` into two phases: Phase 1 corrects the manifest (SHA correction uses REFINE), Phase 2 ensures lock completeness (runs RESOLVE/REFINE/DERIVE as needed per entry).
- Unify SHA correction and version refinement — both are `tags_for_sha` applied to different targets (manifest vs lock).

## Capabilities

### New Capabilities
- `lock-reconciliation`: The lock detects incomplete entries and the system fills missing fields using the minimal required operations. Future schema additions self-heal on next tidy run.

### Modified Capabilities
- `resolution-metadata`: ResolvedAction no longer carries specifier or resolved_version. Metadata flow restructured around three distinct operations instead of resolve + populate_resolved_fields.

## Impact

- `crates/gx-lib/src/domain/action.rs`: `ResolvedAction` struct simplified, `populate_resolved_fields` removed from `resolution.rs`
- `crates/gx-lib/src/domain/lock.rs`: `LockEntry` gains `is_complete()`
- `crates/gx-lib/src/domain/resolution.rs`: Restructured around RESOLVE/REFINE/DERIVE primitives
- `crates/gx-lib/src/commands/tidy.rs`: `update_lock` rewritten with two-phase flow
- Tests across all affected modules need updating
