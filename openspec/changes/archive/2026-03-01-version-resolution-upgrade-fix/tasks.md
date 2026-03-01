# Tasks: Version resolution and upgrade fix

## 1. Replace `find_upgrade` + `find_latest_upgrade` with `find_upgrade_candidate`

- [x] 1.1 Add `find_upgrade_candidate(manifest_version, lock_version, candidates, allow_major) -> Option<Version>` — single function that returns actual tag names, never fabricates
- [x] 1.2 Update `determine_upgrades` in `upgrade.rs` to call `find_upgrade_candidate` with the lock version as floor
- [x] 1.3 Remove `find_upgrade` and `find_latest_upgrade` from `Version`
- [x] 1.4 Update all tests: replace `find_upgrade`/`find_latest_upgrade` test cases with `find_upgrade_candidate` tests covering safe mode, latest mode, lock floor, pre-releases, non-semver, and actual-tag-returned assertions

**Files**: `crates/gx-lib/src/domain/action.rs`, `crates/gx-lib/src/commands/upgrade.rs`

## 2. Add `version` and `specifier` fields to lock entries

- [x] 2.1 Add `version: Option<String>` and `specifier: Option<String>` fields to `ActionEntry` in lock file structs
- [x] 2.2 Update `ActionEntry` serialization to write `version` and `specifier` fields (skip if None for backward compat)
- [x] 2.3 Update `ActionEntry` deserialization to accept missing `version` and `specifier` fields
- [x] 2.4 Bump lock file version constant to `"1.3"`
- [x] 2.5 Add roundtrip test for new fields

**Files**: `crates/gx-lib/src/infrastructure/lock_file.rs`

## 3. Add specifier derivation from manifest version

- [x] 3.1 Add `specifier(&self) -> Option<String>` method to `Version` — returns `"^4"`, `"^4.2"`, or `"~4.1.0"` based on precision
- [x] 3.2 Add unit tests for specifier derivation (Major → ^, Minor → ^, Patch → ~, non-semver → None)

**Files**: `crates/gx-lib/src/domain/action.rs`

## 4. Add SHA-to-version resolution in registry

- [x] 4.1 Add `resolve_version_for_sha(owner_repo, sha, tags) -> Option<Version>` — given a SHA and a list of tags, find the most specific tag pointing to that SHA
- [x] 4.2 This requires resolving each candidate tag to its SHA and comparing — implement efficiently (can reuse tag data from `get_version_tags`)
- [x] 4.3 Add to `VersionRegistry` trait or as a method on `GithubRegistry`
- [x] 4.4 Add unit tests with mock registry

**Files**: `crates/gx-lib/src/infrastructure/github.rs`, `crates/gx-lib/src/domain/resolution.rs`

## 5. Update tidy to populate lock version and specifier

- [x] 5.1 Update `update_lock` in `tidy.rs` to fetch tag list and resolve precise version via SHA matching
- [x] 5.2 Compute specifier from manifest version and store in lock entry
- [x] 5.3 Update `ResolvedAction` to carry `version` and `specifier` fields
- [x] 5.4 Update `Lock::set()` to store `version` and `specifier`
- [x] 5.5 Update tidy tests with new lock entry assertions

**Files**: `crates/gx-lib/src/commands/tidy.rs`, `crates/gx-lib/src/domain/resolution.rs`, `crates/gx-lib/src/domain/action.rs`

## 6. Update upgrade to pass lock version to candidate selection

- [x] 6.1 In `determine_upgrades`, read the lock entry's version for each action being upgraded
- [x] 6.2 Pass lock version to `find_upgrade_candidate` as the floor
- [x] 6.3 Update upgrade tests to cover lock-floor scenarios

**Files**: `crates/gx-lib/src/commands/upgrade.rs`

## 7. Verification

- [x] 7.1 Run `cargo test` — all tests pass
- [x] 7.2 Run `cargo clippy` — no warnings
- [x] 7.3 Manual test: `GITHUB_TOKEN=$(gh auth token) cargo run -- tidy` — verify lock entries have `version` and `specifier` fields
- [x] 7.4 Manual test: `GITHUB_TOKEN=$(gh auth token) cargo run -- upgrade --latest` — verify no tag fabrication, actual tags returned
