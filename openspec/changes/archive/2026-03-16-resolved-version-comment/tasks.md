# Tasks: Use resolved version as workflow comment

## 1. Delete VersionComment type
- [x] Delete `VersionComment` struct and all impls from `domain/action/identity.rs` (lines 248-275)
- [x] Remove `VersionComment` from all import statements across the codebase

## 2. Remove comment from Resolution
- [x] Delete `comment: VersionComment` field from `Resolution` in `domain/lock/resolution.rs`
- [x] Update `Resolution` construction in `infra/lock/format.rs` (read path, line 77)
- [x] Remove `comment` TOML insertion in `infra/lock/format.rs` (write path, line 144)
- [x] Remove `comment` field from `ResolutionEntryData` serde struct in `infra/lock/format.rs`
- [x] Update `Resolution` construction in `infra/lock/migration.rs` (line 79)

## 3. Remove comment from Specifier::Range variant
- [x] Delete `comment: VersionComment` field from `Range` variant in `domain/action/specifier.rs`
- [x] Remove `comment` initialization in `parse()` and `from_v1()`
- [x] Rename `to_comment()` → `to_lookup_tag()` (line 84)
- [x] Update all callers of `to_comment()` → `to_lookup_tag()`:
  - `domain/resolution.rs:123`
  - `domain/lock/mod.rs:100` (removed in next step)
  - `tidy/lock_sync.rs:106,121`
  - `upgrade/plan.rs:61,81`

## 4. Simplify Lock methods
- [x] `set()`: Remove `comment` parameter, construct `Resolution { version }` without comment
- [x] `set_resolved()`: Remove comment derivation, rename local variable
- [x] `set_comment()`: Delete entirely (lines 143-148)
- [x] `is_complete()`: Remove the `comment_ok` block (lines 98-105)
- [x] `build_update_map()`: Change `res.comment` → `res.version` (line 177)

## 5. Delete Resolved::to_workflow_ref
- [x] Delete `to_workflow_ref()` method from `domain/action/resolved.rs` (lines 47-52)
- [x] Delete `resolved_action_to_workflow_ref` test (lines 70-83)

## 6. Delete CrossRange::new_comment
- [x] Remove `new_comment: String` field from `CrossRange` in `domain/action/upgrade.rs`
- [x] Remove `new_comment` from all test data constructing `CrossRange`
- [x] Update `upgrade/plan.rs` match arms that destructure `new_comment`

## 7. Update tidy command
- [x] `tidy/lock_sync.rs`: Remove `VersionComment` construction, remove `comment` from `lock.set()` calls, delete `lock.set_comment()` block
- [x] `tidy/patches.rs`: Change `res.comment` → `res.version` (line 70)

## 8. Update tests
- [x] `domain/lock/tests.rs`: Remove `comment` from helper, delete `is_complete_stale_comment` test, update `lock.set()` calls
- [x] `infra/lock/tests.rs`: Remove `comment` from `Resolution` construction and assertions
- [x] `domain/action/specifier.rs` tests: Update references to `to_comment()` and `comment` field

## 9. Verify
- [x] `cargo check` passes
- [x] `cargo clippy` passes
- [x] `cargo test` passes
- [x] Lock file roundtrip works (existing `comment` fields silently dropped)
