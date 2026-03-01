### Requirement: Lock file v1.1 entry format
Each action entry in the lock file SHALL be a TOML inline table with four fields: `sha`, `repository`, `ref_type`, and `date`.

#### Scenario: Standard lock entry with all fields
- **GIVEN** an action `actions/checkout@v6` resolved to SHA `de0fac2e...` from a GitHub Release published at `2026-02-15T10:35:00Z`
- **THEN** the lock file entry is:
  ```toml
  "actions/checkout@v6" = { sha = "de0fac2e...", repository = "actions/checkout", ref_type = "release", date = "2026-02-15T10:35:00Z" }
  ```

#### Scenario: Subpath action stores base repository
- **GIVEN** an action `github/codeql-action/upload-sarif@v3` resolved against repository `github/codeql-action`
- **THEN** the `repository` field is `"github/codeql-action"`

#### Scenario: Lock file version is 1.1
- **THEN** the lock file has `version = "1.1"` at the top level

### Requirement: ref_type field values
The `ref_type` field SHALL be one of four string values indicating what the user's ref resolved to.

#### Scenario: Tag with a GitHub Release
- **GIVEN** the ref `v6` resolves to a tag that has an associated GitHub Release
- **THEN** `ref_type = "release"`

#### Scenario: Tag without a GitHub Release
- **GIVEN** the ref `v1` resolves to a tag with no associated GitHub Release
- **THEN** `ref_type = "tag"`

#### Scenario: Branch ref
- **GIVEN** the ref `main` resolves to a branch
- **THEN** `ref_type = "branch"`

#### Scenario: Direct commit SHA
- **GIVEN** the ref is a 40-character hex SHA
- **THEN** `ref_type = "commit"`

### Requirement: date field semantics by ref_type
The `date` field SHALL contain an RFC 3339 timestamp whose source depends on `ref_type`.

#### Scenario: Release date uses published_at
- **GIVEN** `ref_type = "release"`
- **THEN** `date` is the GitHub Release `published_at` value

#### Scenario: Annotated tag date uses tagger.date
- **GIVEN** `ref_type = "tag"` and the tag is annotated
- **THEN** `date` is the Git tag object's `tagger.date` value

#### Scenario: Lightweight tag date uses committer.date
- **GIVEN** `ref_type = "tag"` and the tag is lightweight (not annotated)
- **THEN** `date` is the commit's `committer.date` value

#### Scenario: Branch date uses committer.date
- **GIVEN** `ref_type = "branch"`
- **THEN** `date` is the commit's `committer.date` value

#### Scenario: Commit date uses committer.date
- **GIVEN** `ref_type = "commit"`
- **THEN** `date` is the commit's `committer.date` value

### Requirement: Migration from v1.0 to v1.1
The system SHALL transparently migrate v1.0 lock files to v1.1 format with inline tables.

#### Scenario: Migration with GITHUB_TOKEN available
- **GIVEN** a v1.0 lock file with plain string SHA values
- **WHEN** the lock file is loaded and `GITHUB_TOKEN` is set
- **THEN** each entry is enriched by fetching metadata from GitHub
- **THEN** the file is rewritten in v1.1 format with inline tables

#### Scenario: Migration without GITHUB_TOKEN
- **GIVEN** a v1.0 lock file and no `GITHUB_TOKEN`
- **WHEN** the lock file is loaded
- **THEN** each entry is populated with defaults: `repository` from `ActionId::base_repo()`, `ref_type = "tag"`, `date = ""`
- **THEN** the file is rewritten in v1.1 format with inline tables
- **THEN** a warning is logged

### Requirement: Roundtrip integrity
Lock file serialization and deserialization SHALL be lossless.

#### Scenario: Save and reload preserves all fields
- **GIVEN** a lock with entries containing sha, repository, ref_type, and date
- **WHEN** saved to disk and reloaded
- **THEN** all field values are identical
