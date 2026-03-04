## MODIFIED Requirements

### Requirement: resolve_from_sha derives all lock fields from SHA
`ActionResolver` SHALL provide a `resolve_from_sha` method that takes an `ActionId`, `CommitSha`, and `&mut ShaIndex` and returns a `ResolvedAction` with version, ref_type, and date derived from the SHA. The method SHALL obtain the `ShaDescription` from the `ShaIndex` (which handles deduplication) rather than calling `describe_sha` on the registry directly.

#### Scenario: SHA with tags resolves to most specific version
- **GIVEN** action `actions/checkout` with SHA `abc123...`
- **AND** the registry reports tags `[v4, v4.2, v4.2.1]` for that SHA
- **WHEN** `resolve_from_sha` is called with a `ShaIndex`
- **THEN** the result has `version = "v4.2.1"` (most specific)
- **AND** the result has `ref_type = Tag`

#### Scenario: SHA with no tags falls back to SHA as version
- **GIVEN** action `actions/checkout` with SHA `abc123...`
- **AND** the registry reports no tags for that SHA
- **WHEN** `resolve_from_sha` is called with a `ShaIndex`
- **THEN** the result has `version = "abc123..."` (the SHA itself)
- **AND** the result has `ref_type = Commit`

#### Scenario: Second call for same SHA reuses indexed description
- **GIVEN** `resolve_from_sha` was previously called for `(actions/checkout, abc123...)`
- **WHEN** `resolve_from_sha` is called again for the same SHA
- **THEN** the `ShaIndex` returns the cached description
- **AND** no additional registry call is made

### Requirement: correct_version uses ShaIndex for tag knowledge
`ActionResolver::correct_version` SHALL accept a `&mut ShaIndex` parameter and obtain tags from it instead of calling `tags_for_sha` on the registry directly.

#### Scenario: correct_version populates ShaIndex for later resolve_from_sha
- **GIVEN** an empty `ShaIndex`
- **AND** action `actions/checkout` with SHA `abc123...`
- **WHEN** `correct_version` is called (which triggers `get_or_describe`)
- **AND** later `resolve_from_sha` is called for the same `(actions/checkout, abc123...)`
- **THEN** the second call does not trigger a registry call

### Requirement: upgrade_sha_versions_to_tags uses ShaIndex directly
`upgrade_sha_versions_to_tags` SHALL use `sha_index.get_or_describe()` to obtain tags for SHA-pinned manifest entries instead of calling `tags_for_sha` on the registry directly. It SHALL pick the most specific tag from the returned `ShaDescription` and update the manifest version.

#### Scenario: SHA version upgraded to tag via ShaIndex
- **GIVEN** a manifest entry `actions/checkout` with version set to SHA `abc123...`
- **AND** the registry reports tags `[v4, v4.2]` for that SHA
- **WHEN** `upgrade_sha_versions_to_tags` is called with a `ShaIndex`
- **THEN** the manifest version is updated to `v4.2` (most specific)
- **AND** the `ShaDescription` is cached in the `ShaIndex`

#### Scenario: SHA with no tags remains unchanged
- **GIVEN** a manifest entry `actions/checkout` with version set to SHA `abc123...`
- **AND** the registry reports no tags for that SHA
- **WHEN** `upgrade_sha_versions_to_tags` is called with a `ShaIndex`
- **THEN** the manifest version remains as the SHA
- **AND** the `ShaDescription` is still cached for later phases

#### Scenario: upgrade populates ShaIndex for later update_lock
- **GIVEN** `upgrade_sha_versions_to_tags` described `(actions/checkout, abc123...)`
- **WHEN** `update_lock` later calls `resolve_from_sha` for the same SHA
- **THEN** the `ShaIndex` returns the cached description
- **AND** no additional registry call is made

## DELETED Requirements

### Requirement: refine_version deleted
`ActionResolver::refine_version` SHALL be removed. It has no callers in production code. Its logic (find the most specific tag for a SHA) is handled by `upgrade_sha_versions_to_tags` which now uses `ShaIndex` directly.
