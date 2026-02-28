## ADDED Requirements

### Requirement: Run lint checks with gx lint
The system SHALL provide a `gx lint` subcommand that validates workflows against the manifest and lock file without modifying any files.

#### Scenario: Clean repo with no issues
- **WHEN** user runs `gx lint` and all workflows are in sync with manifest and lock
- **THEN** the command exits with code 0

#### Scenario: Errors detected
- **WHEN** user runs `gx lint` and one or more rules produce error-level diagnostics
- **THEN** the command prints all diagnostics and exits with code 1

#### Scenario: Warnings only
- **WHEN** user runs `gx lint` and rules produce only warn-level diagnostics (no errors)
- **THEN** the command prints all diagnostics and exits with code 0

#### Scenario: No manifest file exists
- **WHEN** user runs `gx lint` and `gx.toml` does not exist
- **THEN** the command exits with code 0 (nothing to lint)

### Requirement: Zero-config runs all rules at defaults
The system SHALL run all built-in rules at their hardcoded default levels when no `[lint.rules]` section is present in `gx.toml`.

#### Scenario: No lint config
- **GIVEN** `gx.toml` has an `[actions]` section but no `[lint]` section
- **WHEN** user runs `gx lint`
- **THEN** all rules run at their default levels (`sha-mismatch`=error, `unpinned`=error, `unsynced-manifest`=error, `stale-comment`=warn)

### Requirement: Configure rule severity
The system SHALL allow each rule's severity to be set to `error`, `warn`, or `off` via the `[lint.rules]` section in `gx.toml`.

#### Scenario: Disable a rule
- **GIVEN** `gx.toml` contains `stale-comment = { level = "off" }`
- **WHEN** user runs `gx lint`
- **THEN** the `stale-comment` rule does not run and produces no diagnostics

#### Scenario: Promote a rule to error
- **GIVEN** `gx.toml` contains `stale-comment = { level = "error" }`
- **WHEN** user runs `gx lint` and stale comments exist
- **THEN** stale comment diagnostics are reported as errors and the command exits with code 1

### Requirement: Ignore targets for rules
The system SHALL support ignore entries in rule configuration using typed keys: `action`, `workflow`, and `job`. Multiple keys in a single entry compose as intersection (narrowing scope).

#### Scenario: Ignore a specific action
- **GIVEN** `unpinned` rule has `ignore = [{ action = "actions/internal-tool" }]`
- **WHEN** user runs `gx lint` and `actions/internal-tool` is unpinned in a workflow
- **THEN** no diagnostic is produced for `actions/internal-tool`
- **THEN** other unpinned actions still produce diagnostics

#### Scenario: Ignore scoped to workflow and job
- **GIVEN** `sha-mismatch` rule has `ignore = [{ action = "actions/checkout", workflow = ".github/workflows/legacy.yml", job = "compat" }]`
- **WHEN** `actions/checkout` has a SHA mismatch in `legacy.yml` job `compat`
- **THEN** no diagnostic is produced for that specific location
- **WHEN** `actions/checkout` has a SHA mismatch in `legacy.yml` job `build`
- **THEN** a diagnostic IS produced (different job, not covered by ignore)

#### Scenario: Ignore scoped to workflow only
- **GIVEN** `unpinned` rule has `ignore = [{ workflow = ".github/workflows/experimental.yml" }]`
- **WHEN** any action is unpinned in `experimental.yml`
- **THEN** no diagnostic is produced for actions in that workflow
