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

#### Scenario: Recoverable error in SHA-first path degrades gracefully
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: actions/checkout@abc123... # v4`
- **AND** `describe_sha` returns `RateLimited` or `AuthRequired`
- **AND** fallback `resolve(spec)` also returns a recoverable error
- **WHEN** tidy (init) runs
- **THEN** the action is skipped with a warning (not a hard error)
- **AND** the lock file is written without that entry

#### Scenario: Strict error in resolution is still a hard failure
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: nonexistent/action@v1`
- **AND** resolution returns `ResolveFailed` (404 not found)
- **WHEN** tidy (init) runs
- **THEN** the command fails with `TidyError::ResolutionFailed`

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

#### Scenario: describe_sha error propagates through resolve_from_sha
- **GIVEN** `describe_sha` returns an error (e.g., `AuthRequired` or `RateLimited`)
- **WHEN** `resolve_from_sha` is called
- **THEN** the error is propagated to the caller

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

### Requirement: lookup_sha dereferences annotated tags to commit SHAs
The `VersionRegistry::lookup_sha` implementation SHALL always return a commit SHA in `ResolvedRef.sha`, even when the underlying git ref points to an annotated tag object. When the refs API returns `object.type == "tag"`, the implementation SHALL dereference via the tag object API to obtain the underlying commit SHA.

#### Scenario: Annotated tag is dereferenced to commit SHA
- **GIVEN** action `release-plz/action` with tag `v0.5`
- **AND** the refs API returns `object.type = "tag"` with a tag object SHA
- **WHEN** `lookup_sha` resolves `v0.5`
- **THEN** the returned `CommitSha` is the underlying commit (not the tag object)
- **AND** the SHA is valid for use in `uses: release-plz/action@{sha}` workflow pins

#### Scenario: Lightweight tag returns commit SHA directly
- **GIVEN** action `actions/checkout` with tag `v4`
- **AND** the refs API returns `object.type = "commit"` with a commit SHA
- **WHEN** `lookup_sha` resolves `v4`
- **THEN** the returned `CommitSha` is the commit SHA from the response (no dereferencing needed)

#### Scenario: Branch ref returns commit SHA directly
- **GIVEN** action `actions/checkout` with branch `main`
- **AND** the refs API returns `object.type = "commit"` with a commit SHA
- **WHEN** `lookup_sha` resolves `main`
- **THEN** the returned `CommitSha` is the commit SHA from the response

#### Scenario: Init with annotated-tag action produces valid lock entry
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: release-plz/action@v0.5` (annotated tag)
- **WHEN** tidy (init) runs
- **THEN** the lock entry SHA is a valid commit SHA (fetchable via the commits API)
- **AND** the workflow is pinned to that commit SHA
- **AND** running tidy again is a no-op (idempotent)
