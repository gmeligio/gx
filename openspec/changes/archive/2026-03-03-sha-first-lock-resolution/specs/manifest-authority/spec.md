## MODIFIED Requirements

### Requirement: Lock resolution uses workflow SHAs when available
Lock entries SHALL be resolved using the workflow SHA when one is available for the action. When no workflow SHA exists, resolution SHALL fall back to the registry. Existing complete lock entries SHALL NOT be re-resolved.

#### Scenario: Lock uses workflow SHA for new entry
- **GIVEN** manifest has `actions/checkout = "v4"`
- **AND** the lock has no entry for `(actions/checkout, v4)`
- **AND** workflows have `uses: actions/checkout@abc123... # v4`
- **WHEN** tidy resolves the lock
- **THEN** the lock SHA is `abc123...` (from workflow, not freshly resolved from registry)
- **AND** the version is derived from `tags_for_sha(abc123...)` (most specific)

#### Scenario: Lock falls back to registry when no workflow SHA
- **GIVEN** manifest has `actions/checkout = "v4"`
- **AND** the lock has no entry for `(actions/checkout, v4)`
- **AND** workflows have `uses: actions/checkout@v4` (no SHA pin)
- **WHEN** tidy resolves the lock
- **THEN** the SHA is obtained from the registry

#### Scenario: Lock uses cached entry when complete
- **GIVEN** manifest has `actions/checkout = "v4"`
- **AND** the lock already has a complete entry for `(actions/checkout, v4)`
- **WHEN** tidy resolves the lock
- **THEN** no registry call is made for that entry
