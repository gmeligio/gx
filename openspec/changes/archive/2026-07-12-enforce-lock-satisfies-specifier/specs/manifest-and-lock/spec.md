## ADDED Requirements

### Requirement: A resolved lock version satisfies its specifier

A resolution's `version` MUST satisfy the specifier it is keyed under. When a specifier is a semver range (`^`, `~`), the resolved version SHALL be within that range. Non-semver specifiers (branch refs like `main`, bare SHAs) impose no range and are exempt — any resolved version is valid for them.

**User value:** A user editing `gx.toml` trusts that the lock reflects the range they declared. The manifest specifier is authoritative; the pinned SHA and its locked version are a *preference* that is honored only while it satisfies that range (the model pnpm, uv, and Cargo share).

`gx tidy` (and `gx init`, which shares its resolution path) SHALL NOT write a resolution whose version violates its specifier, even when a workflow pins a SHA whose tag lies outside the range.

#### Scenario: In-range pinned SHA is preserved
- **GIVEN** a manifest specifier `^5` for `actions/checkout`
- **AND** a workflow pins a SHA whose most-specific tag is `v5.4.0`
- **WHEN** tidy runs
- **THEN** the lock resolution for `(actions/checkout, ^5)` has version `v5.4.0`
- **AND** the workflow SHA is kept as the resolved commit

#### Scenario: Non-semver specifier accepts any resolved version
- **GIVEN** a manifest specifier `main` for `actions/checkout`
- **AND** a workflow pins a SHA whose tags include `v6.0.2`
- **WHEN** tidy runs
- **THEN** the resolution is valid and no range violation is reported

### Requirement: Tidy reconciles an out-of-range pinned SHA within the manifest range

When a workflow's pinned SHA carries a tag outside the manifest's declared range, `gx tidy` treats the pin as a stale preference. The SHA remains authoritative for *which commit* the tag family points at, but the manifest range is authoritative for *which version label is admissible*. Tidy SHALL re-resolve the version within the range and repin the workflow to the resolved commit, rather than recording the out-of-range tag.

#### Scenario: Cross-major pinned SHA is re-resolved within range
- **GIVEN** a manifest specifier `^5` for `actions/checkout`
- **AND** a workflow pins a SHA whose most-specific tag is `v6.0.2`
- **AND** the registry offers `v5.4.0` within the `^5` range
- **WHEN** tidy runs
- **THEN** the lock resolution for `(actions/checkout, ^5)` has a version satisfying `^5` (e.g. `v5.4.0`)
- **AND** the lock never records `v6.0.2` under the `^5` key
- **AND** the workflow is repinned to the in-range resolution

#### Scenario: Sub-major violation is also caught
- **GIVEN** a manifest specifier `~1.15.2` for an action
- **AND** a workflow pins a SHA whose most-specific tag is `v1.16.0`
- **WHEN** tidy runs
- **THEN** `v1.16.0` is rejected as out of range (it does not satisfy `~1.15.2`)
- **AND** the resolution version satisfies `~1.15.2`

#### Scenario: Init deriving a fresh specifier is not a violation
- **GIVEN** no manifest exists
- **AND** a workflow pins `uses: actions/checkout@<sha> # v6` whose most-specific tag is `v6.0.2`
- **WHEN** init runs
- **THEN** the manifest specifier is derived as `^6`
- **AND** the lock resolution has version `v6.0.2`, which satisfies the derived `^6`
- **AND** no range violation is reported (the specifier was manufactured from this SHA, so it holds by construction)
