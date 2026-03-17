## Upgrade Operations

Users upgrade GitHub Actions safely and precisely using `gx upgrade`.

### Requirement: Upgrade a single action within its current major version

The system SHALL allow upgrading a single named action within its current major version, leaving all other actions in the manifest unchanged.

#### Scenario: Safe upgrade of a single action
- **WHEN** user runs `gx upgrade actions/checkout`
- **THEN** only `actions/checkout` is checked and upgraded within its current major
- **AND** all other actions in the manifest are untouched

#### Scenario: Single action not found in manifest
- **WHEN** user runs `gx upgrade actions/nonexistent`
- **THEN** the command exits with an error indicating the action is not in the manifest

#### Scenario: Single action already at latest within major
- **WHEN** user runs `gx upgrade actions/checkout` and no newer version exists within the current major
- **THEN** the command reports the action is up to date

### Requirement: Upgrade a single action to the absolute latest version

The system SHALL allow upgrading a single named action to the highest available version across all majors (including major version bumps), leaving all other actions in the manifest unchanged.

#### Scenario: Latest upgrade of a single action
- **WHEN** user runs `gx upgrade --latest actions/checkout`
- **THEN** only `actions/checkout` is upgraded to the highest available version across all majors
- **AND** all other actions in the manifest are untouched

### Requirement: Pin a single action to an exact version

The system SHALL allow pinning a single named action to a specific version.

#### Scenario: Pinned upgrade of a single action
- **WHEN** user runs `gx upgrade actions/checkout@v5`
- **THEN** `actions/checkout` is set to exactly `v5`
- **AND** all other actions in the manifest are untouched

### Requirement: Reject combining --latest with an exact version pin

The system SHALL reject the combination of `--latest` and an `ACTION@VERSION` argument with a clear error message.

#### Scenario: --latest with ACTION@VERSION is rejected
- **WHEN** user runs `gx upgrade --latest actions/checkout@v5`
- **THEN** the command exits with an error explaining that `--latest` cannot be combined with an exact version pin

### Requirement: Scoped upgrade only modifies the global manifest entry

A single-action upgrade SHALL only modify the global manifest entry. Per-workflow, per-job, and per-step overrides SHALL NOT be modified.

#### Scenario: Override entries are preserved during single-action upgrade
- **GIVEN** the manifest has a workflow-level override for `actions/checkout`
- **WHEN** user runs `gx upgrade actions/checkout`
- **THEN** the global entry is upgraded
- **AND** the workflow-level override is unchanged

---

### Guardrail: Every flag combination the CLI accepts produces a valid upgrade operation without runtime panics

All flag and argument combinations that pass CLI validation SHALL produce a valid upgrade operation directly. There SHALL be no runtime failure path where a syntactically accepted combination causes a panic or unexpected crash.

#### Scenario: Safe upgrade of all actions is always constructible
- **WHEN** user runs `gx upgrade` with no flags and no action argument
- **THEN** the operation is constructed and executed without error

#### Scenario: Pinning to a version is always constructible
- **WHEN** user runs `gx upgrade actions/checkout@v5`
- **THEN** the operation is constructed and executed without error
- **AND** no runtime validation is needed beyond CLI argument parsing

#### Scenario: Pinning all actions to a single version is unrepresentable
- **WHEN** a user provides a version pin without naming a specific action
- **THEN** the CLI rejects this at argument parsing time
- **AND** the system never attempts to construct an invalid operation internally
