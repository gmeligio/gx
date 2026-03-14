## 1. Lock file writes — standard tables via toml_edit

- [x] 1.1 Replace `serialize_lock()` in `infra::lock::convert` with a `build_lock_document(lock: &Lock) -> DocumentMut` function that builds standard TOML tables (not inline tables)
- [x] 1.2 Replace `build_lock_inline_table()` with a helper that populates a standard table entry (sha, version, comment, repository, ref_type, date)
- [x] 1.3 Update `Store::save()` in `infra::lock::mod` to use the new `build_lock_document`
- [x] 1.4 Update `apply_lock_diff()` to build standard tables instead of inline tables
- [x] 1.5 Remove the `version = "1.4"` field from lock output

## 2. Lock file reads — migration from old formats

- [x] 2.1 Add v1.4 migration path: detect `version` field + inline tables → parse entries, set `migrated = true`
- [x] 2.2 Update format detection: no `version` field + standard tables = new format
- [x] 2.3 Ensure v1.0 and v1.3 migration paths still work and set `migrated = true`
- [x] 2.4 Remove `version` field from `LockData` struct (write side only; read side handles it in migration)

## 3. Manifest writes — toml_edit

- [x] 3.1 Replace `format_manifest_toml()` in `infra::manifest::convert` with a `build_manifest_document(manifest: &Manifest) -> DocumentMut` function
- [x] 3.2 Build `[actions]` section with sorted key-value pairs
- [x] 3.3 Build `[actions.overrides]` section with inline tables in arrays (skip section if empty)
- [x] 3.4 Build `[lint]` section if present (skip if empty)
- [x] 3.5 Update `Store::save()` in `infra::manifest::mod` to use the new builder
- [x] 3.6 Remove `[gx]` section from manifest output

## 4. Manifest reads — strip [gx] section

- [x] 4.1 When parsing a manifest with `[gx]` section, ignore it and set `migrated = true`
- [x] 4.2 Ensure v1 manifests (no `[gx]`, `"v4"` style values) still parse via `from_v1()`
- [x] 4.3 Remove `GxSection` struct and related code

## 5. Tests

- [x] 5.1 Lock roundtrip test: build domain model → write → read → assert identical
- [x] 5.2 Lock migration tests: v1.0, v1.3, v1.4 inputs → parse → assert `migrated = true` and correct domain values
- [x] 5.3 Lock output format test: assert standard tables (not inline), no `version` field, sorted entries
- [x] 5.4 Manifest roundtrip test: build domain model → write → read → assert identical
- [x] 5.5 Manifest migration tests: v1 (no gx section), v2 (with gx section) → parse → assert correct
- [x] 5.6 Manifest output format test: assert no `[gx]` section, overrides as inline tables, sorted actions
- [x] 5.7 Unknown fields test: parse files with extra fields → assert no error

## 6. Cleanup

- [x] 6.1 Remove all `format!()`/`writeln!()` TOML string building
- [x] 6.2 Remove `GxSection`, `min_version` handling, version comparison logic
- [x] 6.3 Remove `version` field handling from lock write path
- [x] 6.4 Update the project's own `.github/gx.toml` and `.github/gx.lock` to the new format
