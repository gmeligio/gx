## Versionless Format

### Requirement: Versionless file formats
Neither `gx.toml` nor `gx.lock` SHALL contain a format version field. There is no `min_version`, no `version = "1.4"`, and no `[gx]` section.

#### Scenario: New manifest has no gx section
- **GIVEN** a manifest written by the current gx version
- **THEN** the file contains `[actions]` and optionally `[actions.overrides]` and `[lint]`
- **AND** there is no `[gx]` section

#### Scenario: New lock has no version field
- **GIVEN** a lock file written by the current gx version
- **THEN** the file contains `[actions]` with standard TOML tables
- **AND** there is no top-level `version` field

### Requirement: Forward-compatible reads
The parser SHALL ignore unknown TOML keys and sections without erroring.

#### Scenario: Manifest with unknown top-level section
- **GIVEN** a manifest containing an unknown section `[metadata]`
- **WHEN** the manifest is parsed
- **THEN** parsing succeeds and the unknown section is ignored

#### Scenario: Lock entry with unknown field
- **GIVEN** a lock entry containing an unknown field `checksum = "abc123"`
- **WHEN** the lock file is parsed
- **THEN** parsing succeeds and the unknown field is ignored

---

## Write Format

### Requirement: Writes use toml_edit
All file writes for `gx.toml` and `gx.lock` SHALL use `toml_edit::DocumentMut` to build the output. No manual string formatting (`format!()`, `writeln!()`) SHALL be used for TOML generation.

### Requirement: Fresh build on write
Writes SHALL build a fresh `DocumentMut` from the domain model. No prior document state is carried between read and write.

#### Scenario: Unknown fields are not preserved on write
- **GIVEN** a lock file containing an unknown field `checksum = "abc123"` in an entry
- **WHEN** the lock is read and then written back
- **THEN** the unknown field is not present in the output

#### Scenario: Write output is deterministic
- **GIVEN** the same domain model
- **WHEN** written twice
- **THEN** both outputs are identical byte-for-byte

---

## Manifest Format

### Requirement: Manifest structure
The manifest SHALL NOT contain a `[gx]` section. The file format is `[actions]` with optional `[actions.overrides]` and `[lint]` sections.

### Requirement: Manifest writes use toml_edit
Overrides SHALL be written as inline tables within arrays. Empty `[actions.overrides]` section is not written if no overrides exist.

#### Scenario: Manifest with actions and overrides
- **GIVEN** a manifest with `actions/checkout = "^4"` and an override `{ workflow = ".github/workflows/ci.yml", version = "^3" }`
- **WHEN** the manifest is written
- **THEN** the output is:
  ```toml
  [actions]
  "actions/checkout" = "^4"

  [actions.overrides]
  "actions/checkout" = [
    { workflow = ".github/workflows/ci.yml", version = "^3" },
  ]
  ```

#### Scenario: Manifest with actions only
- **GIVEN** a manifest with `actions/checkout = "^4"` and no overrides
- **WHEN** the manifest is written
- **THEN** the output contains `[actions]` section only (no empty `[actions.overrides]`)

---

## Lock Format

### Requirement: Lock file entry format
Each action entry in the lock file SHALL be a standard TOML table under `[actions]` with six fields: `sha`, `version`, `comment`, `repository`, `ref_type`, and `date`.

#### Scenario: Standard lock entry with all fields
- **GIVEN** an action `actions/checkout@^6` resolved to SHA `de0fac2e...` from a GitHub Release published at `2026-02-15T10:35:00Z`, where the SHA matches tag `v6.2.3`
- **THEN** the lock file entry is:
  ```toml
  [actions."actions/checkout@^6"]
  sha = "de0fac2e..."
  version = "v6.2.3"
  comment = "v6"
  repository = "actions/checkout"
  ref_type = "release"
  date = "2026-02-15T10:35:00Z"
  ```

#### Scenario: Subpath action stores base repository
- **GIVEN** an action `github/codeql-action/upload-sarif@^3` resolved against repository `github/codeql-action`
- **THEN** the `repository` field is `"github/codeql-action"`

#### Scenario: Entries are sorted alphabetically
- **GIVEN** a lock with entries for `actions/checkout@^6` and `actions/setup-node@^3`
- **WHEN** the lock is written
- **THEN** `actions/checkout@^6` appears before `actions/setup-node@^3`

### Requirement: Roundtrip integrity
Lock file serialization and deserialization SHALL be lossless for known fields.

---

## Format Migration

### Requirement: Write-time migration
Migration SHALL occur transparently when a write command (tidy, init, upgrade) outputs files. Read-only commands (lint) do not modify files.

#### Scenario: Migration message is concise
- **WHEN** migration occurs
- **THEN** output includes `migrated gx.toml -> semver specifiers` and/or `migrated gx.lock`

### Requirement: Migration from v1.0 to current format
The system SHALL transparently migrate v1.0 lock files (plain string SHA values with `version` field) to the current format.

### Requirement: Migration from v1.3 to current format
The system SHALL transparently migrate v1.3 lock files (`specifier` field and `@v6` style keys) to current `@^6` format.

### Requirement: Migration from v1.4 inline tables
The system SHALL transparently migrate v1.4 lock files (inline tables with `version = "1.4"`) to the new standard-table format.

### Requirement: Manifest v1 migration
v1 manifests (no `[gx]` section with `"v4"` style values) are parsed directly with specifier values converted via `from_v1()`. v2 manifests (with `[gx]` section) have the section stripped. Both set `migrated = true`.
