## ADDED Requirements

### Requirement: Version correction is separate from metadata resolution
The `ActionResolver` SHALL provide version correction as a standalone operation that returns only a version, not a `ResolvedAction`. Metadata resolution SHALL always go through `resolve()`.

#### Scenario: Version correction returns version only
- **WHEN** `correct_version(id, sha)` is called with a SHA that points to tags `[v6, v6.0.1]`
- **AND** the action's manifest version is `v6`
- **THEN** the result is `(v6, false)` indicating no correction was needed

#### Scenario: Version correction detects mismatch
- **WHEN** `correct_version(id, sha)` is called with a SHA that points to tags `[v5, v5.0.0]`
- **AND** the action's manifest version is `v4`
- **THEN** the result is `(v5, true)` indicating the version was corrected

#### Scenario: Version correction degrades gracefully without token
- **WHEN** `correct_version(id, sha)` is called without a GITHUB_TOKEN
- **THEN** the result is `(original_version, false)` — the original version is kept unchanged

### Requirement: All lock entries flow through resolve
Every action stored in the lock SHALL have its metadata populated via `resolve()` → `lookup_sha()`. There SHALL be no code path that constructs a `ResolvedAction` with hardcoded `ref_type` or `date` defaults.

#### Scenario: SHA-pinned workflow action gets full metadata
- **WHEN** a workflow has `uses: actions/checkout@de0fac2e... # v6`
- **AND** `gx tidy` processes this action
- **THEN** the lock entry has `ref_type` and `date` populated from the GitHub API (not hardcoded defaults)

#### Scenario: Tag-only workflow action gets full metadata
- **WHEN** a workflow has `uses: actions/checkout@v6`
- **AND** `gx tidy` processes this action
- **THEN** the lock entry has `ref_type` and `date` populated from the GitHub API

### Requirement: SHA-pinned actions keep the workflow SHA
When a workflow already has a SHA-pinned action, the lock entry SHALL use the workflow's SHA, not the SHA that `lookup_sha()` returns for the version tag.

#### Scenario: Tag moved but workflow SHA preserved
- **GIVEN** workflow has `uses: actions/checkout@abc123... # v6`
- **AND** the `v6` tag now points to a different commit `def456...`
- **WHEN** `gx tidy` runs
- **THEN** the lock entry SHA is `abc123...` (from the workflow, not the registry)
- **AND** the lock entry `ref_type` and `date` come from the registry's resolution of `v6`

## MODIFIED Requirements

### Requirement: Resolution returns ref_type and date
The version registry SHALL return the ref_type, repository, and date alongside the commit SHA when resolving a ref.

#### Scenario: Tag with a GitHub Release
- **WHEN** resolving `actions/checkout@v6`
- **AND** `/git/ref/tags/v6` succeeds
- **AND** `/releases/tags/v6` returns a release with `published_at`
- **THEN** the result contains `ref_type = Release`, the release's `published_at` as `date`, and `repository = "actions/checkout"`

#### Scenario: Tag without a GitHub Release (annotated tag)
- **WHEN** resolving `some/action@v1`
- **AND** `/git/ref/tags/v1` succeeds and points to a tag object
- **AND** no release exists for `v1`
- **THEN** the result contains `ref_type = Tag`, the tag's `tagger.date` as `date`

#### Scenario: Tag without a GitHub Release (lightweight tag)
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

#### Scenario: Commit date parsed from correct JSON nesting
- **WHEN** fetching commit date from `GET /repos/{owner}/{repo}/commits/{sha}`
- **THEN** the date is read from `commit.committer.date` (nested under the `commit` object)
- **AND** NOT from the top-level `committer` field (which is a GitHub user object without a date)

### Requirement: ResolvedAction carries metadata
The `ResolvedAction` struct SHALL include `repository`, `ref_type`, and `date` so that `Lock::set()` can store the full entry.

#### Scenario: Resolution flows through to lock
- **WHEN** `ActionResolver::resolve()` returns a `Resolved` result
- **AND** `lock.set(&resolved_action)` is called
- **THEN** the lock entry contains all four fields (sha, repository, ref_type, date)

#### Scenario: ResolvedAction supports SHA override
- **GIVEN** a `ResolvedAction` produced by `resolve()`
- **WHEN** `with_sha(new_sha)` is called
- **THEN** a new `ResolvedAction` is returned with the SHA replaced and all other fields preserved
