## MODIFIED Requirements

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

### Requirement: Migration from v1.4 inline tables
The system SHALL transparently migrate v1.4 lock files (inline tables with `version = "1.4"`) to the new standard-table format.

#### Scenario: v1.4 lock migration
- **GIVEN** a v1.4 lock file with `version = "1.4"` and inline table entries
- **WHEN** the lock file is loaded
- **THEN** entries are parsed into domain types
- **AND** the parsed lock signals `migrated = true`

### Requirement: Migration from v1.0 to current format
The system SHALL transparently migrate v1.0 lock files to the current format.

#### Scenario: v1.0 lock migration
- **GIVEN** a v1.0 lock file with plain string SHA values and `version` field
- **WHEN** the lock file is loaded
- **THEN** entries are enriched with available metadata
- **AND** the parsed lock signals `migrated = true`

### Requirement: Migration from v1.3 to current format
The system SHALL transparently migrate v1.3 lock files to the current format.

#### Scenario: v1.3 lock migration
- **GIVEN** a v1.3 lock file with `specifier` field and `@v6` style keys
- **WHEN** the lock file is loaded
- **THEN** keys are migrated to `@^6` format
- **AND** `specifier` field is dropped, `comment` is derived
- **AND** the parsed lock signals `migrated = true`

### Requirement: Roundtrip integrity
Lock file serialization and deserialization SHALL be lossless for known fields.

#### Scenario: Save and reload preserves all known fields
- **GIVEN** a lock with entries containing sha, version, comment, repository, ref_type, and date
- **WHEN** saved to disk and reloaded
- **THEN** all known field values are identical
