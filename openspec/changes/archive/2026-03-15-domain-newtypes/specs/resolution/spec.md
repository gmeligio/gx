<!-- MODIFIED: resolution/spec.md -->
<!-- Change: ShaDescription and ResolvedRef repository and date fields use newtypes instead of String. -->

### Requirement: ShaDescription carries commit metadata

MODIFIED: The `ShaDescription` struct SHALL contain `tags: Vec<Version>`, `repository: Repository`, and `date: CommitDate`. It SHALL NOT contain `sha`, `ref_type`, or `version` (these are derived by the caller).

#### Scenario: ShaDescription fields use domain newtypes
- **GIVEN** a `ShaDescription` returned from `describe_sha`
- **WHEN** the caller accesses `repository`
- **THEN** it SHALL be a `Repository` newtype (not a bare `String`)
- **AND** `date` SHALL be a `CommitDate` newtype (not a bare `String`)
- **AND** the TOML serialization boundary in the infra layer converts between `String` and these newtypes

### Requirement: ResolvedRef carries commit metadata

MODIFIED: `ResolvedRef` SHALL have its `repository` and `date` fields updated to `Repository` and `CommitDate` newtypes respectively.

#### Scenario: ResolvedRef fields use domain newtypes
- **GIVEN** a `ResolvedRef` returned from resolution
- **THEN** `ResolvedRef::repository` SHALL be `Repository` (not `String`)
- **AND** `ResolvedRef::date` SHALL be `CommitDate` (not `String`)
