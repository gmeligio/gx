## REMOVED Requirements

### Requirement: Manifest has [gx] section with min_version
**Reason**: The `[gx]` section is removed as part of the versionless format. There is no `min_version` check.
**Migration**: Manifests with `[gx]` section have it stripped on read and set `migrated = true`. New manifests are written without it.

### Requirement: Unsupported future versions produce hard errors
**Reason**: There are no version fields to check. Forward compatibility is handled by ignoring unknown fields.
**Migration**: No replacement needed. Unknown fields are silently ignored.

## MODIFIED Requirements

### Requirement: Lock file v1.4 entry format
Each action entry in the lock file SHALL be a standard TOML table under `[actions]` with six fields: `sha`, `version`, `comment`, `repository`, `ref_type`, and `date`. The lock key SHALL use the manifest specifier. There is no top-level `version` field.

#### Scenario: Standard lock entry with specifier key
- **GIVEN** an action with manifest specifier `"^6"` resolved to SHA `de0fac2e...` with tag `v6.0.2`
- **THEN** the lock file entry is:
  ```toml
  [actions."actions/checkout@^6"]
  sha = "de0fac2e..."
  version = "v6.0.2"
  comment = "v6"
  repository = "actions/checkout"
  ref_type = "release"
  date = "2026-02-15T10:35:00Z"
  ```

#### Scenario: Tilde specifier lock entry
- **GIVEN** manifest specifier `"~1.15.2"`
- **THEN** lock key is `"actions-rust-lang/setup-rust-toolchain@~1.15.2"` and `comment = "v1.15.2"`

### Requirement: Write-time migration
Migration SHALL occur transparently when a write command (tidy, init, upgrade) outputs files.

#### Scenario: Read-only command on old format
- **WHEN** `gx lint` runs on a v1 manifest and v1.3 lock
- **THEN** files are not modified

#### Scenario: Write command triggers migration
- **WHEN** `gx tidy` runs on a v1 manifest and v1.3 lock
- **THEN** both files are written in current format (versionless manifest, standard-table lock)

#### Scenario: Migration message is concise
- **WHEN** migration occurs
- **THEN** output includes `migrated gx.toml → semver specifiers` and/or `migrated gx.lock`
