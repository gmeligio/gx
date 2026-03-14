## ADDED Requirements

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
- **THEN** parsing succeeds
- **AND** the unknown section is ignored

#### Scenario: Lock entry with unknown field
- **GIVEN** a lock entry containing an unknown field `checksum = "abc123"`
- **WHEN** the lock file is parsed
- **THEN** parsing succeeds
- **AND** the unknown field is ignored

### Requirement: Writes use toml_edit
All file writes for `gx.toml` and `gx.lock` SHALL use `toml_edit::DocumentMut` to build the output. No manual string formatting (`format!()`, `writeln!()`) SHALL be used for TOML generation.

#### Scenario: Lock file written via toml_edit
- **GIVEN** a lock with entries to write
- **WHEN** the lock is saved to disk
- **THEN** the output is produced by `toml_edit::DocumentMut::to_string()`

#### Scenario: Manifest written via toml_edit
- **GIVEN** a manifest with actions and overrides to write
- **WHEN** the manifest is saved to disk
- **THEN** the output is produced by `toml_edit::DocumentMut::to_string()`

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
