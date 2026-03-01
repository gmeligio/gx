## MODIFIED Requirements

### Requirement: ResolvedAction carries metadata
The `ResolvedAction` struct SHALL include `repository`, `ref_type`, and `date` so that `Lock::set()` can store the full entry. `ResolvedAction` SHALL NOT carry `resolved_version` or `specifier` — these are outputs of REFINE and DERIVE respectively, not of resolution.

#### Scenario: Resolution flows through to lock
- **WHEN** `ActionResolver::resolve()` returns a `Resolved` result
- **AND** REFINE and DERIVE are applied to produce version and specifier
- **AND** `lock.set()` is called with the combined data
- **THEN** the lock entry contains all six fields (sha, version, specifier, repository, ref_type, date)

#### Scenario: ResolvedAction supports SHA override
- **GIVEN** a `ResolvedAction` produced by `resolve()`
- **WHEN** `with_sha(new_sha)` is called
- **THEN** a new `ResolvedAction` is returned with the SHA replaced and all other fields preserved

### Requirement: Version correction is separate from metadata resolution
The `ActionResolver` SHALL provide version refinement as a standalone operation that returns a version based on SHA tag lookup. This operation SHALL be used for both manifest version correction (Phase 1) and lock version population (Phase 2).

#### Scenario: Version refinement returns best tag for SHA
- **WHEN** `refine_version(id, sha)` is called with a SHA that points to tags `[v6, v6.0.1]`
- **THEN** the result is `v6` (shortest/least-specific tag preferred)

#### Scenario: Version refinement used for manifest correction
- **WHEN** a workflow pins SHA `abc123` with comment `v4`
- **AND** `refine_version(id, abc123)` returns `v5`
- **THEN** the manifest version is corrected from `v4` to `v5`

#### Scenario: Version refinement used for lock version field
- **WHEN** a lock entry is missing its `version` field
- **AND** `refine_version(id, sha)` returns `v6.0.2`
- **THEN** the lock entry's `version` is set to `v6.0.2`

#### Scenario: Version refinement degrades gracefully without token
- **WHEN** `refine_version(id, sha)` is called without a GITHUB_TOKEN
- **THEN** the operation returns `None` or the original version
- **AND** the entry remains incomplete (to be retried on next run with token)

## REMOVED Requirements

### Requirement: All lock entries flow through resolve
**Reason**: Replaced by the lock reconciliation model. Lock entries no longer require full resolution to populate all fields. REFINE and DERIVE can fill missing fields independently without re-resolving the SHA.
**Migration**: Lock completeness check (`is_complete`) determines what operations each entry needs. Full resolution is only triggered when the entry is entirely missing.
