### Requirement: Lock completeness is checked across both tiers

Completeness checking operates on the combination of a resolution entry and its corresponding action entry. The `Lock` SHALL provide an `is_complete(spec: &Spec)` method that returns `true` only when:

1. A resolution exists for the spec with a non-empty `version`
2. The resolution's `comment` matches the specifier's expected comment
3. An action entry exists for `(ActionId, resolution.version)` with all metadata fields populated: `sha` (non-empty), `repository` (non-empty), `ref_type` (set), `date` (non-empty)

This replaces the previous `Entry::is_complete(manifest_version)` method. Completeness is now a property of the lock as a whole (checking both tiers), not of a single entry.

#### Scenario: Complete resolution and action entry
- **WHEN** a resolution exists for `(actions/checkout, ^4)` with `version = "v4.2.1"` and `comment = "v4"`
- **AND** an action entry exists for `(actions/checkout, v4.2.1)` with sha, repository, ref_type, and date all populated
- **THEN** the spec is complete

#### Scenario: Missing resolution
- **WHEN** no resolution exists for `(actions/checkout, ^4)`
- **THEN** the spec is incomplete
- **AND** triggers full resolution (RESOLVE + REFINE + DERIVE)

#### Scenario: Resolution exists but action entry missing
- **WHEN** a resolution exists for `(actions/checkout, ^4)` pointing to `v4.2.1`
- **AND** no action entry exists for `(actions/checkout, v4.2.1)`
- **THEN** the spec is incomplete
- **AND** triggers full resolution (RESOLVE + REFINE + DERIVE)
- **BECAUSE** the action entry may have been removed by orphan cleanup or file corruption; the version `v4.2.1` may no longer be available, so re-resolving from scratch is the only safe path

#### Scenario: Stale comment
- **WHEN** a resolution exists with `comment = "v6"` but the specifier is `^6.1` (expected comment `"v6.1"`)
- **THEN** the spec is incomplete
- **AND** triggers local derivation only (DERIVE) to update the comment

#### Scenario: Action entry missing ref_type
- **WHEN** the action entry exists but `ref_type` is not set
- **THEN** the spec is incomplete
- **AND** triggers refinement (REFINE + DERIVE)

#### Scenario: Non-semver manifest version
- **WHEN** a resolution exists for `(actions/checkout, main)` with `comment = ""`
- **AND** a complete action entry exists for `(actions/checkout, main)`
- **THEN** the spec is complete
- **BECAUSE** non-semver versions produce empty comments

### Requirement: Incomplete entries trigger targeted operations

The tidy command SHALL check each spec for completeness across both tiers and run only the operations needed.

#### Scenario: Missing resolution and action entry triggers full resolution
- **WHEN** a manifest spec has no resolution entry
- **THEN** the system runs RESOLVE (network) + REFINE (network) + DERIVE (local)
- **AND** creates both a resolution entry and an action entry

#### Scenario: Resolution exists but action entry incomplete triggers refinement
- **WHEN** a resolution points to a version whose action entry is missing metadata
- **THEN** the system runs REFINE (network) + DERIVE (local)
- **AND** does NOT run RESOLVE (the version is already known)

#### Scenario: Only comment is stale triggers local derivation
- **WHEN** a resolution exists with correct version but stale comment
- **AND** the action entry is complete
- **THEN** the system runs DERIVE only (update comment in resolution)
- **AND** does NOT make any network calls

#### Scenario: Complete spec is skipped
- **WHEN** a spec has a complete resolution and a complete action entry
- **THEN** no operations are performed

### Requirement: Self-healing on schema additions
When new fields are added to the action entry schema, existing lock files SHALL be reconciled automatically. The completeness check inspects action entry fields, so missing fields are detected and filled on the next tidy run without migration code.

### Requirement: Orphan cleanup in tidy
The tidy command SHALL remove action entries that are not referenced by any resolution entry. This cleanup runs after resolution and retain operations.

#### Scenario: Specifier change leaves orphaned action entry
- **GIVEN** a resolution `(actions/checkout, ^4)` pointing to `v4.2.1`
- **AND** an action entry for `(actions/checkout, v4.2.1)`
- **WHEN** the user changes the manifest specifier to `^4.3` and tidy resolves to `v4.3.0`
- **THEN** the resolution updates to point to `v4.3.0`
- **AND** a new action entry for `v4.3.0` is created
- **AND** the old `v4.2.1` action entry is removed during orphan cleanup

#### Scenario: Shared action entry is not orphaned
- **GIVEN** resolutions `(actions/checkout, ^4)` and `(actions/checkout, ^4.2)` both pointing to `v4.2.1`
- **WHEN** the `^4.2` resolution is removed (override deleted)
- **THEN** the `v4.2.1` action entry is NOT removed because `^4` still references it

#### Scenario: Upgrade orphans old action entry
- **GIVEN** a resolution `(actions/checkout, ^4)` pointing to `v4.2.1`
- **AND** an action entry for `(actions/checkout, v4.2.1)`
- **WHEN** `gx upgrade` resolves `^4` to `v4.3.0`
- **THEN** the resolution updates to point to `v4.3.0`
- **AND** a new action entry for `v4.3.0` is created
- **AND** the old `v4.2.1` action entry is removed during orphan cleanup

#### Scenario: Branch ref re-resolution updates action entry in place
- **GIVEN** a resolution `(actions/checkout, main)` pointing to version `main`
- **AND** an action entry for `(actions/checkout, main)` with SHA `aaa...`
- **WHEN** tidy re-resolves `main` and gets SHA `bbb...`
- **THEN** the action entry for `(actions/checkout, main)` is updated with the new SHA
- **AND** no orphaned entry is created
- **BECAUSE** the key (`main`) does not change — only the commit metadata changes
