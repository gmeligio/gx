## ADDED Requirements

### Requirement: SHA-first lock resolution from workflow SHAs
When workflow files pin an action to a SHA, lock resolution SHALL use that SHA directly to build the lock entry. The version, ref_type, and date fields SHALL be derived from the SHA, not from resolving the manifest version tag via the registry.

#### Scenario: Init with SHA-pinned action
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: jdx/mise-action@6d1e696... # v3`
- **AND** the registry reports that `6d1e696...` points to tags `[v3, v3.6, v3.6.1]`
- **WHEN** tidy (init) runs
- **THEN** the manifest adds `jdx/mise-action = "v3"` (from the comment)
- **AND** the lock entry has `sha = "6d1e696..."` (the workflow SHA, not a freshly resolved one)
- **AND** the lock entry has `version = "v3.6.1"` (most specific tag for that SHA)
- **AND** the lock entry has `specifier = "^3"` (derived from manifest version `v3`)

#### Scenario: Fallback to version-based resolution when no workflow SHA
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: actions/checkout@v4` (no SHA pin)
- **WHEN** tidy (init) runs
- **THEN** the lock entry SHA is obtained from the registry by resolving the `v4` tag

#### Scenario: Existing lock entry is not re-resolved
- **GIVEN** the lock already has a complete entry for `(actions/checkout, v4)`
- **WHEN** tidy runs
- **THEN** no registry call is made for that entry (workflow SHA is not used to overwrite)

### Requirement: resolve_from_sha derives all lock fields from SHA
`ActionResolver` SHALL provide a `resolve_from_sha` method that takes an `ActionId` and `CommitSha` and returns a `ResolvedAction` with version, ref_type, and date derived from the SHA.

#### Scenario: SHA with tags resolves to most specific version
- **GIVEN** action `actions/checkout` with SHA `abc123...`
- **AND** the registry reports tags `[v4, v4.2, v4.2.1]` for that SHA
- **WHEN** `resolve_from_sha` is called
- **THEN** the result has `version = "v4.2.1"` (most specific)
- **AND** the result has `ref_type = Tag`

#### Scenario: SHA with no tags falls back to SHA as version
- **GIVEN** action `actions/checkout` with SHA `abc123...`
- **AND** the registry reports no tags for that SHA
- **WHEN** `resolve_from_sha` is called
- **THEN** the result has `version = "abc123..."` (the SHA itself)
- **AND** the result has `ref_type = Commit`

### Requirement: Lock version field uses most specific tag
The lock entry's `version` field SHALL always be the most specific (most components) semver tag pointing to the entry's SHA. Among tags with the same number of components, the highest version SHALL win.

#### Scenario: Most specific tag selected for lock
- **GIVEN** a SHA points to tags `[v3, v3.6, v3.6.1]`
- **WHEN** selecting the version for a lock entry
- **THEN** the version is `v3.6.1` (3 components > 2 > 1)

#### Scenario: Highest version wins among same component count
- **GIVEN** a SHA points to tags `[v3.6.1, v3.6.2]`
- **WHEN** selecting the version for a lock entry
- **THEN** the version is `v3.6.2` (higher patch)
