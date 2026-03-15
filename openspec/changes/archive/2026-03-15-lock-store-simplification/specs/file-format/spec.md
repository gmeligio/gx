## MODIFIED Requirements

### Requirement: Write-time migration
Migration SHALL occur transparently when a write command (tidy, init, upgrade) outputs files. Read-only commands (lint) do not modify files. The lock file is always written in the current two-tier format via full rewrite — no diff-based patching, no migration flags, no migration messages.

#### Scenario: Flat lock is silently migrated on tidy
- **WHEN** a flat-format lock file exists and `gx tidy` runs
- **THEN** the lock file is rewritten in two-tier format
- **AND** no "migrated" message is printed

#### Scenario: Flat lock is silently migrated on upgrade
- **WHEN** a flat-format lock file exists and `gx upgrade` runs
- **THEN** the lock file is rewritten in two-tier format
- **AND** no "migrated" message is printed

#### Scenario: Two-tier lock is rewritten identically
- **WHEN** a two-tier lock file exists and a write command runs with no logical changes
- **THEN** the lock file content is byte-for-byte identical after the write

#### Scenario: Lock save is always a full rewrite
- **WHEN** `Store::save()` is called
- **THEN** the entire `Lock` domain model is serialized to disk
- **AND** no prior file content is read or patched

### Requirement: Migration from flat lock to two-tier
The system SHALL transparently read flat-format lock files (single `[actions]` section with `"action@specifier"` composite keys) and convert them to the domain `Lock` model. The next write produces the two-tier format.

#### Scenario: Flat lock migrates to two-tier
- **GIVEN** a flat lock file:
  ```toml
  [actions."actions/checkout@^6"]
  sha = "de0fac2e..."
  version = "v6.2.3"
  comment = "v6"
  repository = "actions/checkout"
  ref_type = "release"
  date = "2026-02-15T10:35:00Z"
  ```
- **WHEN** a write command (tidy, init, upgrade) runs
- **THEN** the output is two-tier format:
  ```toml
  [resolutions."actions/checkout"."^6"]
  version = "v6.2.3"
  comment = "v6"

  [actions."actions/checkout"."v6.2.3"]
  sha = "de0fac2e..."
  repository = "actions/checkout"
  ref_type = "release"
  date = "2026-02-15T10:35:00Z"
  ```

#### Scenario: Flat entries deduplicating to one action entry
- **GIVEN** a flat lock file with two entries resolving to the same version:
  ```toml
  [actions."actions/checkout@^4"]
  sha = "abc123..."
  version = "v4.2.1"
  comment = "v4"
  repository = "actions/checkout"
  ref_type = "tag"
  date = "2026-01-01T00:00:00Z"

  [actions."actions/checkout@^4.2"]
  sha = "abc123..."
  version = "v4.2.1"
  comment = "v4.2"
  repository = "actions/checkout"
  ref_type = "tag"
  date = "2026-01-01T00:00:00Z"
  ```
- **WHEN** migration runs
- **THEN** the output has two resolution entries and one action entry

#### Scenario: Flat entry with missing version migrates with specifier fallback
- **GIVEN** a flat lock entry with `version` absent or empty
- **WHEN** migration runs
- **THEN** the resolution is created with the specifier as the version fallback
- **AND** the entry is detected as incomplete on next tidy run

#### Scenario: v1.4 inline tables parsed as flat format
- **GIVEN** a lock file with `version = "1.4"` and inline table entries under `[actions]`
- **WHEN** `Store::load()` is called
- **THEN** the file is parsed as flat format (the `version` field is ignored by serde)
- **AND** the entries are converted to the domain `Lock` model

#### Scenario: Empty lock file returns default
- **GIVEN** a lock file that exists but is empty
- **WHEN** `Store::load()` is called
- **THEN** an empty `Lock::default()` is returned

#### Scenario: Unrecognized format produces an error
- **GIVEN** a lock file with non-TOML content or unrecognized TOML structure
- **WHEN** `Store::load()` is called
- **THEN** an error is returned indicating the format is unrecognized

### Requirement: Manifest v1 migration
v1 manifests (no `[gx]` section with `"v4"` style values) are parsed directly with specifier values converted via `from_v1()`. v2 manifests (with `[gx]` section) have the section stripped. The `manifest_migrated` flag is preserved for manifest migration messages. The lock-specific `lock_migrated` flag and `Parsed<T>` wrapper are removed; manifest migration uses a simpler mechanism.

#### Scenario: Manifest migration still reports
- **WHEN** a v1 manifest is loaded and a write command runs
- **THEN** the output includes `migrated gx.toml -> semver specifiers`

## REMOVED Requirements

### Requirement: Migration from v1.0 to current format
**Reason**: No users remain on v1.0 format. Breaking change accepted for ~3 total users.
**Migration**: Delete `gx.lock` and run `gx tidy` to regenerate.

### Requirement: Migration from v1.3 to current format
**Reason**: No users remain on v1.3 format. Breaking change accepted for ~3 total users.
**Migration**: Delete `gx.lock` and run `gx tidy` to regenerate.

### Requirement: Migration from v1.4 inline tables
**Reason**: v1.4 inline tables are a subset of the flat format. The flat format reader handles them. No separate v1.4 migration path is needed.
**Migration**: Handled automatically by the flat format reader.

### Requirement: Migration message is concise
**Reason**: Migration is now fully transparent. No user-facing messages for lock format changes.
**Migration**: Remove all "migrated gx.lock" progress messages from commands.
