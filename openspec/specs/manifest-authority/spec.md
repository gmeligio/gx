## ADDED Requirements

### Requirement: Manifest is source of truth for existing action versions
When tidy runs and the manifest already tracks an action, the manifest specifier SHALL NOT be overwritten by workflow state. Tidy SHALL resolve the manifest specifier to a SHA via the registry and update the workflow to match.

#### Scenario: Manifest has ^4, workflow has stale v3 SHA
- **GIVEN** manifest has `actions/checkout = "^4"`
- **AND** all workflows have `uses: actions/checkout@abc123 # v3` where abc123 is v3's SHA
- **WHEN** tidy runs
- **THEN** the manifest keeps `actions/checkout = "^4"`
- **AND** the lock resolves ^4 to its correct SHA from the registry
- **AND** workflow files are updated to `uses: actions/checkout@<v4-sha> # v4`

#### Scenario: Manifest has ^4, workflow has v4 SHA (already consistent)
- **GIVEN** manifest has `actions/checkout = "^4"`
- **AND** all workflows have `uses: actions/checkout@def456 # v4` where def456 is v4's SHA
- **WHEN** tidy runs
- **THEN** the manifest keeps `actions/checkout = "^4"`
- **AND** no changes are made to workflow files

#### Scenario: Manifest has ^4, workflows have mixed v3 and v5
- **GIVEN** manifest has `actions/checkout = "^4"`
- **AND** ci.yml has `uses: actions/checkout@abc123 # v3`
- **AND** release.yml has `uses: actions/checkout@def456 # v5`
- **WHEN** tidy runs
- **THEN** the manifest keeps `actions/checkout = "^4"` as the global specifier
- **AND** overrides are recorded for ci.yml (^3) and release.yml (^5)

### Requirement: Workflows are source of truth for new actions
When tidy discovers an action in workflows that is not yet in the manifest, the dominant workflow version SHALL be used as the initial manifest specifier (converted via `from_v1`).

#### Scenario: New action with single version across workflows
- **GIVEN** the manifest does not track `actions/setup-node`
- **AND** all workflows use `actions/setup-node@abc123 # v4`
- **WHEN** tidy runs
- **THEN** the manifest adds `actions/setup-node = "^4"`

#### Scenario: New action with multiple versions picks dominant
- **GIVEN** the manifest does not track `actions/setup-node`
- **AND** three workflows use `actions/setup-node@abc123 # v4`
- **AND** one workflow uses `actions/setup-node@def456 # v3`
- **WHEN** tidy runs
- **THEN** the manifest adds `actions/setup-node = "^4"` (most-used)
- **AND** an override is recorded for the workflow using ^3

### Requirement: SHA correction applies only to new actions
When a new action is added to the manifest, tidy SHALL verify the workflow tag against the registry using the workflow's SHA. This correction SHALL NOT apply to actions already present in the manifest.

#### Scenario: New action with wrong comment tag
- **GIVEN** the manifest does not track `actions/checkout`
- **AND** workflows have `uses: actions/checkout@abc123 # v4`
- **AND** the registry reports that abc123 points to tags `[v3.6.0, v3]`
- **WHEN** tidy runs
- **THEN** the manifest adds `actions/checkout = "~3.6.0"` (corrected via registry, patch precision)

#### Scenario: SHA correction does not apply to existing actions
- **GIVEN** the manifest has `actions/checkout = "^4"`
- **AND** workflows have `uses: actions/checkout@abc123 # v3`
- **AND** abc123 points to v3 tags in the registry
- **WHEN** tidy runs
- **THEN** the manifest keeps `actions/checkout = "^4"` (existing, not corrected)

### Requirement: SHA-to-tag upgrade via registry
When the manifest has a raw SHA as an action version, tidy SHALL attempt to upgrade it to a human-readable specifier using the registry.

#### Scenario: SHA upgraded to tag with registry
- **GIVEN** manifest has `actions/checkout = "abc123def456789012345678901234567890abcd"`
- **AND** the registry reports that SHA points to tags `[v4, v4.2.1]`
- **WHEN** tidy runs
- **THEN** the manifest is updated to `actions/checkout = "~4.2.1"` (best tag, patch precision)

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
- **GIVEN** manifest has `actions/old-action = "^1"`
- **AND** no workflow references `actions/old-action`
- **WHEN** tidy runs
- **THEN** the manifest removes `actions/old-action`

### Requirement: Lock resolution uses workflow SHAs when available
Lock entries SHALL be resolved using the workflow SHA when one is available for the action. When no workflow SHA exists, resolution SHALL fall back to the registry. Existing complete lock entries SHALL NOT be re-resolved.

#### Scenario: Lock uses workflow SHA for new entry
- **GIVEN** manifest has `actions/checkout = "^4"`
- **AND** the lock has no entry for `(actions/checkout, ^4)`
- **AND** workflows have `uses: actions/checkout@abc123... # v4`
- **WHEN** tidy resolves the lock
- **THEN** the lock SHA is `abc123...` (from workflow, not freshly resolved from registry)
- **AND** the version is derived from `tags_for_sha(abc123...)` (most specific)

#### Scenario: Lock falls back to registry when no workflow SHA
- **GIVEN** manifest has `actions/checkout = "^4"`
- **AND** the lock has no entry for `(actions/checkout, ^4)`
- **AND** workflows have `uses: actions/checkout@v4` (no SHA pin)
- **WHEN** tidy resolves the lock
- **THEN** the SHA is obtained from the registry

#### Scenario: Lock uses cached entry when complete
- **GIVEN** manifest has `actions/checkout = "^4"`
- **AND** the lock already has a complete entry for `(actions/checkout, ^4)`
- **WHEN** tidy resolves the lock
- **THEN** no registry call is made for that entry

### Requirement: Workflow output omits comment for SHA-only versions
When the manifest version is a raw SHA (not a tag), workflow files SHALL be written with just the SHA ref, without a trailing comment.

#### Scenario: Tag version produces comment
- **GIVEN** manifest specifier is `^4` and lock SHA is `abc123`
- **WHEN** tidy writes the workflow reference
- **THEN** the output is `abc123 # v4`

#### Scenario: SHA version produces no comment
- **GIVEN** manifest version is `abc123def456789012345678901234567890abcd`
- **AND** the lock SHA is the same value
- **WHEN** tidy writes the workflow reference
- **THEN** the output is `abc123def456789012345678901234567890abcd` (no `#` comment)

### Requirement: Manifest v1 to v2 migration
When a v1 manifest (no `[gx]` section) is modified by tidy, the `[gx]` section SHALL be added automatically.

#### Scenario: v1 manifest migrated on first tidy
- **GIVEN** a v1 manifest with `[actions]` only
- **WHEN** tidy modifies the manifest via `apply_manifest_diff`
- **THEN** a `[gx]` section is added with `min_version` set to the current gx version
- **AND** action values are written as specifier strings (e.g., `"^4"` instead of `"v4"`)
