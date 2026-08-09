## MODIFIED Requirements

### Requirement: Zero-config runs all rules at defaults

The system SHALL run all built-in rules at their hardcoded default levels when no
`[lint.rules]` section is present in `gx.toml`. The default set covers both
action-hygiene rules and workflow-security rules, and SHALL cover every rule the
system implements — a rule that runs but is absent from this set would leave the
user unable to predict, from the specification, what a zero-config run does.

#### Scenario: No lint config

- **GIVEN** `gx.toml` has an `[actions]` section but no `[lint]` section
- **WHEN** user runs `gx lint`
- **THEN** all rules run at their default levels:
  - `sha-mismatch` = error
  - `unpinned` = error
  - `unsynced-manifest` = error
  - `stale-comment` = warn
  - `missing-permissions` = error
  - `excessive-permissions` = error
  - `dangerous-trigger` = error
  - `pr-head-checkout` = error
  - `missing-concurrency` = warn
  - `unprotected-secrets` = error
  - `dangling-reference` = error
  - `invalid-expression` = error
  - `run-shellcheck` = warn

#### Scenario: No rule runs without a documented default
- **GIVEN** the complete set of lint rules the system implements
- **WHEN** a zero-config run is performed
- **THEN** every rule that runs SHALL appear in the default set above
- **AND** the check SHALL enumerate rules from the system's own definition, so a
  rule added later cannot silently run at an unstated default

### Requirement: Configure rule severity

The system SHALL allow each rule's severity to be set to `error`, `warn`, or `off`
via the `[lint.rules]` section in `gx.toml`. Unrecognized rule names produce a
parse error.

The set of rule names the system accepts in configuration SHALL be the same set it
prints in diagnostic output, by construction rather than by convention: both SHALL
be derived from a single definition of each rule's name. A rule name SHALL NOT be
acceptable in `gx.toml` but absent from output, nor printed in output but rejected
in `gx.toml`.

**User value:** the maintainer configuring `gx lint` copies a rule name out of the
output — `unpinned`, `run-shellcheck` — and pastes it into `[lint.rules]` to
silence or escalate it. That workflow is only reliable if the two vocabularies are
the same one. When they are maintained as separate hand-written lists, a rule can
be added to output while config still rejects it, and the user's paste fails with
a parse error naming a rule gx itself just printed.

This guardrail is load-bearing: the drift it prevents has already occurred. Three
rules (`dangling-reference`, `invalid-expression`, `run-shellcheck`) reached the
product while this specification's own enumeration of valid names still listed
ten, because the name list was maintained by hand in more than one place.

#### Scenario: A rule's configured name and its reported name are the same string
- **GIVEN** any lint rule the system implements
- **WHEN** the name it is configured by in `[lint.rules]` is compared to the name
  it prints in `gx lint` output
- **THEN** the two SHALL be the same string for every rule
- **AND** changing how a rule's name is written SHALL change both together, so the
  two cannot be given differing values

#### Scenario: A rule reachable in output but not in config is rejected
- **GIVEN** a rule whose reported name and whose accepted config name are made to
  differ
- **WHEN** the system is built and its checks are run
- **THEN** the discrepancy SHALL be caught, rather than shipping a rule that
  `gx lint` names in output but `gx.toml` refuses to configure

#### Scenario: Unrecognized rule name in config
- **GIVEN** `gx.toml` contains `sha-missmatch = { level = "error" }` (typo)
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL fail with an error identifying the unrecognized rule name
- **AND** the error SHALL name the offending key so the user can find the typo
- **NOTE** the exact wording of this error has never been contractual and is not
  asserted here; only that it names the offending key

#### Scenario: All valid rule names accepted
- **GIVEN** `gx.toml` contains any combination of `sha-mismatch`, `unpinned`,
  `stale-comment`, `unsynced-manifest`, `missing-permissions`,
  `excessive-permissions`, `dangerous-trigger`, `pr-head-checkout`,
  `missing-concurrency`, `unprotected-secrets`, `dangling-reference`,
  `invalid-expression`, `run-shellcheck` in `[lint.rules]`
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL succeed and each rule's configured level is applied

#### Scenario: Disable a rule
- **GIVEN** `gx.toml` contains `stale-comment = { level = "off" }`
- **WHEN** user runs `gx lint`
- **THEN** the `stale-comment` rule does not run and produces no diagnostics

#### Scenario: Disable a workflow-security rule

- **GIVEN** `gx.toml` contains `missing-concurrency = { level = "off" }`
- **WHEN** user runs `gx lint`
- **THEN** the `missing-concurrency` rule does not run and produces no diagnostics

#### Scenario: Promote a rule to error
- **GIVEN** `gx.toml` contains `stale-comment = { level = "error" }`
- **WHEN** user runs `gx lint` and stale comments exist
- **THEN** stale comment diagnostics are reported as errors and the command exits
  with code 1
