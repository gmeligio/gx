<!-- MODIFIED: domain-composition/spec.md -->
<!-- Change: Commit (ResolvedCommit in spec) repository and date fields use newtypes. Resolution comment updated to VersionComment. -->

### Requirement: ResolvedCommit extracts shared commit metadata

MODIFIED: The `repository` and `date` fields use domain newtypes instead of bare `String`:

```
ResolvedCommit { sha: CommitSha, repository: Repository, ref_type: Option<RefType>, date: CommitDate }
```

Note: The spec names this struct `ResolvedCommit`; in the current codebase it is `Commit` (in `domain::action::resolved`). The field type changes apply to the actual `Commit` struct. `ResolvedRef` (in `domain::resolution`) also has its `repository` and `date` fields updated to `Repository` and `CommitDate` respectively.

#### Scenario: Lock actions tier uses Commit with newtypes
- **GIVEN** the lock's actions tier maps `ActionKey` → `Commit`
- **THEN** `Commit::repository` SHALL be `Repository` (not `String`)
- **AND** `Commit::date` SHALL be `CommitDate` (not `String`)

#### Scenario: Resolution comment updated to VersionComment
- **GIVEN** the lock's `Resolution` struct has a `comment` field
- **THEN** `Resolution::comment` SHALL be `VersionComment` (not `String`)
- **AND** `Resolution.version` remains `Version` (unchanged)
- **NOTE** The main spec's `Entry { commit: ResolvedCommit, version: Option<String>, comment: String }` is aspirational naming. In the codebase, `Resolution` holds the `comment` field, and `Commit` holds commit metadata. This MODIFIED marker covers the `comment` field change (task 4).

#### Scenario: Lock file TOML format unchanged by newtypes
- **WHEN** a `Commit` with `Repository` and `CommitDate` newtypes is serialized to TOML
- **THEN** the fields `repository` and `date` SHALL appear as plain string values in the TOML table
- **AND** existing lock files SHALL parse and roundtrip identically
