# Tasks: Fix upgrade and lock completeness

## 1. Update specs

- [x] 1.1 Update `version-resolution/spec.md`: change "Latest includes pre-releases" scenario to Renovate-like behavior. Add scenarios: stable manifest excludes pre-releases, pre-release manifest prefers stable, pre-release manifest falls back to newer pre-release.
- [x] 1.2 Update `lock-format/spec.md`: add scenario for version fallback to manifest version when no more-specific tag exists. Clarify that version/specifier are always present (not conditional). Add pre-release specifier scenario.

**Files**: `openspec/specs/version-resolution/spec.md`, `openspec/specs/lock-format/spec.md`

## 2. Fix `Version::precision()` for pre-releases

- [x] 2.1 Strip pre-release suffix (split on `-`, take first part) before counting components in `precision()`
- [x] 2.2 Add tests: `v3.0.0-beta.2` → Patch, `v3.0-rc.1` → Minor, `v3-alpha` → Major
- [x] 2.3 Verify `specifier()` works correctly with pre-release versions (uses full string, not just base)

**Files**: `crates/gx-lib/src/domain/action.rs`

## 3. Return `UpgradeAction` from `find_upgrade_candidate`

- [x] 3.1 Add `UpgradeAction` enum with `InRange { candidate }` and `CrossRange { candidate, new_manifest_version }` variants
- [x] 3.2 Add helper to extract precision-preserving version from candidate (e.g., `v3.0.0` at Major precision → `v3`)
- [x] 3.3 Change `find_upgrade_candidate` return type from `Option<Version>` to `Option<UpgradeAction>`, adding in-range vs cross-range detection
- [x] 3.4 Add Renovate-like pre-release handling: stable manifest filters out pre-releases, pre-release manifest includes both but prefers stable via custom comparator
- [x] 3.5 Update existing tests, add tests for: in-range returns `InRange`, cross-range returns `CrossRange` with preserved precision, stable manifest filters pre-releases, pre-release manifest prefers stable, pre-release manifest falls back to newer pre-release when no stable exists

**Files**: `crates/gx-lib/src/domain/action.rs`

## 4. Update upgrade command to use `UpgradeAction`

- [x] 4.1 Update `determine_upgrades` to use new `find_upgrade_candidate` signature
- [x] 4.2 Change `UpgradeCandidate` to carry the `UpgradeAction` instead of just `upgraded: Version`
- [x] 4.3 Update `run` to conditionally update manifest: only for `CrossRange`, using `new_manifest_version`
- [x] 4.4 Update `run` to resolve using the correct version: existing manifest version for `InRange`, new manifest version for `CrossRange`
- [x] 4.5 Update tests in upgrade.rs

**Files**: `crates/gx-lib/src/commands/upgrade.rs`

## 5. Share `populate_resolved_fields` and add version fallback

- [x] 5.1 Extract `populate_resolved_fields` from `tidy.rs` to a shared location (e.g., `domain/resolution.rs`)
- [x] 5.2 Add fallback: when `tags_for_sha` fails or returns empty, set `resolved_version` to the manifest version
- [x] 5.3 Update `resolve_and_store` in upgrade.rs to call `populate_resolved_fields`
- [x] 5.4 Update tidy.rs to use the shared function

**Files**: `crates/gx-lib/src/domain/resolution.rs`, `crates/gx-lib/src/commands/upgrade.rs`, `crates/gx-lib/src/commands/tidy.rs`

## 6. Always serialize all 6 lock fields

- [x] 6.1 Update `serialize_lock` to always output `version` and `specifier`, using lock key version as fallback for version and empty string for specifier
- [x] 6.2 Update serialization tests

**Files**: `crates/gx-lib/src/infrastructure/lock.rs`

## 7. Verification

- [x] 7.1 `cargo test` — 160/161 tests pass (1 pre-existing failure in tidy_test)
- [x] 7.2 `cargo clippy` — reviewed (warnings are pre-existing)
- [x] 7.3 Manual: run `gx tidy` — verify all lock entries have version and specifier
- [x] 7.4 Manual: run `gx upgrade --latest` on a repo with floating versions — verify manifest precision is preserved, lock entries have all 6 fields, pre-releases handled correctly
