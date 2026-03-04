## ADDED Requirements

### Requirement: ShaIndex accumulates SHA descriptions during a plan run
`ShaIndex` SHALL be a domain entity that stores `ShaDescription` results keyed by `(ActionId, CommitSha)`. It SHALL provide a `get_or_describe` method that returns the stored description if the SHA has already been described, or calls `describe_sha` on the registry, stores the result, and returns it.

#### Scenario: First request for a SHA calls the registry
- **GIVEN** an empty `ShaIndex`
- **AND** action `actions/checkout` with SHA `abc123...`
- **WHEN** `get_or_describe` is called
- **THEN** the registry's `describe_sha` is called
- **AND** the result is stored in the index
- **AND** the result is returned

#### Scenario: Second request for the same SHA does not call the registry
- **GIVEN** a `ShaIndex` that already contains a description for `(actions/checkout, abc123...)`
- **WHEN** `get_or_describe` is called for the same `(actions/checkout, abc123...)`
- **THEN** the registry's `describe_sha` is NOT called
- **AND** the previously stored result is returned

#### Scenario: Different SHAs for the same action are stored separately
- **GIVEN** a `ShaIndex` with a description for `(actions/checkout, abc123...)`
- **WHEN** `get_or_describe` is called for `(actions/checkout, def456...)`
- **THEN** the registry's `describe_sha` is called for the new SHA
- **AND** both descriptions are stored independently

#### Scenario: Registry error propagates through get_or_describe
- **GIVEN** an empty `ShaIndex`
- **AND** the registry returns `Err(TokenRequired)` for `describe_sha`
- **WHEN** `get_or_describe` is called
- **THEN** the error is propagated to the caller
- **AND** the index does not store a result for that SHA

### Requirement: ShaIndex is scoped to a single plan run
A `ShaIndex` SHALL be created at the start of a `plan()` call and discarded when the plan completes. It SHALL NOT persist across separate plan invocations.

#### Scenario: Fresh index per plan run
- **WHEN** `tidy::plan()` is called
- **THEN** a new empty `ShaIndex` is created
- **AND** it is passed to all phases within that plan call
- **AND** it is dropped when the plan returns
