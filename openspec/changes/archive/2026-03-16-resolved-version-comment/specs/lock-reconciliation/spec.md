### Requirement: Lock completeness is checked across both tiers [MODIFIED]

Completeness checking operates on the combination of a resolution entry and its corresponding action entry. The `Lock` SHALL provide an `is_complete(spec: &Spec)` method that returns `true` only when:

1. A resolution exists for the spec with a non-empty `version`
2. ~~The resolution's `comment` matches the specifier's expected comment~~ **REMOVED**
3. An action entry exists for `(ActionId, resolution.version)` with all metadata fields populated: `sha` (non-empty), `repository` (non-empty), `ref_type` (set), `date` (non-empty)

#### Scenario: Complete resolution and action entry [MODIFIED]
- **WHEN** a resolution exists for `(actions/checkout, ^4)` with `version = "v4.2.1"`
- **AND** an action entry exists for `(actions/checkout, v4.2.1)` with sha, repository, ref_type, and date all populated
- **THEN** the spec is complete

#### Scenario: Stale comment [REMOVED]
~~- **WHEN** a resolution exists with `comment = "v6"` but the specifier is `^6.1` (expected comment `"v6.1"`)~~
~~- **THEN** the spec is incomplete~~
~~- **AND** triggers local derivation only (DERIVE) to update the comment~~

**Reason**: The `comment` field no longer exists in `Resolution`. Completeness depends only on the resolved version and action entry metadata.

### Requirement: Incomplete entries trigger targeted operations [MODIFIED]

#### Scenario: Only comment is stale triggers local derivation [REMOVED]
~~- **WHEN** a resolution exists with correct version but stale comment~~
~~- **AND** the action entry is complete~~
~~- **THEN** the system runs DERIVE only (update comment in resolution)~~
~~- **AND** does NOT make any network calls~~

**Reason**: There is no comment to derive. The DERIVE operation for comment consistency is no longer needed.

#### Scenario: Non-semver manifest version [MODIFIED]
- **WHEN** a resolution exists for `(actions/checkout, main)` with `version = "main"`
- **AND** a complete action entry exists for `(actions/checkout, main)`
- **THEN** the spec is complete
- ~~**BECAUSE** non-semver versions produce empty comments~~ **BECAUSE** the resolution has a non-empty version and the action entry is fully populated
