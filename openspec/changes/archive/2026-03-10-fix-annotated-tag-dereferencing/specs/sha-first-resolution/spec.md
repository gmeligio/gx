## ADDED Requirements

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
