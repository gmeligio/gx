## ADDED Requirements

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
