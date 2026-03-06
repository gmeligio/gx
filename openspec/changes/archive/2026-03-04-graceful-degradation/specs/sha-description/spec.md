## MODIFIED Requirements

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

#### Scenario: Auth required error propagates
- **GIVEN** no GitHub token is configured
- **AND** the API returns 401 or 403 (private repository)
- **WHEN** `describe_sha` is called
- **THEN** the result is `Err(ResolutionError::AuthRequired)`

#### Scenario: Rate limited error propagates
- **GIVEN** the API returns 429 or 403 with exhausted rate limit
- **WHEN** `describe_sha` is called
- **THEN** the result is `Err(ResolutionError::RateLimited)`
