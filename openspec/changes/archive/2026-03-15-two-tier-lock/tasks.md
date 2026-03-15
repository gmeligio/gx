# Tasks: Two-Tier Lock File

## Phase 1: Domain model

### Task 1.1: Add `Resolution` type and `ActionKey`
- [x] Add `Resolution { version: Version, comment: String }` in `src/domain/lock/resolution.rs`
- [x] Add `ActionKey { id: ActionId, version: Version }` with `Hash`, `Eq` derives
- [x] Add `Hash`, `Eq` derives to `Version` if not already present
- [x] **Verify**: `cargo check` passes

### Task 1.2a: Restructure `Lock` core (struct, new, get, has, set)
- [x] Change `Lock` from `HashMap<Spec, Entry>` to `resolutions: HashMap<Spec, Resolution>` + `actions: HashMap<ActionKey, Commit>`
- [x] Update `Lock::new()` to accept both maps
- [x] Update `Lock::get(spec)` to two-step lookup returning `Option<(&Resolution, &Commit)>`
- [x] Update `Lock::has(spec)` to check resolutions map
- [x] Update `Lock::set()` signature to accept `spec`, `version`, `commit`, `comment`
- [x] Remove `Lock::set_version()` and `Lock::set_comment()` (no longer needed)
- [x] Add `Lock::is_complete(spec)` that checks both tiers
- [x] Update `Lock::is_empty()`
- [x] **Verify**: `cargo check` passes

### Task 1.2b: Restructure `Lock` operations (retain, cleanup, update_map)
- [x] Update `Lock::retain()` to prune resolutions only
- [x] Add `Lock::cleanup_orphans()` to prune unreferenced action entries
- [x] Update `Lock::build_update_map()` to read comment from resolution
- [x] Update `Lock::entries()` / iteration patterns
- [x] **Verify**: `cargo check` passes

### Task 1.2c: Restructure `Lock::diff()` and tests
- [x] Update `Lock::diff()` to compare resolutions tier with SHA lookups from actions tier
- [x] Update all `Lock` tests for two-tier structure
- [x] **Verify**: `cargo check` passes

### Task 1.3: Remove `Entry`, replace with `Commit`
- [x] Remove `Entry` struct entirely (its fields minus `comment` and `version` are exactly `Commit`)
- [x] Use `Commit` as the action map value type (already done in 1.2a)
- [x] Remove `entry.rs` or repurpose for `Resolution`
- [x] Delete old `Entry` tests (replaced by `Lock` tests in 1.2c)
- [x] **Verify**: `cargo check` passes

Note: `is_complete` is now `Lock::is_complete(spec)` and was added in Task 1.2a.

## Phase 2: Consumer updates

### Task 2.1: Update `tidy/lock_sync.rs`
- [x] Change `populate_lock_entry` to pass resolved version, commit, and comment to `Lock::set()`
- [x] Remove the proxy `ResolvedAction` pattern (no longer needed — spec and resolved version are separate)
- [x] Remove separate `set_version` / `set_comment` calls
- [x] Update completeness check to use two-tier `is_complete`
- [x] Update all `lock_sync` tests
- [x] **Verify**: `cargo test --lib` passes

### Task 2.2: Update `upgrade/plan.rs`
- [x] Change `lock.set(resolved)` calls to new signature
- [x] Change `lock.get(spec)` usages to handle `(&Resolution, &Commit)` return
- [x] Update `lock.set_comment()` calls (now part of `set()`)
- [x] Update lock version floor lookup (reads `resolution.version` instead of `entry.version`)
- [x] **Verify**: `cargo test --lib` passes

### Task 2.3: Update lint modules
- [x] `lint/sha_mismatch.rs`: access SHA via `commit.sha` from two-step lookup
- [x] `lint/stale_comment.rs`: read comment from resolution, not entry
- [x] `lint/unpinned.rs`: adjust lock entry access
- [x] Update lint tests
- [x] **Verify**: `cargo test --lib` passes

### Task 2.4: Update `tidy/mod.rs` for orphan cleanup
- [x] Call `lock.cleanup_orphans()` after both resolution and retain operations complete
- [x] Ordering: resolve specs → retain resolutions → cleanup orphaned action entries
- [x] Update tidy tests
- [x] **Verify**: `cargo test --lib` passes

### Task 2.5: Update `LockDiff` in `domain/plan.rs`
- [x] Adjust `LockDiff` to work with two-tier: diff compares resolutions, looks up SHAs from actions
- [x] Update `LockDiff` consumers (tidy report, upgrade report)
- [x] Update diff tests
- [x] **Verify**: `cargo test --lib` passes

### Task 2.6: Update remaining consumers
- [x] `tidy/report.rs`: adjust report building for new lock API
- [x] `tidy/patches.rs`: adjust lock access patterns
- [x] `tidy/manifest_sync.rs`: adjust if it reads lock entries
- [x] `infra/workflow_update.rs`: adjust if it accesses lock directly
- [x] **Verify**: `cargo check` passes

## Phase 3: Serialization

### Task 3.1: Two-tier lock serialization
- [x] Update `build_lock_document()` to write `[resolutions]` and `[actions]` sections
- [x] Sort resolutions by action ID then specifier
- [x] Sort actions by action ID then version
- [x] Action entries have 4 fields: sha, repository, ref_type, date
- [x] Resolution entries have 2 fields: version, comment
- [x] Update serialization tests
- [x] **Verify**: `cargo test --lib` passes

### Task 3.2: Two-tier lock deserialization
- [x] Add `LockDataTwoTier` deserialization struct with `resolutions` and `actions` maps
- [x] Detect format: presence of `[resolutions]` → two-tier; `@` in action keys → flat
- [x] Parse two-tier format into domain `Lock`
- [x] Update `lock_from_data` or add parallel `lock_from_two_tier`
- [x] Update deserialization tests
- [x] **Verify**: roundtrip test passes

### Task 3.3: Flat-to-two-tier migration
- [x] Add `migrate_flat_to_two_tier()` that splits flat entries into resolutions + actions
- [x] Wire into parse chain: v1.0 → flat → two-tier, v1.3 → flat → two-tier, flat → two-tier
- [x] Set `migrated = true` when flat-to-two-tier migration occurs
- [x] Update migration tests
- [x] **Verify**: all migration test scenarios pass

## Phase 4: Validation

### Task 4.1: Full test suite
- [x] Run `cargo test` — all tests pass
- [x] Run `cargo clippy` — no warnings
- [x] Run integration tests
- [x] Run e2e tests
- [x] Verify lock file output matches expected two-tier format
