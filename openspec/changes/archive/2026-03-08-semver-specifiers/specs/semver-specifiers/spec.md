## NEW Requirements

### Requirement: Manifest uses semver specifiers
The manifest SHALL use semver specifier strings as action version values instead of v-prefixed version strings.

#### Scenario: Caret specifier for major version
- **GIVEN** a user wants to track `actions/checkout` at major version 6
- **THEN** the manifest entry is `"actions/checkout" = "^6"`

#### Scenario: Caret specifier for minor version
- **GIVEN** a user wants to track `release-plz/action` at minor version 0.5
- **THEN** the manifest entry is `"release-plz/action" = "^0.5"`

#### Scenario: Tilde specifier for patch version
- **GIVEN** a user wants to pin `setup-rust-toolchain` to patch 1.15.2
- **THEN** the manifest entry is `"actions-rust-lang/setup-rust-toolchain" = "~1.15.2"`

#### Scenario: Non-semver refs are unchanged
- **GIVEN** a user wants to track an action at branch `main`
- **THEN** the manifest entry is `"some/action" = "main"`

#### Scenario: SHA refs are unchanged
- **GIVEN** a user wants to pin to a specific commit SHA
- **THEN** the manifest entry is `"some/action" = "abc123def456789012345678901234567890abcd"`

### Requirement: Manifest has [gx] section with min_version
The manifest SHALL support an optional `[gx]` section with a `min_version` field.

#### Scenario: New manifest includes [gx] section
- **WHEN** gx writes a manifest (via init, tidy, or upgrade)
- **THEN** the manifest includes `[gx]\nmin_version = "<current_gx_version>"`

#### Scenario: Absent [gx] section indicates v1 format
- **GIVEN** a manifest without a `[gx]` section
- **WHEN** parsed by gx
- **THEN** the parser treats it as v1 format (v-prefixed versions)

#### Scenario: Old gx binary rejects new manifest
- **GIVEN** a manifest with `min_version = "0.6.0"`
- **AND** the running gx binary is version 0.5.10
- **WHEN** the manifest is parsed
- **THEN** gx exits with error: `gx.toml requires gx >= 0.6.0 (you have 0.5.10)`

### Requirement: Override version field contains specifiers
Override entries SHALL use the `version` field name with specifier values.

#### Scenario: Override with caret specifier
- **GIVEN** an override for `actions/checkout` in deploy.yml
- **THEN** the override entry is `{ workflow = ".github/workflows/deploy.yml", version = "^3" }`

### Requirement: Specifier type with semver::VersionReq
The domain SHALL have a `Specifier` type that wraps `semver::VersionReq` for range matching.

#### Scenario: Range specifier matches version
- **GIVEN** specifier `"^6"`
- **WHEN** checked against version `6.0.2`
- **THEN** it matches

#### Scenario: Range specifier rejects version outside range
- **GIVEN** specifier `"^6"`
- **WHEN** checked against version `7.0.0`
- **THEN** it does not match

#### Scenario: Tilde specifier restricts to minor
- **GIVEN** specifier `"~1.15.2"`
- **WHEN** checked against version `1.15.3`
- **THEN** it matches
- **WHEN** checked against version `1.16.0`
- **THEN** it does not match

#### Scenario: Non-semver specifier
- **GIVEN** specifier `"main"` (branch ref)
- **THEN** `matches()` returns false for any semver version

### Requirement: Specifier provides comment derivation
Each `Specifier` SHALL produce a human-readable comment string for workflow files.

#### Scenario: Caret specifier comment
- **GIVEN** specifier `"^6"`
- **THEN** comment is `"v6"`

#### Scenario: Tilde specifier comment
- **GIVEN** specifier `"~1.15.2"`
- **THEN** comment is `"v1.15.2"`

#### Scenario: Ref specifier comment
- **GIVEN** specifier `"main"`
- **THEN** comment is `"main"`

## UPDATED Requirements

### Requirement: Lock file v1.4 entry format
Each action entry in the lock file SHALL be a TOML inline table with six fields: `sha`, `version`, `comment`, `repository`, `ref_type`, and `date`. The lock key SHALL use the manifest specifier.

#### Scenario: Standard lock entry with specifier key
- **GIVEN** an action with manifest specifier `"^6"` resolved to SHA `de0fac2e...` with most specific tag `v6.0.2`
- **THEN** the lock file entry is:
  ```toml
  "actions/checkout@^6" = { sha = "de0fac2e...", version = "v6.0.2", comment = "v6", repository = "actions/checkout", ref_type = "release", date = "2026-02-15T10:35:00Z" }
  ```

#### Scenario: Tilde specifier lock entry
- **GIVEN** manifest specifier `"~1.15.2"` resolved to SHA `1780873c...` with tag `v1.15.2`
- **THEN** the lock key is `"actions-rust-lang/setup-rust-toolchain@~1.15.2"` and `comment = "v1.15.2"`

#### Scenario: Lock file version is 1.4
- **THEN** the lock file has `version = "1.4"` at the top level

### Requirement: comment field stores workflow display version
The `comment` field SHALL contain the human-readable string written as the `# comment` in workflow files.

#### Scenario: Comment from init/tidy
- **GIVEN** a workflow with `uses: actions/checkout@sha123... # v6`
- **WHEN** tidy scans and creates a lock entry
- **THEN** `comment = "v6"` (preserved from workflow)

#### Scenario: Comment from cross-range upgrade
- **GIVEN** manifest specifier `"^4"` upgraded to `"^6"`
- **THEN** the new lock entry has `comment = "v6"`

#### Scenario: Comment for in-range upgrade
- **GIVEN** manifest specifier `"^6"` with existing `comment = "v6"`
- **WHEN** an in-range upgrade occurs (e.g., v6.0.2 → v6.2.0)
- **THEN** `comment` remains `"v6"` (unchanged)

### Requirement: Migration from v1 manifest to v2
The manifest parser SHALL read both v1 (v-prefixed) and v2 (specifier) formats into the same domain model.

#### Scenario: v1 major version migrated
- **GIVEN** manifest entry `"actions/checkout" = "v6"`
- **WHEN** parsed
- **THEN** domain specifier is `"^6"`

#### Scenario: v1 minor version migrated
- **GIVEN** manifest entry `"release-plz/action" = "v0.5"`
- **WHEN** parsed
- **THEN** domain specifier is `"^0.5"`

#### Scenario: v1 patch version migrated
- **GIVEN** manifest entry `"setup-rust-toolchain" = "v1.15.2"`
- **WHEN** parsed
- **THEN** domain specifier is `"~1.15.2"`

#### Scenario: v1 branch ref unchanged
- **GIVEN** manifest entry `"some/action" = "main"`
- **WHEN** parsed
- **THEN** domain specifier is `"main"`

### Requirement: Migration from v1.3 lock to v1.4
The lock parser SHALL read v1.3 format and convert it to the v1.4 domain model.

#### Scenario: v1.3 entry migrated to v1.4
- **GIVEN** lock entry `"actions/checkout@v6" = { sha = "...", version = "v6.0.2", specifier = "^6", ... }`
- **WHEN** parsed
- **THEN** the domain lock key is `actions/checkout@^6`
- **AND** lock entry has `comment = "v6"` (derived from old key version `v6`)
- **AND** `specifier` field is dropped

### Requirement: Write-time migration
Migration SHALL occur transparently when a write command (tidy, init, upgrade) outputs files.

#### Scenario: Read-only command on old format
- **GIVEN** a v1 manifest and v1.3 lock
- **WHEN** `gx lint` runs
- **THEN** no files are modified

#### Scenario: Write command triggers migration
- **GIVEN** a v1 manifest and v1.3 lock
- **WHEN** `gx tidy` runs
- **THEN** both files are written in current format (v2 manifest, v1.4 lock)

#### Scenario: Migration message is concise
- **GIVEN** migration occurred during a write command
- **THEN** output includes `migrated gx.toml → semver specifiers` and/or `migrated gx.lock → v1.4`
- **AND** no other migration details are printed

### Requirement: Unsupported future versions produce hard errors
When the parser encounters a version newer than it supports, it SHALL error.

#### Scenario: Future lock version
- **GIVEN** a lock file with `version = "2.0"`
- **WHEN** parsed by current gx
- **THEN** error: unsupported lock file version

### Requirement: Upgrade preserves operator
Cross-range upgrades SHALL preserve the semver operator from the original specifier.

#### Scenario: Caret preserved on major upgrade
- **GIVEN** manifest specifier `"^4"` and best candidate `v6.1.0`
- **THEN** new specifier is `"^6"` (caret preserved, major precision)

#### Scenario: Tilde preserved on minor upgrade
- **GIVEN** manifest specifier `"~1.15.2"` and best candidate `v1.16.0`
- **THEN** new specifier is `"~1.16.0"` (tilde preserved, patch precision)

#### Scenario: Caret with minor precision preserved
- **GIVEN** manifest specifier `"^4.2"` and best candidate `v5.0.0`
- **THEN** new specifier is `"^5.0"` (caret preserved, minor precision)

### Requirement: In-range detection uses semver::VersionReq
Upgrade SHALL use `semver::VersionReq::matches()` instead of hand-rolled range logic.

#### Scenario: Caret range match
- **GIVEN** specifier `"^6"` and candidate `v6.2.0`
- **THEN** candidate is in-range (VersionReq::matches returns true)

#### Scenario: Caret range miss
- **GIVEN** specifier `"^6"` and candidate `v7.0.0`
- **THEN** candidate is cross-range (VersionReq::matches returns false)

#### Scenario: Tilde range match
- **GIVEN** specifier `"~1.15.2"` and candidate `v1.15.4`
- **THEN** candidate is in-range

#### Scenario: Tilde range miss
- **GIVEN** specifier `"~1.15.2"` and candidate `v1.16.0`
- **THEN** candidate is cross-range
