# Tasks: Semver Specifiers

## Phase 1: Domain — Specifier type

- [x] **1.1** Create `Specifier` enum in `domain/action/identity.rs` with `Range`, `Ref`, `Sha` variants
  - `Range` wraps `semver::VersionReq`, stores `raw` string and `comment` string
  - Implement `parse()`, `matches()`, `to_comment()`, `operator()`, `precision()`
  - Implement `Display`, `Clone`, `PartialEq`, `Eq`, `Hash`
  - Unit tests for parsing `"^6"`, `"~1.15.2"`, `"^0.5"`, `"main"`, SHA strings

- [x] **1.2** Add migration constructor: `Specifier::from_v1(v: &str)` that converts `"v6"` → `Specifier::Range("^6")` using existing `Version::specifier()` logic

- [x] **1.3** Replace `Version` with `Specifier` in `ActionSpec`, `ActionOverride`, `LockKey`
  - Update `domain/action/spec.rs`, `domain/manifest.rs`, `domain/lock.rs`
  - `Manifest.get()` returns `&Specifier`, `Manifest.set()` takes `Specifier`
  - `LockKey` uses `Specifier` instead of `Version`
  - Fix all compilation errors in domain layer

- [x] **1.4** Update `ResolvedAction` to carry `Specifier` instead of `Version` for the manifest specifier
  - `to_workflow_ref()` uses lock entry `comment` (deferred to Phase 3 integration)
  - Update `From<&ResolvedAction> for LockKey`

## Phase 2: Domain — Upgrade rearchitecture

- [x] **2.1** Update `UpgradeAction::CrossRange` to produce `new_specifier: Specifier` + `new_comment: String`
  - Update `UpgradeCandidate.manifest_version()` → `manifest_specifier()`
  - Update `extract_at_precision` to produce specifier + comment (preserving operator)

- [x] **2.2** Refactor `find_upgrade_candidate` to take `&Specifier` instead of `&Version`
  - Replace hand-rolled `is_in_range` logic with `specifier.matches(&candidate_semver)`
  - Preserve operator from original specifier for cross-range output
  - Update all existing upgrade tests

- [x] **2.3** Update `upgrade/mod.rs` to use new types
  - `planned_manifest.set()` takes `Specifier`
  - `resolve_and_store` uses specifier for lock key
  - `diff_manifests` compares specifiers

## Phase 3: Infrastructure — Lock file v1.4

- [x] **3.1** Update `LockEntry`: drop `specifier` field, add `comment: String` field
  - Update `domain/lock.rs` struct and constructor
  - `is_complete()` checks `comment` instead of `specifier`
  - Update `LockEntryPatch` for diff-based updates

- [x] **3.2** Update `infra/lock.rs` serializer to write v1.4 format
  - Version string `"1.4"`
  - Lock key uses `Specifier` display (e.g., `"actions/checkout@^6"`)
  - Six fields: `sha`, `version`, `comment`, `repository`, `ref_type`, `date`
  - Update `serialize_lock`, `build_lock_inline_table`, `create_lock`

- [x] **3.3** Update `infra/lock.rs` parser with version-dispatched migration
  - Read version field first, dispatch to parser for that version
  - v1.0: existing `migrate_v1` → adapt to produce `comment` field
  - v1.3: new `migrate_v1_3` — rekey `@v6` → `@^6`, derive `comment` from old key, drop `specifier`
  - v1.4: direct parse
  - Unknown future version: hard error
  - Return `Parsed<Lock> { value, migrated }`
  - Remove auto-rewrite side effect from parser (pure function)

## Phase 4: Infrastructure — Manifest v2

- [x] **4.1** Add `[gx]` section to manifest TOML wire types
  - Optional `GxSection { min_version: String }`
  - Detect v1 format by absence of `[gx]` section

- [x] **4.2** Update manifest parser to read both v1 and v2 formats
  - v1: convert `"v6"` → `Specifier::from_v1("v6")` for global and override entries
  - v2: parse `"^6"` → `Specifier::parse("^6")` directly
  - Return `Parsed<Manifest> { value, migrated }`
  - Version guard: error if binary version < `min_version`

- [x] **4.3** Update manifest serializer to write v2 format
  - Always write `[gx]\nmin_version = "<gx_version>"`
  - Write specifier strings as values
  - Override `version` field contains specifier string
  - Update `format_manifest_toml`, `create_manifest`, `apply_manifest_diff`

## Phase 5: Integration — Tidy & Init

- [x] **5.1** Update workflow scanning to produce `Specifier` from comments
  - `UsesRef::interpret()` → produce `Specifier` (via `Specifier::from_v1()` on the comment)
  - `sync_manifest_actions` stores `Specifier` in manifest

- [x] **5.2** Update `populate_lock_entry` to store `comment` in lock
  - Comment comes from the specifier's `to_comment()` for new entries
  - Comment preserved from existing lock entry for in-range updates

- [x] **5.3** Update `build_file_update_map` to use lock `comment` for workflow output
  - `format!("{} # {}", entry.sha, entry.comment)` instead of `format!("{} # {}", entry.sha, version)`

- [x] **5.4** Wire migration signaling in tidy/init command handlers
  - Print `migrated gx.toml → semver specifiers` when `manifest.migrated`
  - Print `migrated gx.lock → v1.4` when `lock.migrated`

## Phase 6: Integration — Upgrade

- [x] **6.1** Update `determine_upgrades` to pass `Specifier` to `find_upgrade_candidate`
  - Lock version lookup unchanged (still `Version` from `LockEntry.version`)

- [x] **6.2** Update upgrade plan execution to use new `CrossRange` fields
  - `planned_manifest.set(id, new_specifier)`
  - Lock entry gets `new_comment`
  - Wire migration signaling in upgrade command handler

## Phase 7: Tests & Specs

- [x] **7.1** Update existing lock-format spec (`openspec/specs/lock-format/spec.md`)
  - Document v1.4 format, comment field, migration from v1.3

- [x] **7.2** Update existing manifest-authority spec (`openspec/specs/manifest-authority/spec.md`)
  - Scenarios use specifier values instead of v-prefixed versions

- [x] **7.3** Update all integration tests
  - `tests/e2e_pipeline.rs` — workflow fixtures and assertions
  - `tests/integ_tidy.rs` — manifest/lock assertions
  - `tests/integ_upgrade.rs` — upgrade flow assertions

- [x] **7.4** Update all unit tests across modified modules
  - `domain/action/identity.rs`, `domain/lock.rs`, `domain/manifest.rs`
  - `domain/action/upgrade.rs`, `domain/action/resolved.rs`
  - `infra/lock.rs`, `infra/manifest.rs`

- [x] **7.5** Add migration-specific tests
  - v1 manifest → v2 roundtrip
  - v1.3 lock → v1.4 roundtrip
  - v1.0 lock → v1.4 migration
  - Mixed format detection and error cases
  - Version guard (`min_version`) rejection
