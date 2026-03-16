# Design: Use resolved version as workflow comment

## Approach

Straightforward field and method deletions with no architectural decisions needed. Every change follows mechanically from the proposal.

## Changes by module

### domain/action/identity.rs
- Delete `VersionComment` struct and all trait impls (lines 248-275).

### domain/action/specifier.rs
- Delete `comment: VersionComment` field from `Range` variant (line 17).
- Remove `comment` initialization in `parse()` (line 38).
- Rename `to_comment()` → `to_lookup_tag()` (line 84). Body unchanged — still returns the version prefix string.
- Update `from_v1()` to not construct `comment` field.

### domain/action/resolved.rs
- Delete `to_workflow_ref()` method (lines 47-52) and its test (lines 70-83).

### domain/action/upgrade.rs
- Delete `new_comment: String` field from `CrossRange` variant (line 19).
- Remove `new_comment` from all test data constructing `CrossRange`.

### domain/lock/resolution.rs
- Delete `comment: VersionComment` field from `Resolution` struct.
- Update import to remove `VersionComment`.

### domain/lock/mod.rs
- `set()`: Remove `comment: VersionComment` parameter. Construct `Resolution { version }` without comment.
- `set_resolved()`: Remove `comment` derivation. The `to_comment()` call at line 76 becomes `to_lookup_tag()` — but it's only used for the `Version::from()` on the next line, not for a comment. Rename the local variable for clarity.
- `set_comment()`: Delete entirely (lines 143-148).
- `is_complete()`: Remove the `comment_ok` block (lines 98-105).
- `build_update_map()`: Change `res.comment` → `res.version` at line 177 (this method is deleted in the next proposal, but must compile in the interim).

### tidy/lock_sync.rs
- `populate_lock_entry()`: Remove `VersionComment` construction and `comment` parameter from `lock.set()` calls. Delete the `lock.set_comment()` call block.
- Rename `to_comment()` calls → `to_lookup_tag()`.

### tidy/patches.rs
- `build_file_update_map()`: Change `res.comment` → `res.version` at line 70.

### upgrade/plan.rs
- Remove `VersionComment` import.
- Rename `to_comment()` → `to_lookup_tag()` at lines 61, 81.
- Remove `new_comment` destructuring from `CrossRange` match arm.
- Change `resolution.comment` → `resolution.version` at line 268.

### infra/lock/format.rs
- `ResolutionEntryData`: Remove `comment: String` field (line 22).
- Write path (line 144): Remove `comment` insertion into TOML table.
- Read path (line 77): Remove `comment` from `Resolution` construction.

### infra/lock/migration.rs
- Remove `VersionComment` from `Resolution` construction during flat→two-tier migration (line 79).

### Tests
- `domain/lock/tests.rs`: Remove `comment` from helper, delete `is_complete_stale_comment` test, update all `lock.set()` calls.
- `infra/lock/tests.rs`: Remove `comment` from `Resolution` construction and assertions.
- `domain/action/specifier.rs` tests: Update any tests referencing `to_comment()` or `comment` field.

## Migration

Lock files with existing `comment` fields parse fine — serde ignores unknown fields. On next write, the `comment` field is dropped silently.
