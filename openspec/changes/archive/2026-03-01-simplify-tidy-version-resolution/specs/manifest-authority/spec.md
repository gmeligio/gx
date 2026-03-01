## ADDED Requirements

### Requirement: Manifest is source of truth for existing action versions
When tidy runs and the manifest already tracks an action, the manifest version SHALL NOT be overwritten by workflow state. Tidy SHALL resolve the manifest version to a SHA via the registry and update the workflow to match.

#### Scenario: Manifest has v4, workflow has stale v3 SHA
- **GIVEN** manifest has `actions/checkout = "v4"`
- **AND** all workflows have `uses: actions/checkout@abc123 # v3` where abc123 is v3's SHA
- **WHEN** tidy runs
- **THEN** the manifest keeps `actions/checkout = "v4"`
- **AND** the lock resolves v4 to its correct SHA from the registry
- **AND** workflow files are updated to `uses: actions/checkout@<v4-sha> # v4`

#### Scenario: Manifest has v4, workflow has v4 SHA (already consistent)
- **GIVEN** manifest has `actions/checkout = "v4"`
- **AND** all workflows have `uses: actions/checkout@def456 # v4` where def456 is v4's SHA
- **WHEN** tidy runs
- **THEN** the manifest keeps `actions/checkout = "v4"`
- **AND** no changes are made to workflow files

#### Scenario: Manifest has v4, workflows have mixed v3 and v5
- **GIVEN** manifest has `actions/checkout = "v4"`
- **AND** ci.yml has `uses: actions/checkout@abc123 # v3`
- **AND** release.yml has `uses: actions/checkout@def456 # v5`
- **WHEN** tidy runs
- **THEN** the manifest keeps `actions/checkout = "v4"` as the global version
- **AND** overrides are recorded for ci.yml (v3) and release.yml (v5)

### Requirement: Workflows are source of truth for new actions
When tidy discovers an action in workflows that is not yet in the manifest, the dominant workflow version SHALL be used as the initial manifest version.

#### Scenario: New action with single version across workflows
- **GIVEN** the manifest does not track `actions/setup-node`
- **AND** all workflows use `actions/setup-node@abc123 # v4`
- **WHEN** tidy runs
- **THEN** the manifest adds `actions/setup-node = "v4"`

#### Scenario: New action with multiple versions picks dominant
- **GIVEN** the manifest does not track `actions/setup-node`
- **AND** three workflows use `actions/setup-node@abc123 # v4`
- **AND** one workflow uses `actions/setup-node@def456 # v3`
- **WHEN** tidy runs
- **THEN** the manifest adds `actions/setup-node = "v4"` (most-used)
- **AND** an override is recorded for the workflow using v3

### Requirement: SHA correction applies only to new actions
When a new action is added to the manifest, tidy SHALL verify the workflow tag against the registry using the workflow's SHA. This correction SHALL NOT apply to actions already present in the manifest.

#### Scenario: New action with wrong comment tag
- **GIVEN** the manifest does not track `actions/checkout`
- **AND** workflows have `uses: actions/checkout@abc123 # v4`
- **AND** the registry reports that abc123 points to tags `[v3.6.0, v3]`
- **WHEN** tidy runs
- **THEN** the manifest adds `actions/checkout = "v3.6.0"` (corrected via registry)

#### Scenario: SHA correction does not apply to existing actions
- **GIVEN** the manifest has `actions/checkout = "v4"`
- **AND** workflows have `uses: actions/checkout@abc123 # v3`
- **AND** abc123 points to v3 tags in the registry
- **WHEN** tidy runs
- **THEN** the manifest keeps `actions/checkout = "v4"` (existing, not corrected)

### Requirement: SHA-to-tag upgrade via registry
When the manifest has a raw SHA as an action version, tidy SHALL attempt to upgrade it to a human-readable tag using the registry.

#### Scenario: SHA upgraded to tag with registry
- **GIVEN** manifest has `actions/checkout = "abc123def456789012345678901234567890abcd"`
- **AND** the registry reports that SHA points to tags `[v4, v4.2.1]`
- **WHEN** tidy runs
- **THEN** the manifest is updated to `actions/checkout = "v4.2.1"` (best tag)

#### Scenario: SHA stays when no token available
- **GIVEN** manifest has `actions/checkout = "abc123def456789012345678901234567890abcd"`
- **AND** no GITHUB_TOKEN is configured
- **WHEN** tidy runs
- **THEN** the manifest keeps the SHA version unchanged

#### Scenario: SHA stays when registry returns no tags
- **GIVEN** manifest has `actions/checkout = "abc123def456789012345678901234567890abcd"`
- **AND** the registry returns an empty tag list for that SHA
- **WHEN** tidy runs
- **THEN** the manifest keeps the SHA version unchanged

### Requirement: Workflows are source of truth for action removal
When an action exists in the manifest but no longer appears in any workflow, tidy SHALL remove it from the manifest.

#### Scenario: Action removed from all workflows
- **GIVEN** manifest has `actions/old-action = "v1"`
- **AND** no workflow references `actions/old-action`
- **WHEN** tidy runs
- **THEN** the manifest removes `actions/old-action`

### Requirement: Lock resolution uses registry exclusively
Lock entries SHALL be resolved from the registry or existing cached lock entries. Workflow SHAs SHALL NOT be injected into lock resolution.

#### Scenario: Lock resolves from registry for new version
- **GIVEN** manifest has `actions/checkout = "v4"`
- **AND** the lock has no entry for `(actions/checkout, v4)`
- **WHEN** tidy resolves the lock
- **THEN** the SHA is obtained from the registry, not from the workflow files

#### Scenario: Lock uses cached entry when complete
- **GIVEN** manifest has `actions/checkout = "v4"`
- **AND** the lock already has a complete entry for `(actions/checkout, v4)`
- **WHEN** tidy resolves the lock
- **THEN** no registry call is made for that entry

### Requirement: Workflow output omits comment for SHA-only versions
When the manifest version is a raw SHA (not a tag), workflow files SHALL be written with just the SHA ref, without a trailing comment.

#### Scenario: Tag version produces comment
- **GIVEN** manifest version is `v4` and lock SHA is `abc123`
- **WHEN** tidy writes the workflow reference
- **THEN** the output is `abc123 # v4`

#### Scenario: SHA version produces no comment
- **GIVEN** manifest version is `abc123def456789012345678901234567890abcd`
- **AND** the lock SHA is the same value
- **WHEN** tidy writes the workflow reference
- **THEN** the output is `abc123def456789012345678901234567890abcd` (no `#` comment)
