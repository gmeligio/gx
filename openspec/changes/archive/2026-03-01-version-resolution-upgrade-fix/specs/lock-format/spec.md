### Requirement: Lock file v1.3 entry format
Each action entry in the lock file SHALL be a TOML inline table with six fields: `sha`, `version`, `specifier`, `repository`, `ref_type`, and `date`.

#### Scenario: Standard lock entry with all fields
- **GIVEN** an action `actions/checkout@v6` resolved to SHA `de0fac2e...` from a GitHub Release, where the SHA matches tag `v6.2.3`
- **THEN** the lock file entry is:
  ```toml
  "actions/checkout@v6" = { sha = "de0fac2e...", version = "v6.2.3", specifier = "^6", repository = "actions/checkout", ref_type = "release", date = "2026-02-15T10:35:00Z" }
  ```

#### Scenario: Patch-precision action with tilde specifier
- **GIVEN** an action `rust-lang/crates-io-auth-action@v1.0.3` resolved to SHA `b7e9a28e...`
- **THEN** `version = "v1.0.3"` and `specifier = "~1.0.3"`

#### Scenario: Lock file version is 1.3
- **THEN** the lock file has `version = "1.3"` at the top level

### Requirement: `version` field stores most specific resolved version
The `version` field SHALL contain the most specific semver tag pointing to the same SHA as the resolved ref.

#### Scenario: Floating tag resolved to specific version
- **GIVEN** manifest version `v4` resolves to SHA `abc123`
- **AND** tags `v4` and `v4.2.1` both point to SHA `abc123`
- **THEN** the lock entry has `version = "v4.2.1"`

#### Scenario: No more specific tag exists
- **GIVEN** manifest version `v1` resolves to SHA `def456`
- **AND** only tag `v1` points to SHA `def456`
- **THEN** the lock entry has `version = "v1"`

#### Scenario: Pre-release version stored as-is
- **GIVEN** manifest version `v3.0.0-beta.2` resolves to SHA `789abc`
- **AND** tag `v3.0.0-beta.2` points to SHA `789abc`
- **THEN** the lock entry has `version = "v3.0.0-beta.2"`

### Requirement: `specifier` field stores semver range
The `specifier` field SHALL contain the semver range derived from the manifest version's precision.

#### Scenario: Major precision specifier
- **GIVEN** manifest version `v4`
- **THEN** `specifier = "^4"`

#### Scenario: Minor precision specifier
- **GIVEN** manifest version `v4.2`
- **THEN** `specifier = "^4.2"`

#### Scenario: Patch precision specifier
- **GIVEN** manifest version `v4.1.0`
- **THEN** `specifier = "~4.1.0"`

### Requirement: Backward compatibility with pre-1.3 locks
The `version` and `specifier` fields SHALL be optional during deserialization for backward compatibility.

#### Scenario: Loading a v1.1 lock file
- **GIVEN** a lock file with `version = "1.1"` and entries without `version` or `specifier` fields
- **WHEN** loaded by the new code
- **THEN** the entries are parsed successfully with `version` and `specifier` as `None`

#### Scenario: First tidy populates new fields
- **GIVEN** a v1.1 lock file loaded without `version` or `specifier`
- **WHEN** `gx tidy` runs
- **THEN** all entries are updated with `version` and `specifier` fields
- **THEN** the lock file version is bumped to `"1.3"`

### Requirement: Roundtrip integrity with new fields
Lock file serialization and deserialization SHALL be lossless for all six fields.

#### Scenario: Save and reload preserves version and specifier
- **GIVEN** a lock with entries containing sha, version, specifier, repository, ref_type, and date
- **WHEN** saved to disk and reloaded
- **THEN** all field values are identical
