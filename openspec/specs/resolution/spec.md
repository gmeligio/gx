## SHA-First Resolution

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
- **THEN** no registry call is made for that entry

#### Scenario: Recoverable error in SHA-first path degrades gracefully
- **GIVEN** `describe_sha` returns `RateLimited` or `AuthRequired`
- **AND** fallback `resolve(spec)` also returns a recoverable error
- **WHEN** tidy (init) runs
- **THEN** the action is skipped with a warning (not a hard error)
- **AND** the lock file is written without that entry

#### Scenario: Strict error in resolution is still a hard failure
- **GIVEN** resolution returns `ResolveFailed` (404 not found)
- **WHEN** tidy (init) runs
- **THEN** the command fails with `TidyError::ResolutionFailed`

### Requirement: resolve_from_sha derives all lock fields from SHA
`ActionResolver` SHALL provide a `resolve_from_sha` method that takes an `ActionId`, `CommitSha`, and `&mut ShaIndex` and returns a `ResolvedAction` with version, ref_type, and date derived from the SHA. It obtains the `ShaDescription` from the `ShaIndex` (which handles deduplication).

#### Scenario: SHA with tags resolves to most specific version
- **GIVEN** action `actions/checkout` with SHA `abc123...`
- **AND** the registry reports tags `[v4, v4.2, v4.2.1]` for that SHA
- **WHEN** `resolve_from_sha` is called
- **THEN** the result has `version = "v4.2.1"` (most specific)

#### Scenario: SHA with no tags falls back to SHA as version
- **GIVEN** the registry reports no tags for that SHA
- **WHEN** `resolve_from_sha` is called
- **THEN** the result has `version = "abc123..."` and `ref_type = Commit`

### Requirement: correct_version uses ShaIndex for tag knowledge
`ActionResolver::correct_version` SHALL accept a `&mut ShaIndex` parameter and obtain tags from it instead of calling `tags_for_sha` on the registry directly.

### Requirement: upgrade_sha_versions_to_tags uses ShaIndex directly
`upgrade_sha_versions_to_tags` SHALL use `sha_index.get_or_describe()` to obtain tags for SHA-pinned manifest entries instead of calling `tags_for_sha` on the registry directly.

### Requirement: Lock version field uses most specific tag
The lock entry's `version` field SHALL always be the most specific (most components) semver tag pointing to the entry's SHA. Among tags with the same number of components, the highest version SHALL win.

### Requirement: lookup_sha dereferences annotated tags to commit SHAs
The `VersionRegistry::lookup_sha` implementation SHALL always return a commit SHA, even when the underlying git ref points to an annotated tag object. When the refs API returns `object.type == "tag"`, the implementation SHALL dereference via the tag object API to obtain the underlying commit SHA.

---

## SHA Description

### Requirement: VersionRegistry provides describe_sha operation
The `VersionRegistry` trait SHALL provide a `describe_sha` method that accepts an `ActionId` and `CommitSha` and returns a `ShaDescription` containing the tags pointing to that SHA, the base repository name, and the commit date.

### Requirement: ShaDescription carries commit metadata
The `ShaDescription` struct SHALL contain `tags: Vec<Version>`, `repository: Repository`, and `date: CommitDate`. It SHALL NOT contain `sha`, `ref_type`, or `version` (these are derived by the caller).

#### Scenario: ShaDescription fields use domain newtypes
- **GIVEN** a `ShaDescription` returned from `describe_sha`
- **WHEN** the caller accesses `repository`
- **THEN** it SHALL be a `Repository` newtype (not a bare `String`)
- **AND** `date` SHALL be a `CommitDate` newtype (not a bare `String`)
- **AND** the TOML serialization boundary in the infra layer converts between `String` and these newtypes

### Requirement: GithubRegistry describe_sha skips ref resolution
The `GithubRegistry` implementation of `describe_sha` SHALL go directly to the commit endpoint (`/commits/{sha}`) to fetch the date, skipping the tag/branch/commit fallback chain. Tag lookup failure is non-fatal (returns empty tags, not error).

---

## SHA Index

### Requirement: ShaIndex accumulates SHA descriptions during a plan run
`ShaIndex` SHALL be a domain entity that stores `ShaDescription` results keyed by `(ActionId, CommitSha)`. It SHALL provide a `get_or_describe` method that returns the stored description if cached, or calls `describe_sha` on the registry, stores the result, and returns it.

### Requirement: ShaIndex is scoped to a single plan run
A `ShaIndex` SHALL be created at the start of a `plan()` call and discarded when the plan completes. It SHALL NOT persist across separate plan invocations.

---

## Resolution Metadata

### Requirement: Resolution returns ref_type and date
The version registry SHALL return the ref_type, repository, and date alongside the commit SHA when resolving a ref.

#### Scenario: Tag with a GitHub Release
- **WHEN** resolving `actions/checkout@v6`
- **AND** `/releases/tags/v6` returns a release with `published_at`
- **THEN** the result contains `ref_type = Release`, the release's `published_at` as `date`

#### Scenario: Tag without a GitHub Release (annotated tag)
- **THEN** the result contains `ref_type = Tag`, the tag's `tagger.date` as `date`

#### Scenario: Tag without a GitHub Release (lightweight tag)
- **THEN** the result contains `ref_type = Tag`, the commit's `committer.date` as `date`

#### Scenario: Branch ref resolved
- **THEN** the result contains `ref_type = Branch`, the commit's `committer.date` as `date`

#### Scenario: Direct SHA passthrough
- **THEN** the result contains `ref_type = Commit`, the commit's `committer.date` as `date`

### Requirement: Resolution date uses best available source
Release `published_at` > tag `tagger.date` > commit `committer.date`. The date is read from `commit.committer.date` (nested), NOT the top-level `committer` field.

### Requirement: ResolvedAction carries metadata
The `ResolvedAction` struct SHALL include `repository` (`Repository`), `ref_type`, and `date` (`CommitDate`). It SHALL NOT carry `resolved_version` or `specifier` (these are outputs of REFINE and DERIVE).

### Requirement: ResolvedRef carries commit metadata

`ResolvedRef` SHALL have its `repository` and `date` fields updated to `Repository` and `CommitDate` newtypes respectively.

#### Scenario: ResolvedRef fields use domain newtypes
- **GIVEN** a `ResolvedRef` returned from resolution
- **THEN** `ResolvedRef::repository` SHALL be `Repository` (not `String`)
- **AND** `ResolvedRef::date` SHALL be `CommitDate` (not `String`)

### Requirement: SHA-pinned actions keep the workflow SHA
When a workflow already has a SHA-pinned action, the lock entry SHALL use the workflow's SHA, not the SHA that `lookup_sha()` returns for the version tag.
