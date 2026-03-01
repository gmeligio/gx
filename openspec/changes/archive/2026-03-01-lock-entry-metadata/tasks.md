# Tasks: Rich lock entry metadata

## 1. Add domain types: RefType enum and LockEntry struct
- [x] Add `RefType` enum (`Release`, `Tag`, `Branch`, `Commit`) to `domain/action.rs` with `Display` and serde traits
- [x] Add `LockEntry` struct (`sha`, `repository`, `ref_type`, `date`) to `domain/lock.rs`
- [x] Add `ResolvedRef` struct to `domain/resolution.rs` for the registry return type
- [x] Unit tests for `RefType` serialization/deserialization and `Display`

**Files**: `crates/gx-lib/src/domain/action.rs`, `crates/gx-lib/src/domain/lock.rs`, `crates/gx-lib/src/domain/resolution.rs`, `crates/gx-lib/src/domain/mod.rs`

## 2. Update Lock domain entity to use LockEntry
- [x] Change `Lock` internal map from `HashMap<LockKey, CommitSha>` to `HashMap<LockKey, LockEntry>`
- [x] Update `Lock::new()`, `Lock::get()`, `Lock::set()`, `Lock::entries()` signatures
- [x] Update `Lock::build_update_map()` to read SHA from `LockEntry.sha`
- [x] Update all existing Lock unit tests

**Files**: `crates/gx-lib/src/domain/lock.rs`

## 3. Update ResolvedAction to carry metadata
- [x] Add `repository: String`, `ref_type: RefType`, `date: String` to `ResolvedAction`
- [x] Update `ResolvedAction::new()` constructor
- [x] Update `Lock::set()` to build `LockEntry` from the enriched `ResolvedAction`
- [x] Update all call sites constructing `ResolvedAction` (resolution.rs, tidy.rs tests, upgrade.rs tests)

**Files**: `crates/gx-lib/src/domain/action.rs`, `crates/gx-lib/src/domain/lock.rs`, `crates/gx-lib/src/domain/resolution.rs`, `crates/gx-lib/src/commands/tidy.rs`, `crates/gx-lib/src/commands/upgrade.rs`

## 4. Update VersionRegistry trait and ActionResolver
- [x] Change `VersionRegistry::lookup_sha()` return type from `Result<CommitSha, _>` to `Result<ResolvedRef, _>`
- [x] Update `ActionResolver::resolve()` to construct `ResolvedAction` from `ResolvedRef`
- [x] Update `ActionResolver::validate_and_correct()` to thread metadata through all return paths
- [x] Update `MockRegistry` in resolution.rs tests
- [x] Update `NoopRegistry` in tidy.rs tests

**Files**: `crates/gx-lib/src/domain/resolution.rs`, `crates/gx-lib/src/commands/tidy.rs`, `crates/gx-lib/src/commands/upgrade.rs`

## 5. Update integration tests for new signatures
- [x] Update `NoopRegistry` and `MockRegistry` in `tidy_test.rs` to return `ResolvedRef`
- [x] Update `MockUpgradeRegistry` in `upgrade_test.rs` to return `ResolvedRef`
- [x] Update all `ResolvedAction::new()` calls in `upgrade_test.rs` (7 call sites)
- [x] Update v1.0 lock file string in `test_upgrade_with_existing_lock_and_empty_manifest` to v2.0 format

**Files**: `crates/gx-lib/tests/tidy_test.rs`, `crates/gx-lib/tests/upgrade_test.rs`

## 6. Update lock file serialization (v2.0 format)
- [x] Replace `ActionEntry` TOML struct: `HashMap<String, String>` → `HashMap<String, ActionEntryData>` where `ActionEntryData = { sha, repository, ref_type, date }`
- [x] Update `lock_to_data()` to serialize `LockEntry` fields
- [x] Update `lock_from_data()` to deserialize `ActionEntryData` into `LockEntry`
- [x] Update serialization unit tests (basic roundtrip and TOML load)

**Files**: `crates/gx-lib/src/infrastructure/lock.rs`

## 7. Implement v1.0 → v2.0 migration
- [x] Add `LockDataV1` struct that reads old `HashMap<String, String>` format
- [x] Add `migrate_v1()` to convert v1 entries to v2 with default metadata
- [x] Detect format in `parse_lock()`: try v2.0 first, fall back to v1.0
- [x] Migration populates defaults (`base_repo()`, `ref_type = Tag`, `date = ""`)
- [x] Migration triggers automatic rewrite to current format
- [x] Fix `test_file_lock_migrates_old_version` and `test_parse_lock_reads_file`

**Files**: `crates/gx-lib/src/infrastructure/lock.rs`

## 8. Fix clippy warnings
- [x] Fix duplicate match arms in `RefType::from` (delegate `From<String>` to `From<&str>`)
- [x] Fix `if let` suggestion in `parse_lock`
- [x] Fix dead code warning on `LockDataV1.version`
- [x] Fix missing backticks in `LockEntry.date` doc comment
- [x] Suppress intentional `match_same_arms` for `RefType` default case

**Files**: `crates/gx-lib/src/domain/action.rs`, `crates/gx-lib/src/domain/lock.rs`, `crates/gx-lib/src/infrastructure/lock.rs`

## 9. Extend GithubRegistry to detect ref_type and fetch dates
- [x] Refactor `resolve_ref()` to return `(sha, ref_type)` — track which API path succeeded (tag/branch/commit)
- [x] Add `fetch_commit_date()`: `GET /repos/{repo}/commits/{sha}` → `committer.date`
- [x] Add `fetch_release_date()`: `GET /repos/{repo}/releases/tags/{tag}` → `published_at`
- [x] Add `fetch_tag_date()`: `GET /repos/{repo}/git/tags/{sha}` → `tagger.date` (for annotated tags)
- [x] Add response structs: `ReleaseResponse`, `TagObjectResponse`, `CommitDetailResponse`
- [x] Implement date priority: release `published_at` > annotated tag `tagger.date` > commit `committer.date`
- [x] Populate `repository` from `base_repo` calculation
- [x] Replace the current stub in `impl VersionRegistry for GithubRegistry` with real logic
- [x] Update existing GithubRegistry unit tests

**Files**: `crates/gx-lib/src/infrastructure/github.rs`

## 10. Keep lock file version at 1.1
- [x] Keep `LOCK_FILE_VERSION` at `"1.1"` — new fields are additive, no breaking change
- [x] Verify TOML renders entries as inline tables (not sub-tables)
- [x] Update any tests that reference the version string

**Files**: `crates/gx-lib/src/infrastructure/lock.rs`

## 11. Serialize lock entries as inline TOML tables
- [x] Replace `toml::to_string_pretty(&data)` in `FileLock::save()` with manual string building
- [x] Sort entries by key for deterministic output
- [x] Write each entry as `"key" = { sha = "...", repository = "...", ref_type = "...", date = "..." }`
- [x] Update roundtrip test to verify inline format output
- [x] Remove `Serialize` derive from `ActionEntryData` and `LockData` (no longer needed)

**Files**: `crates/gx-lib/src/infrastructure/lock.rs`

## 12. End-to-end verification
- [x] Run `cargo test` — all tests pass
- [x] Run `cargo clippy` — no warnings
- [x] Manual test: run `gx tidy` on the gx repo itself, verify lock file has all fields populated
- [x] Manual test: delete `gx.lock`, run `gx tidy`, verify fresh lock file is correct
- [x] Manual test: create a v1.0 lock file, run `gx tidy`, verify migration works
- [x] Verify lock file output uses inline tables (one line per entry under `[actions]`)
- [x] Verify lock file version remains `"1.1"`
