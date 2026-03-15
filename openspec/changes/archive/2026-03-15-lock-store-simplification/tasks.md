## 1. Create format module (current format read + write)

- [x] 1.1 Create `src/infra/lock/format.rs` with `TwoTierData`, `ResolutionEntryData`, `ActionCommitData` serde structs (moved from `convert.rs`)
- [x] 1.2 Add `try_parse(content: &str, path: &Path) -> Result<Option<Lock>, Error>` that checks for `[resolutions` and parses two-tier format
- [x] 1.3 Move `build_lock_document(&Lock) -> DocumentMut` and its helpers (`ensure_implicit_table`, `ref_type_to_str`, `populate_action_table`) into `format.rs` as `pub(super) fn write(lock: &Lock) -> String`
- [x] 1.4 Add roundtrip test: write a `Lock`, parse the output, verify identical

## 2. Create migration module (flat format reader)

- [x] 2.1 Create `src/infra/lock/migration.rs` with `FlatData`, `FlatEntryData` serde structs (moved from `convert.rs` `LockData`/`ActionEntryData`)
- [x] 2.2 Add `try_parse(content: &str, path: &Path) -> Result<Option<Lock>, Error>` that detects flat format (has `[actions` but no `[resolutions`) and parses it
- [x] 2.3 Add test: flat format with version field (e.g. `version = "1.4"` inline tables) parses correctly — verifies v1.4 is handled as flat
- [x] 2.4 Add test: flat format without version field parses correctly
- [x] 2.5 Add test: flat entry with missing version falls back to specifier
- [x] 2.6 Add test: two flat entries deduplicating to one action entry

## 3. Consolidate Store API

- [x] 3.1 Add `Store::load(&self) -> Result<Lock, Error>` that tries `format::try_parse` then `migration::try_parse`, returns error for unrecognized format
- [x] 3.2 Update `Store::save(&self, lock: &Lock)` to use `format::write()`
- [x] 3.3 Add `Store::load` test: file doesn't exist returns `Lock::default()`
- [x] 3.3b Add `Store::load` test: file exists but is empty returns `Lock::default()`
- [x] 3.4 Add `Store::load` test: two-tier format loads correctly
- [x] 3.5 Add `Store::load` test: flat format loads correctly
- [x] 3.6 Add `Store::load` test: unrecognized content returns error
- [x] 3.7 Delete free functions `parse()`, `create()`, `apply_lock_diff()` from `mod.rs`
- [x] 3.8 Delete `convert.rs` (all code moved to `format.rs` and `migration.rs`)
- [x] 3.9 Delete old `migration.rs` (v1.0, v1.3 migration code)

## 4. Remove Parsed<T> and migrated flag

- [x] 4.1 Remove `Parsed<T>` from `src/domain/mod.rs`
- [x] 4.2 Remove `lock_migrated` from `Config`. Keep `manifest_migrated` (manifest v1 migration still uses it for the "migrated gx.toml" message)
- [x] 4.3 Update `Config::load()` to call `Store::load()` directly (no `Parsed` wrapper)
- [x] 4.4 Remove "migrated gx.lock" messages from `tidy::Tidy::run()`, `upgrade::Upgrade::run()`, and `init::Init::run()`

## 5. Update plan() to return final Lock

- [x] 5.1 Add `lock: Lock` and rename existing `lock` to `lock_changes: LockDiff` in `tidy::Plan`
- [x] 5.2 Update `tidy::plan()` to return the planned `Lock` in the plan
- [x] 5.3 Update `tidy::Plan::is_empty()` to check `lock_changes` instead of `lock`
- [x] 5.4 Update `tidy::Tidy::run()` to use `Store::save(&tidy_plan.lock)` instead of `apply_lock_diff`/`create`
- [x] 5.5 Apply the same pattern to `upgrade::Plan` — add `lock: Lock`, rename diff to `lock_changes`
- [x] 5.6 Update `upgrade::Upgrade::run()` to use `Store::save()`
- [x] 5.7 Update `init::Init::run()` to use `Store::save()` instead of `lock::create()`

## 6. Update tests and integration

- [x] 6.1 Update `src/infra/lock/tests.rs` — remove v1.0/v1.3 migration tests, rewrite `apply_lock_diff` tests as `Store::save` tests
- [x] 6.2 Update `src/infra/lock/convert.rs` tests — move relevant ones to `format.rs` tests
- [x] 6.3 Update `src/tidy/tests.rs` to use `Store::save()` instead of `apply_lock_diff`/`create`
- [x] 6.4 Update `tests/common/setup.rs` to use `Store` API
- [x] 6.5 Update `tests/integ_tidy.rs`, `tests/integ_upgrade.rs`, `tests/integ_pipeline.rs`, `tests/e2e_pipeline.rs` to use `Store` API
- [x] 6.6 Update `src/config.rs` tests to remove `lock_migrated` / `manifest_migrated` fields
- [x] 6.7 Run `cargo test` and `cargo clippy` — fix any remaining references

## 7. Update file-format spec

- [x] 7.1 Archive the delta spec into `openspec/specs/file-format/spec.md` (apply MODIFIED/REMOVED changes)
