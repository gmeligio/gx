# Tasks: Lazy Pipeline Architecture

Every step follows TDD: write/adapt tests first, then implement until tests pass. Existing tests in `tests/tidy_test.rs` (17), `tests/lint_test.rs` (11), `tests/upgrade_test.rs` (16), and domain unit tests are the safety net — they must pass at every step.

## Phase 1: Domain Accessors Return Iterators

### Step 1: Manifest::specs() returns iterator

- [x] Change `Manifest::specs()` from `Vec<&ActionSpec>` to `impl Iterator<Item = &ActionSpec>`
- [x] Update all callers in `tidy.rs` (lines 83, 138, 235, 389, 408) — remove `.iter()`, adapt collection
- [x] Update caller in `upgrade.rs` (line 170) — adapt `.retain()` pattern
- [x] Update caller in `lint/unsynced_manifest.rs` (line 23) — adapt to iterator
- [x] Adapt domain unit tests in `domain/manifest.rs` if they assert on `Vec` return type
- [x] Run `cargo test` — all existing tests pass (no behavioral change)

### Step 2: WorkflowActionSet returns iterators

- [x] Change `action_ids()` from `Vec<ActionId>` to `impl Iterator<Item = &ActionId>`
- [x] Change `versions_for()` from `Vec<Version>` to `impl Iterator<Item = &Version>`
- [x] Update caller in `tidy.rs` (line 81) — collect directly to `HashSet<&ActionId>`
- [x] Update caller in `lint/unsynced_manifest.rs` (line 21) — collect directly to `HashSet`
- [x] Update callers of `versions_for()` — adapt to iterator
- [x] Adapt unit tests in `domain/workflow_actions.rs` if they assert on `Vec` return type
- [x] Run `cargo test` — all existing tests pass

### Step 3: Remove collect-then-re-iterate anti-patterns

- [x] `tidy.rs` sync_manifest_actions: use iterator chains instead of collecting to Vec then iterating
- [x] `tidy.rs` upgrade_sha_versions_to_tags: iterate specs directly, no intermediate Vec clone
- [x] `tidy.rs` update_lock: remove double-collection of specs (lines 389 vs 408)
- [x] `tidy.rs` build_keys_to_retain: chain iterators instead of collecting then pushing
- [x] `tidy.rs` prune_stale_overrides: iterate override keys directly, no `.keys().cloned().collect()`
- [x] `upgrade.rs` line 141-147: use `.chain()` or `.extend()` instead of collect + push loop
- [x] Run `cargo test` — all 46+ existing tests pass (pure refactor, no behavioral change)
- [x] Run `cargo clippy` — no warnings

## Phase 2: Iterator-Based Scanner

### Step 4: Scanner trait returns iterator

- [x] Add `scan()` method to `WorkflowScanner` returning `Box<dyn Iterator<Item = Result<LocatedAction, WorkflowError>> + '_>`
- [x] Add `scan_paths()` method returning `Box<dyn Iterator<Item = Result<PathBuf, WorkflowError>> + '_>`
- [x] Implement default `scan_all_located()` that collects from `scan()` — existing callers unaffected
- [x] Implement both on `FileWorkflowScanner` — parse one file at a time, yield actions per file
- [x] Write tests in `infrastructure/workflow.rs`: scanner yields same actions as `scan_all_located()` but per-file
- [x] Write test: scanner yields error for a single malformed file without aborting other files
- [x] Update `NoopRegistry` and `MockRegistry` in test files if scanner mock needs `scan()`
- [x] Run `cargo test` — all existing tests pass + new scanner tests pass

### Step 5: Tidy uses iterator scanner

- [x] Change `tidy::run()` to call `scanner.scan()` and collect into `WorkflowActionSet` directly
- [x] Remove intermediate `Vec<LocatedAction>` — build action set and located-actions collection in one pass
- [x] Run `cargo test` — all 17 tidy integration tests pass (same behavior, different internal path)

### Step 6: Lint uses iterator scanner for file-local rules

- [x] Write test: lint detects unpinned action when scanning file-by-file (same result as current)
- [x] Write test: lint detects sha-mismatch when scanning file-by-file
- [x] Write test: lint detects unsynced manifest (global rule) after full scan
- [x] Refactor lint to scan files via iterator, run file-local rules per-action
- [x] Collect into `WorkflowActionSet` incrementally during scan
- [x] Run global rules (unsynced_manifest) after scan completes
- [x] Update lint function signature — take `&dyn WorkflowScanner` instead of `&[LocatedAction]`
- [x] Update `app.rs` lint dispatcher accordingly
- [x] Run `cargo test` — all 11 lint integration tests pass + new tests pass

## Phase 3: Plan Types

### Step 7: Define plan/diff types

- [x] Create `src/domain/plan.rs` with: `ManifestDiff`, `LockDiff`, `LockEntryPatch`, `WorkflowPatch`, `TidyPlan`, `UpgradePlan`
- [x] Implement `is_empty()` on all diff types and plan types
- [x] Implement `Debug` on plan types
- [x] Write unit tests: `ManifestDiff::is_empty()` returns true for default, false after adding an entry
- [x] Write unit tests: `TidyPlan::is_empty()` is true only when all sub-diffs are empty
- [x] Export from `domain/mod.rs`
- [x] Run `cargo test` — all existing tests pass + new type tests pass

### Step 8: Tidy produces TidyPlan

- [x] Write test: tidy plan for empty workflows returns empty plan
- [x] Write test: tidy plan with one new action produces `ManifestDiff.added` with that action + `LockDiff.added` with resolved entry
- [x] Write test: tidy plan with one removed action produces `ManifestDiff.removed` with that action + `LockDiff.removed`
- [x] Write test: tidy plan with multiple versions of same action produces override diff entries
- [x] Write test: tidy plan with stale override produces override removal
- [x] Write test: tidy plan when everything in sync returns empty plan (no-op)
- [x] Create `tidy::plan()` function: `fn plan<R, P>(&Manifest, &Lock, &R, &P) -> Result<TidyPlan, TidyError>`
- [x] Extract manifest diff logic from `sync_manifest_actions` — produce `ManifestDiff` instead of mutating
- [x] Extract override diff logic from `sync_overrides` / `prune_stale_overrides` — produce override diffs
- [x] Extract lock diff logic from `update_lock` — produce `LockDiff` with resolved entries for new specs only
- [x] Extract workflow pin logic from `build_file_update_map` — produce `Vec<WorkflowPatch>`
- [x] Wire plan phases together into `tidy::plan()`
- [x] Keep old `tidy::run()` temporarily — existing integration tests continue to pass on old path
- [x] Run `cargo test` — all existing tests pass + new plan tests pass

### Step 9: Upgrade produces UpgradePlan

- [x] Write test: upgrade plan with no upgradable actions returns empty plan
- [x] Write test: upgrade plan with one upgradable action produces correct manifest + lock diffs
- [x] Write test: upgrade plan in latest mode produces major version bump diff
- [x] Create `upgrade::plan()`: `fn plan<R>(&Manifest, &Lock, &R, &UpgradeRequest) -> Result<UpgradePlan, UpgradeError>`
- [x] Extract manifest and lock changes from `determine_upgrades` into diff types
- [x] Keep old `upgrade::run()` temporarily
- [x] Run `cargo test` — all existing tests pass + new plan tests pass

## Phase 4: Surgical Apply via toml_edit

### Step 10: Add toml_edit dependency

- [x] Add `toml_edit` to `Cargo.toml` under `[dependencies]`
- [x] Run `cargo check` — compiles

### Step 11: Implement apply_manifest_diff

- [x] Write test: empty diff does not modify file (read before, read after, content identical)
- [x] Write test: adding one action to existing manifest inserts the entry, preserves existing entries byte-for-byte
- [x] Write test: removing one action from manifest removes only that entry
- [x] Write test: adding an override creates `[actions.overrides]` section if missing
- [x] Write test: adding an override to existing overrides section appends correctly
- [x] Write test: removing all overrides for an action removes the action's override entry
- [x] Write test: removing last override removes `[actions.overrides]` section
- [x] Write test: round-trip — apply diff then parse result back, domain state matches expectations
- [x] Implement `apply_manifest_diff(path: &Path, diff: &ManifestDiff) -> Result<(), ManifestError>` in `infrastructure/manifest.rs`
- [x] Run `cargo test` — all tests pass

### Step 12: Implement apply_lock_diff

- [x] Write test: empty diff does not modify file
- [x] Write test: adding one lock entry inserts inline table with all 6 fields
- [x] Write test: removing one lock entry removes only that key
- [x] Write test: updating version field on existing entry patches only that field
- [x] Write test: updating specifier field on existing entry patches only that field
- [x] Write test: round-trip — apply diff then parse result back, domain state matches
- [x] Implement `apply_lock_diff(path: &Path, diff: &LockDiff) -> Result<(), LockFileError>` in `infrastructure/lock.rs`
- [x] Run `cargo test` — all tests pass

### Step 13: Implement create from scratch (for init)

- [x] Write test: create manifest from diff with 3 actions produces valid parseable TOML with `[actions]` header
- [x] Write test: create manifest with actions + overrides produces both sections
- [x] Write test: create lock from diff with 3 entries produces valid parseable TOML with version header and `[actions]` section
- [x] Write test: round-trip — create file, parse it back with existing `parse_manifest()`/`parse_lock()`, domain state matches
- [x] Implement `create_manifest(path: &Path, diff: &ManifestDiff) -> Result<(), ManifestError>`
- [x] Implement `create_lock(path: &Path, diff: &LockDiff) -> Result<(), LockFileError>`
- [x] Run `cargo test` — all tests pass

## Phase 5: Wire Everything Together

### Step 14: Tidy dispatcher uses plan + apply

- [x] Write integration test: tidy on synced repo produces no file modifications (assert file mtimes or content unchanged)
- [x] Write integration test: tidy with one new action modifies only the relevant entries in manifest/lock
- [x] Migrate existing `tidy_test.rs` integration tests to use plan+apply path (or verify they pass as-is if dispatcher handles the switch)
- [x] Change `app::tidy()` to call `tidy::plan()` then `apply_manifest_diff()` + `apply_lock_diff()` + `apply_workflow_patches()`
- [x] Short-circuit: if `plan.is_empty()`, skip all writes
- [x] Run `cargo test` — all 17 tidy integration tests pass on new path
- [x] Remove old `tidy::run()` function
- [x] Run `cargo test` — all tests still pass after removal

### Step 15: Init dispatcher uses plan + create

- [x] Migrate init-related tests from `tidy_test.rs` to use create path
- [x] Change `app::init()` to call `tidy::plan()` then `create_manifest()` + `create_lock()` + `apply_workflow_patches()`
- [x] Run `cargo test` — all init-related tests pass

### Step 16: Upgrade dispatcher uses plan + apply

- [x] Migrate `upgrade_test.rs` integration tests to use plan+apply path
- [x] Change `app::upgrade()` to call `upgrade::plan()` then apply functions
- [x] Run `cargo test` — all 16 upgrade integration tests pass on new path
- [x] Remove old `upgrade::run()` function
- [x] Run `cargo test` — all tests still pass after removal

### Step 17: Remove dead code

- N/A `Manifest::set()`, `remove()`, `add_override()`, `replace_overrides()` — still used by `plan()` (clone-and-mutate)
- N/A `Lock::set()`, `set_version()`, `set_specifier()`, `retain()` — still used by `plan()` (clone-and-mutate)
- N/A `format_manifest_toml()`, `manifest_to_data()`, `ManifestData` — still used by `create_manifest()` and `parse_manifest()`
- N/A `serialize_lock()` — still used by `FileLock::save()` (lock migration path)
- N/A `FileManifest::save()`, `FileLock::save()` — save used in tests and lock migration
- N/A Domain unit tests — still test actively-used mutation API (used by plan functions)
- [x] Remove `FileManifest` and `FileLock` from public API re-exports (now crate-internal)
- [x] Remove unused `path()` methods from `FileManifest` and `FileLock`
- [x] Replace `FileLock::save()` usage in tidy test with `create_lock()`
- [x] Run `cargo test` — all 282 tests pass
- [x] Run `cargo clippy` — no warnings

## Phase 6: End-to-End Regression Tests

### Step 18: Automated end-to-end tests

- [x] Write integration test: `init` on fresh repo creates parseable manifest and lock, workflow pins match lock SHAs
- [x] Write integration test: `tidy` immediately after `init` is a no-op (file contents unchanged)
- [x] Write integration test: `tidy` after adding a new action to a workflow adds only that action to manifest/lock
- [x] Write integration test: `tidy` after removing an action from all workflows removes only that action from manifest/lock
- [x] Write integration test: `tidy` with override changes patches only the overrides section
- [x] Write integration test: `upgrade` patches only upgraded entries in manifest/lock, preserves the rest
- [x] Write integration test: `lint` detects unsynced-manifest after workflow changes
- [x] Write integration test: sequential `init` → `tidy` → modify workflow → `tidy` → `upgrade` produces correct final state
- [x] Run full suite: `cargo test` — all 290 tests pass
- [x] Run `cargo clippy` — no warnings
