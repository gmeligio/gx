### Requirement: Resolution returns ref_type and date
The version registry SHALL return the ref_type, repository, and date alongside the commit SHA when resolving a ref.

#### Scenario: Tag ref resolved with release
- **WHEN** resolving `actions/checkout@v6`
- **AND** `/git/ref/tags/v6` succeeds
- **AND** `/releases/tags/v6` returns a release with `published_at`
- **THEN** the result contains `ref_type = Release`, the release's `published_at` as `date`, and `repository = "actions/checkout"`

#### Scenario: Tag ref resolved without release (annotated tag)
- **WHEN** resolving `some/action@v1`
- **AND** `/git/ref/tags/v1` succeeds and points to a tag object
- **AND** no release exists for `v1`
- **THEN** the result contains `ref_type = Tag`, the tag's `tagger.date` as `date`

#### Scenario: Tag ref resolved without release (lightweight tag)
- **WHEN** resolving `some/action@v1`
- **AND** `/git/ref/tags/v1` succeeds and points directly to a commit
- **AND** no release exists for `v1`
- **THEN** the result contains `ref_type = Tag`, the commit's `committer.date` as `date`

#### Scenario: Branch ref resolved
- **WHEN** resolving `some/action@main`
- **AND** `/git/ref/tags/main` fails
- **AND** `/git/ref/heads/main` succeeds
- **THEN** the result contains `ref_type = Branch`, the commit's `committer.date` as `date`

#### Scenario: Direct SHA passthrough
- **WHEN** resolving `some/action@abc123def456789012345678901234567890abcd`
- **AND** the ref is a 40-character hex string
- **THEN** the result contains `ref_type = Commit`, the commit's `committer.date` as `date`

### Requirement: Resolution date uses best available source
The system SHALL prefer more authoritative date sources over less authoritative ones.

#### Scenario: Date priority order
- **GIVEN** a tag ref that has both a GitHub Release and an annotated tag
- **THEN** the Release `published_at` is used (not `tagger.date`)
- **BECAUSE** `published_at` is a GitHub-generated timestamp that cannot be forged

### Requirement: ResolvedAction carries metadata
The `ResolvedAction` struct SHALL include `repository`, `ref_type`, and `date` so that `Lock::set()` can store the full entry.

#### Scenario: Resolution flows through to lock
- **WHEN** `ActionResolver::resolve()` returns a `Resolved` result
- **AND** `lock.set(&resolved_action)` is called
- **THEN** the lock entry contains all four fields (sha, repository, ref_type, date)

### Requirement: Subpath action repository resolution
The `repository` field SHALL reflect the actual GitHub repository queried, not the full action path.

#### Scenario: Subpath action stores base repo
- **GIVEN** action ID `github/codeql-action/upload-sarif`
- **WHEN** resolved against `github/codeql-action`
- **THEN** `repository = "github/codeql-action"`
