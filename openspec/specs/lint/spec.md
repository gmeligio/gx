## Lint Command

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
The system SHALL allow each rule's severity to be set to `error`, `warn`, or `off` via the `[lint.rules]` section in `gx.toml`. Rule names are validated at parse time — unrecognized rule names produce a deserialization error.

#### Scenario: Unrecognized rule name in config
- **GIVEN** `gx.toml` contains `sha-missmatch = { level = "error" }` (typo)
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL fail with an error identifying the unrecognized rule name
- **BECAUSE** rule names are deserialized as a `RuleName` enum via `#[serde(rename_all = "kebab-case")]` in `LintData.rules` (`infra::manifest::convert`), not as arbitrary strings

#### Scenario: All valid rule names accepted
- **GIVEN** `gx.toml` contains any combination of `sha-mismatch`, `unpinned`, `stale-comment`, `unsynced-manifest` in `[lint.rules]`
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL succeed and each rule's configured level is applied

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

### Requirement: RuleName enum identifies rules

The `RuleName` enum SHALL be the canonical identifier for lint rules. It implements `Display` (kebab-case output), `FromStr` (kebab-case input), and serde support.

#### Scenario: RuleName FromStr with valid name
- **GIVEN** the string `"sha-mismatch"`
- **WHEN** `RuleName::from_str` is called
- **THEN** it SHALL return `Ok(RuleName::ShaMismatch)`

#### Scenario: RuleName FromStr with invalid name
- **GIVEN** the string `"nonexistent-rule"`
- **WHEN** `RuleName::from_str` is called
- **THEN** it SHALL return `Err` with a message describing the unrecognized rule name

#### Scenario: RuleName Display roundtrips with FromStr
- **GIVEN** any `RuleName` variant
- **WHEN** formatted with `Display` and parsed back with `FromStr`
- **THEN** the result SHALL equal the original variant

---

## Lint Rules

### Requirement: sha-mismatch rule
The system SHALL detect when a workflow file references a SHA that differs from what the lock file specifies for that action and version.

#### Scenario: Workflow SHA matches lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4` and `gx.lock` maps `actions/checkout` v4 to `abc123`
- **WHEN** `sha-mismatch` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Workflow SHA differs from lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4` and `gx.lock` maps `actions/checkout` v4 to `def456`
- **WHEN** `sha-mismatch` rule runs
- **THEN** an error diagnostic is produced identifying the file, action, expected SHA, and actual SHA

### Requirement: unpinned rule
The system SHALL detect when a workflow file references an action using a tag (e.g., `@v4`) instead of a SHA-pinned reference (e.g., `@abc123 # v4`).

#### Scenario: Action is SHA-pinned
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4`
- **WHEN** `unpinned` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Action uses tag reference
- **GIVEN** `ci.yml` has `actions/checkout@v4`
- **WHEN** `unpinned` rule runs
- **THEN** an error diagnostic is produced identifying the file and action

### Requirement: unsynced-manifest rule
The system SHALL detect when the set of actions in workflows does not match the set of actions in the manifest.

#### Scenario: Action in workflow but not in manifest
- **GIVEN** `ci.yml` uses `actions/cache` but `gx.toml` does not list `actions/cache`
- **WHEN** `unsynced-manifest` rule runs
- **THEN** an error diagnostic is produced: action found in workflow but missing from manifest

#### Scenario: Action in manifest but not in any workflow
- **GIVEN** `gx.toml` lists `actions/setup-go` but no workflow file uses it
- **WHEN** `unsynced-manifest` rule runs
- **THEN** an error diagnostic is produced: action in manifest but unused in workflows

#### Scenario: Manifest and workflows are in sync
- **GIVEN** every action in `gx.toml` appears in at least one workflow and vice versa
- **WHEN** `unsynced-manifest` rule runs
- **THEN** no diagnostic is produced

### Requirement: stale-comment rule
The system SHALL detect when a version comment in a workflow file does not match the version that the lock file associates with that SHA.

#### Scenario: Comment matches lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4` and `gx.lock` confirms `abc123` resolves to `v4`
- **WHEN** `stale-comment` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Comment does not match lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v3` but `gx.lock` maps `abc123` to `v4`
- **WHEN** `stale-comment` rule runs
- **THEN** a warn diagnostic is produced identifying the file, action, stated version, and actual version

#### Scenario: No comment present
- **GIVEN** `ci.yml` has `actions/checkout@abc123` (no version comment)
- **WHEN** `stale-comment` rule runs
- **THEN** no diagnostic is produced (nothing to validate)

---

## Lint Output

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

### Requirement: Comprehensive integration test coverage
The system SHALL have integration tests covering all four rules, mixed severity diagnostics, config overrides, and edge cases.

#### Scenario: Mixed severity output tested
- **GIVEN** a workflow that triggers both error and warn diagnostics
- **WHEN** lint runs
- **THEN** both are reported and exit code is 1 (due to errors)

#### Scenario: Warning-only produces exit code 0
- **GIVEN** a workflow that triggers only warn diagnostics (all error rules disabled)
- **WHEN** lint runs
- **THEN** warnings are reported and exit code is 0
