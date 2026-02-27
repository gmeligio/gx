### Requirement: Upgrade a single action in safe mode
The system SHALL allow upgrading a single named action within its current major version, leaving all other actions in the manifest unchanged.

#### Scenario: Safe upgrade of a single action
- **WHEN** user runs `gx upgrade actions/checkout`
- **THEN** only `actions/checkout` is checked and upgraded within its current major
- **THEN** all other actions in the manifest are untouched

#### Scenario: Single action not found in manifest
- **WHEN** user runs `gx upgrade actions/nonexistent`
- **THEN** the command exits with an error indicating the action is not in the manifest

#### Scenario: Single action already at latest within major
- **WHEN** user runs `gx upgrade actions/checkout` and no newer version exists within the current major
- **THEN** the command reports the action is up to date

### Requirement: Upgrade a single action to latest version
The system SHALL allow upgrading a single named action to the absolute latest version (including major version bumps), leaving all other actions in the manifest unchanged.

#### Scenario: Latest upgrade of a single action
- **WHEN** user runs `gx upgrade --latest actions/checkout`
- **THEN** only `actions/checkout` is upgraded to the highest available version across all majors
- **THEN** all other actions in the manifest are untouched

#### Scenario: Single action already at absolute latest
- **WHEN** user runs `gx upgrade --latest actions/checkout` and no newer version exists
- **THEN** the command reports the action is up to date

### Requirement: Reject combining --latest with an exact version pin
The system SHALL reject the combination of `--latest` and an `ACTION@VERSION` argument with a clear error message.

#### Scenario: --latest with ACTION@VERSION is rejected
- **WHEN** user runs `gx upgrade --latest actions/checkout@v5`
- **THEN** the command exits with an error explaining that `--latest` cannot be combined with an exact version pin

### Requirement: Scoped upgrade only modifies global manifest entry
The system SHALL only upgrade the global manifest entry for the targeted action. Per-workflow, per-job, and per-step overrides SHALL NOT be modified by a scoped upgrade.

#### Scenario: Override entries are preserved during single-action upgrade
- **WHEN** user runs `gx upgrade actions/checkout` and the manifest has a workflow-level override for `actions/checkout`
- **THEN** the global entry is upgraded
- **THEN** the workflow-level override is unchanged
