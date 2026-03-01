### Requirement: Version specifier semantics
The manifest version's precision SHALL determine the semver range specifier used for safe upgrade filtering.

#### Scenario: Major precision uses caret
- **GIVEN** manifest version `v4`
- **THEN** the specifier is `^4` (>= 4.0.0, < 5.0.0)

#### Scenario: Minor precision uses caret
- **GIVEN** manifest version `v4.2`
- **THEN** the specifier is `^4.2` (>= 4.2.0, < 5.0.0)

#### Scenario: Patch precision uses tilde
- **GIVEN** manifest version `v4.1.0`
- **THEN** the specifier is `~4.1.0` (>= 4.1.0, < 4.2.0)

### Requirement: Upgrade candidate selection returns actual tags
The upgrade system SHALL return an actual tag name from the candidate list. It SHALL NOT construct or fabricate tag names.

#### Scenario: Candidate is an actual tag
- **GIVEN** manifest version `v2` and candidates `[v1, v2, v2.2.1, v3.0.0-beta.2]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v3.0.0-beta.2` (the actual tag, not `v3`)

#### Scenario: No fabricated tags
- **GIVEN** manifest version `v4` and candidates `[v4, v4.2.1, v5.0.0]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v5.0.0` (not `v5`)

### Requirement: Safe upgrade constrains to specifier range
In safe mode (default `gx upgrade`), candidates SHALL be within the specifier range derived from the manifest version.

#### Scenario: Major precision stays within major
- **GIVEN** manifest version `v4` and candidates `[v4.2.1, v5.0.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.2.1` (v5.0.0 excluded by ^4 range)

#### Scenario: Minor precision stays within major
- **GIVEN** manifest version `v4.2` and candidates `[v4.3.0, v5.0.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.3.0` (v5.0.0 excluded by ^4.2 range)

#### Scenario: Patch precision stays within major.minor
- **GIVEN** manifest version `v4.1.0` and candidates `[v4.1.3, v4.2.0, v5.0.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.1.3` (v4.2.0 excluded by ~4.1.0 range)

### Requirement: Latest upgrade crosses major versions
With `--latest`, candidates SHALL NOT be constrained by the specifier range.

#### Scenario: Latest crosses major
- **GIVEN** manifest version `v4` and candidates `[v4.2.1, v5.0.0, v6.1.0]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v6.1.0`

#### Scenario: Latest includes pre-releases
- **GIVEN** manifest version `v2` and candidates `[v2.2.1, v3.0.0-beta.2]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v3.0.0-beta.2` (pre-releases are valid candidates)

### Requirement: Upgrade floor uses lock version
Candidates SHALL be strictly greater than both the manifest version and the lock's resolved version. The floor is `max(manifest_semver, lock_version_semver)`.

#### Scenario: Lock version eliminates same-SHA candidate
- **GIVEN** manifest version `v4`, lock version `v4.2.1`, candidates `[v4.2.1, v4.3.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.3.0` (v4.2.1 is not > floor 4.2.1)

#### Scenario: Lock version absent (pre-1.3 lock)
- **GIVEN** manifest version `v4`, no lock version, candidates `[v4.2.1, v4.3.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.3.0` (floor falls back to manifest 4.0.0, both candidates qualify, highest wins)

#### Scenario: No upgrade when at latest
- **GIVEN** manifest version `v4`, lock version `v4.3.0`, candidates `[v4.2.1, v4.3.0]`
- **WHEN** upgrading in safe mode
- **THEN** no upgrade (no candidate > 4.3.0 within ^4)

### Requirement: Non-semver versions are skipped
Non-semver refs (branch names, bare SHAs) SHALL be excluded from upgrade candidate selection.

#### Scenario: Non-semver current version
- **GIVEN** manifest version `main`
- **WHEN** upgrading
- **THEN** no upgrade candidate is returned

#### Scenario: Non-semver candidates filtered out
- **GIVEN** manifest version `v4` and candidates `[main, develop, v5.0.0]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v5.0.0` (non-semver candidates ignored)
