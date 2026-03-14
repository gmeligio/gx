## Why

The current file formats embed version numbers (`min_version` in gx.toml, `version` in gx.lock) and rely on migration code to handle format changes. This creates a maintenance burden: every format tweak requires a version bump, a new migration path, and breaks older gx binaries. By switching to a versionless, additive-only format and using `toml_edit` for all writes, both files become eternally forward-compatible — new fields are ignored on read and preserved on write.

## What Changes

- **BREAKING**: Remove the `[gx]` section from `gx.toml` (the `min_version` field is dropped)
- **BREAKING**: Remove the `version` field from `gx.lock`
- **BREAKING**: Switch `gx.lock` entries from inline tables to standard TOML tables
- Replace all manual `format!()` string building with `toml_edit` for both manifest and lock file writes
- Forward compatibility is read-only: unknown fields are ignored on read; writes build fresh output from the domain model
- Read path remains backward-compatible: old formats (v1 manifest, v1.0/v1.3/v1.4 lock) are still parsed and trigger migration

## Capabilities

### New Capabilities

- `forward-compatible-format`: Defines the versionless, additive-only format contract, fresh-build write strategy, and forward-compatible read guarantee for both gx.toml and gx.lock

### Modified Capabilities

- `lock-format`: Lock entries change from inline tables to standard TOML tables; `version` field at top level is removed; writes use `toml_edit` instead of `format!()`
- `manifest-authority`: `[gx]` section and v1→v2 migration are removed; manifest writes use `toml_edit`
- `semver-specifiers`: `[gx]` section requirement removed; `version = "1.4"` lock scenario removed; "unsupported future versions" requirement removed; migration message updated

## Impact

- **Code**: `infra::manifest` and `infra::lock` modules — parse/write/convert/migration code
- **Domain**: `Parsed<T>` stays unchanged; no `toml_edit` types leak into domain
- **Dependencies**: `toml_edit` already in use for `apply_lock_diff`; no new crate needed
- **Files**: Every existing `gx.toml` and `gx.lock` will be rewritten on next tidy (one-time migration)
- **Breaking for users**: Older gx binaries that check `min_version` or `version = "1.4"` will fail to parse files written by the new format. Users must upgrade gx.
