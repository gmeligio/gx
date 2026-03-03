## MODIFIED Requirements

### Requirement: resolve_from_sha derives all lock fields from SHA
`ActionResolver` SHALL provide a `resolve_from_sha` method that takes an `ActionId` and `CommitSha` and returns a `ResolvedAction` with version, ref_type, and date derived from the SHA. The method SHALL use `describe_sha` to obtain tags and metadata in a single registry operation, rather than calling `lookup_sha` and `tags_for_sha` separately.

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

#### Scenario: describe_sha error propagates through resolve_from_sha
- **GIVEN** `describe_sha` returns an error (e.g., `TokenRequired`)
- **WHEN** `resolve_from_sha` is called
- **THEN** the error is propagated to the caller
