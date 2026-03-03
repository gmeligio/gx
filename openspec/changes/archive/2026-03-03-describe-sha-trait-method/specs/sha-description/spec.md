## ADDED Requirements

### Requirement: VersionRegistry provides describe_sha operation
The `VersionRegistry` trait SHALL provide a `describe_sha` method that accepts an `ActionId` and `CommitSha` and returns a `ShaDescription` containing the tags pointing to that SHA, the base repository name, and the commit date.

#### Scenario: SHA with multiple tags
- **GIVEN** action `actions/checkout` with SHA `abc123...`
- **AND** the registry reports tags `[v4, v4.2, v4.2.1]` for that SHA
- **WHEN** `describe_sha` is called
- **THEN** the result has `tags = [v4, v4.2, v4.2.1]`
- **AND** the result has `repository = "actions/checkout"`
- **AND** the result has a non-empty `date`

#### Scenario: SHA with no tags
- **GIVEN** action `actions/checkout` with SHA `abc123...`
- **AND** the registry reports no tags for that SHA
- **WHEN** `describe_sha` is called
- **THEN** the result has `tags = []`
- **AND** the result has `repository = "actions/checkout"`

#### Scenario: Subpath action resolves to base repository
- **GIVEN** action `github/codeql-action/upload-sarif` with SHA `abc123...`
- **WHEN** `describe_sha` is called
- **THEN** the result has `repository = "github/codeql-action"`

#### Scenario: Token required error propagates
- **GIVEN** no GitHub token is configured
- **WHEN** `describe_sha` is called
- **THEN** the result is `Err(ResolutionError::TokenRequired)`

### Requirement: ShaDescription carries commit metadata
The `ShaDescription` struct SHALL contain `tags: Vec<Version>`, `repository: String`, and `date: String`. It SHALL NOT contain `sha` (the caller already has it), `ref_type` (derived from whether tags exist), or `version` (selected from tags by the caller).

#### Scenario: ShaDescription fields
- **WHEN** a `ShaDescription` is constructed
- **THEN** it has fields `tags`, `repository`, and `date`
- **AND** it does not have fields `sha`, `ref_type`, or `version`

### Requirement: GithubRegistry describe_sha skips ref resolution
The `GithubRegistry` implementation of `describe_sha` SHALL go directly to the commit endpoint (`/commits/{sha}`) to fetch the date, skipping the tag/branch/commit fallback chain used by `lookup_sha`. Tag lookup SHALL use the existing `get_tags_for_sha` mechanism.

#### Scenario: Direct commit lookup (no fallback chain)
- **GIVEN** a valid commit SHA
- **WHEN** `GithubRegistry::describe_sha` is called
- **THEN** only the `/commits/{sha}` endpoint is called for metadata (not `/refs/tags/` or `/refs/heads/`)
- **AND** `get_tags_for_sha` is called for tag discovery

#### Scenario: Tag lookup failure is non-fatal
- **GIVEN** a valid commit SHA
- **AND** the tag lookup fails (network error, rate limit, etc.)
- **WHEN** `describe_sha` is called
- **THEN** the result has `tags = []` (empty, not an error)
- **AND** the result still has the commit date from the direct lookup
