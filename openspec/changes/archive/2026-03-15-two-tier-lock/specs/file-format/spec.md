## Versionless Format

### MODIFIED Requirement: Versionless file formats
Neither `gx.toml` nor `gx.lock` SHALL contain a format version field.

#### MODIFIED Scenario: New lock has no version field
- **GIVEN** a lock file written by the current gx version
- **THEN** the file contains `[resolutions]` and `[actions]` sections with standard TOML tables
- **AND** there is no top-level `version` field

---

## Lock Format

### REMOVED Requirement: Lock file entry format
The flat `[actions."action@specifier"]` format with six fields is replaced by the two-tier structure below.

### MODIFIED Requirement: Lock file structure

The lock file SHALL have two top-level sections: `[resolutions]` and `[actions]`.

- `[resolutions]` maps `(ActionId, Specifier)` to a resolved version and comment, using nested TOML tables keyed by action ID then specifier string.
- `[actions]` maps `(ActionId, Version)` to commit metadata (sha, repository, ref_type, date), using nested TOML tables keyed by action ID then version string.

The `comment` field belongs in `[resolutions]` because it depends on specifier precision, not on the resolved version.

#### Scenario: Standard lock with resolutions and actions
- **GIVEN** an action `actions/checkout@^6` resolved to SHA `de0fac2e...` at version `v6.2.3`, from a GitHub Release published at `2026-02-15T10:35:00Z`
- **THEN** the lock file is:
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

#### Scenario: Subpath action stores base repository in actions tier
- **GIVEN** an action `github/codeql-action/upload-sarif@^3` resolved to version `v3.28.0` against repository `github/codeql-action`
- **THEN** the lock file contains:
  ```toml
  [resolutions."github/codeql-action/upload-sarif"."^3"]
  version = "v3.28.0"
  comment = "v3"

  [actions."github/codeql-action/upload-sarif"."v3.28.0"]
  sha = "..."
  repository = "github/codeql-action"
  ref_type = "tag"
  date = "..."
  ```

#### Scenario: Multiple specifiers share one action entry
- **GIVEN** actions `actions/checkout@^4` and `actions/checkout@^4.2` both resolve to `v4.2.1`
- **THEN** the lock file has two resolution entries pointing to one action entry:
  ```toml
  [resolutions."actions/checkout"."^4"]
  version = "v4.2.1"
  comment = "v4"

  [resolutions."actions/checkout"."^4.2"]
  version = "v4.2.1"
  comment = "v4.2"

  [actions."actions/checkout"."v4.2.1"]
  sha = "abc123..."
  repository = "actions/checkout"
  ref_type = "tag"
  date = "2026-01-01T00:00:00Z"
  ```

#### Scenario: Non-semver branch ref in resolution
- **GIVEN** an action `actions/checkout@main` resolved to SHA `abc123...`
- **THEN** the resolution uses the branch name as both specifier key and version:
  ```toml
  [resolutions."actions/checkout"."main"]
  version = "main"
  comment = ""

  [actions."actions/checkout"."main"]
  sha = "abc123..."
  repository = "actions/checkout"
  ref_type = "branch"
  date = "2026-03-01T00:00:00Z"
  ```

#### Scenario: Resolutions are sorted by action ID then specifier
- **GIVEN** resolutions for `actions/setup-node@^3` and `actions/checkout@^6`
- **WHEN** the lock is written
- **THEN** `actions/checkout` resolutions appear before `actions/setup-node` resolutions

#### Scenario: Actions are sorted by action ID then version
- **GIVEN** action entries for `actions/setup-node@v3.7.0` and `actions/checkout@v6.2.3`
- **WHEN** the lock is written
- **THEN** `actions/checkout` entries appear before `actions/setup-node` entries

### Requirement: Roundtrip integrity
Lock file serialization and deserialization SHALL be lossless for known fields across both tiers.

#### Scenario: Two-tier roundtrip
- **GIVEN** a two-tier lock file with resolutions and action entries
- **WHEN** the lock is read and then written back
- **THEN** the output is byte-for-byte identical to the input

---

### MODIFIED Requirement: Forward-compatible reads
The parser SHALL ignore unknown TOML keys and sections without erroring. This applies to both `[resolutions]` and `[actions]` sections.

#### Scenario: Resolution entry with unknown field
- **GIVEN** a resolution entry containing an unknown field `priority = "high"`
- **WHEN** the lock file is parsed
- **THEN** parsing succeeds and the unknown field is ignored

#### Scenario: Action entry with unknown field
- **GIVEN** an action entry containing an unknown field `checksum = "abc123"`
- **WHEN** the lock file is parsed
- **THEN** parsing succeeds and the unknown field is ignored

---

## Format Migration

### MODIFIED Requirement: Write-time migration
Migration SHALL occur transparently when a write command (tidy, init, upgrade) outputs files. Read-only commands (lint) do not modify files. This now includes flat-to-two-tier migration in addition to prior format migrations.

### MODIFIED Requirement: Migration from flat lock to two-tier
The system SHALL transparently migrate flat-format lock files (single `[actions]` section with `"action@specifier"` composite keys) to the two-tier format. Each flat entry is split into a resolution entry (specifier → version, comment) and an action entry (version → sha, repository, ref_type, date).

All prior format migrations (v1.0, v1.3, v1.4) continue to work by first migrating to the flat format, then migrating flat to two-tier.

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

#### Scenario: v1.0 lock migrates to two-tier
- **GIVEN** a v1.0 lock file (plain SHA strings)
- **WHEN** a write command runs
- **THEN** the file is migrated through v1.0 → flat → two-tier

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
- **THEN** the output has two resolution entries and one action entry:
  ```toml
  [resolutions."actions/checkout"."^4"]
  version = "v4.2.1"
  comment = "v4"

  [resolutions."actions/checkout"."^4.2"]
  version = "v4.2.1"
  comment = "v4.2"

  [actions."actions/checkout"."v4.2.1"]
  sha = "abc123..."
  repository = "actions/checkout"
  ref_type = "tag"
  date = "2026-01-01T00:00:00Z"
  ```

#### Scenario: Flat entry with missing version migrates with specifier fallback
- **GIVEN** a flat lock entry with `version` absent or empty (from an incomplete tidy):
  ```toml
  [actions."actions/checkout@^6"]
  sha = "de0fac2e..."
  comment = "v6"
  repository = "actions/checkout"
  ref_type = "release"
  date = "2026-02-15T10:35:00Z"
  ```
- **WHEN** migration runs
- **THEN** the resolution is created with the specifier as the version fallback:
  ```toml
  [resolutions."actions/checkout"."^6"]
  version = "^6"
  comment = "v6"
  ```
- **AND** the action entry key uses the same fallback version
- **AND** the entry is detected as incomplete on next tidy run (triggering REFINE to get the real version)

#### Scenario: Migration message
- **WHEN** flat-to-two-tier migration occurs
- **THEN** output includes `migrated gx.lock`
