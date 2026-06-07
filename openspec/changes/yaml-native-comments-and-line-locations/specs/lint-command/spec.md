## MODIFIED Requirements

### Requirement: Clean diagnostic output

The system SHALL print lint diagnostics as the primary output with clear severity prefixes
and a summary line. For a violation that maps to a single workflow source line, the
diagnostic SHALL include that line as `file:line`; for a violation with no single source
line (manifest-level or whole-file), the diagnostic SHALL render the file location without a
line, as before.

The user who benefits is the workflow maintainer running `gx lint` across a repository with
many workflows: instead of being told only *which file* is wrong and having to eyeball it,
they get a clickable `file:line` that takes them straight to the offending `uses:` line.

#### Scenario: Lint violations produce clean output
- **WHEN** user runs `gx lint` and violations are found
- **THEN** diagnostics are printed with `[error]` or `[warn]` prefixes
- **AND** a summary line shows total count, error count, and warning count
- **AND** the process exits with code 1
- **AND** no internal error wrapper message is printed

#### Scenario: Located violation includes the source line
- **GIVEN** a workflow with an unpinned `uses:` on line 12
- **WHEN** user runs `gx lint`
- **THEN** the diagnostic for that violation renders the location as `<workflow>:12`

#### Scenario: Two identical uses lines keep distinct locations
- **GIVEN** a workflow where the same action appears in two steps on different lines
- **WHEN** user runs `gx lint` and both steps violate a rule
- **THEN** each diagnostic renders its own step's line, not a single shared line

#### Scenario: Manifest-level violation renders without a line
- **WHEN** user runs `gx lint` and a violation has no single workflow source line (e.g. a
  manifest/workflow set mismatch)
- **THEN** the diagnostic renders the file location without a `:line` suffix

#### Scenario: Lint I/O error produces error message
- **WHEN** user runs `gx lint` and a workflow file cannot be read
- **THEN** an error message describing the I/O failure is printed
- **AND** the process exits with a non-zero code

#### Scenario: Mixed severity output
- **GIVEN** a workflow that triggers both error and warn diagnostics
- **WHEN** user runs `gx lint`
- **THEN** both are reported and exit code is 1 (due to errors)

#### Scenario: Warning-only produces exit code 0
- **GIVEN** a workflow that triggers only warn diagnostics (all error rules disabled)
- **WHEN** user runs `gx lint`
- **THEN** warnings are reported and exit code is 0
