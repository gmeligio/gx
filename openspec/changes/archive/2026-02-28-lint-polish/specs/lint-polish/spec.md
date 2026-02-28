## MODIFIED Requirements

### Requirement: Clean error output for lint violations
The system SHALL print lint diagnostics as the primary output and set exit code 1 without printing an error type wrapper message.

#### Scenario: Lint violations produce clean output
- **WHEN** user runs `gx lint` and violations are found
- **THEN** diagnostics are printed with `[error]` or `[warn]` prefixes
- **THEN** a summary line shows total count, error count, and warning count
- **THEN** the process exits with code 1
- **THEN** no `Error: App(Manifest(Validation(...)))` wrapper message is printed

#### Scenario: Lint I/O error produces error message
- **WHEN** user runs `gx lint` and a workflow file cannot be read
- **THEN** an error message describing the I/O failure is printed
- **THEN** the process exits with a non-zero code

## ADDED Requirements

### Requirement: stale-comment rule detects mismatched version comments
The system SHALL detect when a workflow file's version comment does not match the SHA-to-version mapping in the lock file.

#### Scenario: Comment matches lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4`
- **GIVEN** `gx.lock` maps `(actions/checkout, v4)` to `abc123`
- **WHEN** `stale-comment` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Comment does not match lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4`
- **GIVEN** `gx.lock` maps `(actions/checkout, v4)` to `def456`
- **WHEN** `stale-comment` rule runs
- **THEN** a warn diagnostic is produced identifying the file, action, comment version, and SHA mismatch

#### Scenario: No comment present
- **GIVEN** `ci.yml` has `actions/checkout@abc123` (no version comment)
- **WHEN** `stale-comment` rule runs
- **THEN** no diagnostic is produced (nothing to validate)

### Requirement: Comprehensive integration test coverage
The system SHALL have integration tests covering all four rules, mixed severity diagnostics, config overrides, and edge cases.

#### Scenario: SHA-mismatch detection tested
- **GIVEN** a workflow with a SHA that doesn't match the lock
- **WHEN** lint runs
- **THEN** sha-mismatch diagnostic is produced

#### Scenario: Mixed severity output tested
- **GIVEN** a workflow that triggers both error and warn diagnostics
- **WHEN** lint runs
- **THEN** both are reported and exit code is 1 (due to errors)

#### Scenario: Warning-only produces exit code 0
- **GIVEN** a workflow that triggers only warn diagnostics (all error rules disabled)
- **WHEN** lint runs
- **THEN** warnings are reported and exit code is 0
