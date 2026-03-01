## ADDED Requirements

### Requirement: Lock entry completeness check
`LockEntry` SHALL provide an `is_complete` method that returns `true` only when all required fields are present and valid for the given manifest version.

#### Scenario: Complete entry
- **WHEN** a lock entry has `sha`, `version` (Some, non-empty), `specifier` (Some, non-empty, matching manifest version), `repository` (non-empty), `ref_type`, and `date` (non-empty)
- **THEN** `is_complete(manifest_version)` returns `true`

#### Scenario: Missing specifier
- **WHEN** a lock entry has all fields except `specifier` is `None`
- **THEN** `is_complete(manifest_version)` returns `false`

#### Scenario: Empty specifier string
- **WHEN** a lock entry has `specifier = Some("")`
- **THEN** `is_complete(manifest_version)` returns `false`

#### Scenario: Stale specifier
- **WHEN** a lock entry has `specifier = Some("^6")` but the manifest version is `v6.1`
- **THEN** `is_complete(manifest_version)` returns `false`
- **BECAUSE** the specifier should be `^6.1` for a minor-precision manifest version

#### Scenario: Missing version
- **WHEN** a lock entry has `version` as `None`
- **THEN** `is_complete(manifest_version)` returns `false`

#### Scenario: Missing date
- **WHEN** a lock entry has `date` as an empty string
- **THEN** `is_complete(manifest_version)` returns `false`

#### Scenario: Non-semver manifest version
- **WHEN** a lock entry has a manifest version like `main` (branch) that produces no specifier
- **AND** all other fields are present
- **THEN** `is_complete(manifest_version)` returns `true`
- **BECAUSE** non-semver versions have no specifier, so `None` is correct

### Requirement: Incomplete lock entries trigger targeted operations
The tidy command SHALL check each lock entry for completeness and run only the operations needed to fill missing fields.

#### Scenario: Entry missing entirely triggers full resolution
- **WHEN** a manifest spec has no corresponding lock entry
- **THEN** the system runs RESOLVE (network) + REFINE (network) + DERIVE (local)
- **AND** creates a complete lock entry

#### Scenario: Entry missing only specifier triggers local derivation
- **WHEN** a lock entry exists with sha, version, repository, ref_type, and date
- **AND** specifier is missing or empty
- **THEN** the system runs DERIVE only (local computation from manifest version)
- **AND** does NOT make any network calls

#### Scenario: Entry missing version triggers refinement
- **WHEN** a lock entry exists with sha but version is `None`
- **THEN** the system runs REFINE (network: tags_for_sha) + DERIVE (local)
- **AND** does NOT run RESOLVE (the SHA is already known)

#### Scenario: Complete entry is skipped
- **WHEN** a lock entry passes `is_complete(manifest_version)`
- **THEN** no operations are performed for that entry

### Requirement: Self-healing on schema additions
When new fields are added to the lock entry schema, existing lock files SHALL be reconciled automatically on the next tidy run without explicit migration code.

#### Scenario: New field added to lock entry
- **GIVEN** a lock file with entries created before a new field was added
- **WHEN** `gx tidy` runs with code that includes the new field in `is_complete()`
- **THEN** entries missing the new field are detected as incomplete
- **AND** the appropriate operation fills in the new field
- **AND** no migration-specific code is required
