## MODIFIED Requirements

### Requirement: SHA-first lock resolution from workflow SHAs
When workflow files pin an action to a SHA, lock resolution SHALL use that SHA directly to build the lock entry. The version, ref_type, and date fields SHALL be derived from the SHA, not from resolving the manifest version tag via the registry.

#### Scenario: Init with SHA-pinned action
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: jdx/mise-action@6d1e696... # v3`
- **AND** the registry reports that `6d1e696...` points to tags `[v3, v3.6, v3.6.1]`
- **WHEN** tidy (init) runs
- **THEN** the manifest adds `jdx/mise-action = "v3"` (from the comment)
- **AND** the lock entry has `sha = "6d1e696..."` (the workflow SHA, not a freshly resolved one)
- **AND** the lock entry has `version = "v3.6.1"` (most specific tag for that SHA)
- **AND** the lock entry has `specifier = "^3"` (derived from manifest version `v3`)

#### Scenario: Fallback to version-based resolution when no workflow SHA
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: actions/checkout@v4` (no SHA pin)
- **WHEN** tidy (init) runs
- **THEN** the lock entry SHA is obtained from the registry by resolving the `v4` tag

#### Scenario: Existing lock entry is not re-resolved
- **GIVEN** the lock already has a complete entry for `(actions/checkout, v4)`
- **WHEN** tidy runs
- **THEN** no registry call is made for that entry (workflow SHA is not used to overwrite)

#### Scenario: Recoverable error in SHA-first path degrades gracefully
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: actions/checkout@abc123... # v4`
- **AND** `describe_sha` returns `RateLimited` or `AuthRequired`
- **AND** fallback `resolve(spec)` also returns a recoverable error
- **WHEN** tidy (init) runs
- **THEN** the action is skipped with a warning (not a hard error)
- **AND** the lock file is written without that entry

#### Scenario: Strict error in resolution is still a hard failure
- **GIVEN** no manifest or lock exists
- **AND** workflows have `uses: nonexistent/action@v1`
- **AND** resolution returns `ResolveFailed` (404 not found)
- **WHEN** tidy (init) runs
- **THEN** the command fails with `TidyError::ResolutionFailed`
