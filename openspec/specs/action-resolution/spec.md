## Action Resolution

User value: Action references resolve to the correct commit with proper metadata, and errors are communicated clearly.

---

### Requirement: SHA-pinned actions resolve using the workflow SHA

When a workflow already pins an action to a SHA, resolution uses that SHA directly rather than re-resolving from the registry. This preserves the exact commit the user chose.

#### Scenario: Init preserves a workflow's pinned SHA

- **GIVEN** no manifest or lock exists
- **AND** a workflow has `uses: jdx/mise-action@6d1e696... # v3`
- **AND** the registry reports tags `[v3, v3.6, v3.6.1]` for that SHA
- **WHEN** init runs
- **THEN** the lock entry uses SHA `6d1e696...` (the workflow SHA, not a freshly resolved one)
- **AND** the lock version is `v3.6.1` (the most specific tag for that SHA)

#### Scenario: No workflow SHA falls back to version-based resolution

- **GIVEN** a workflow has `uses: actions/checkout@v4` (no SHA pin)
- **WHEN** init runs
- **THEN** the lock SHA is obtained by resolving the `v4` tag via the registry

#### Scenario: Existing lock entries are not re-resolved

- **GIVEN** the lock already has a complete entry for an action
- **WHEN** tidy runs
- **THEN** no registry call is made for that entry

### Requirement: SHA-pinned actions keep the workflow SHA on update

When a workflow already has a SHA-pinned action, the lock entry uses the workflow's SHA, not the SHA that the registry returns for the version tag. This ensures workflow pinning is never silently overridden.

---

### Requirement: Tag selection prefers the most specific version

When multiple tags point to the same SHA, the lock version is the tag with the most semver components. Among tags with equal component count, the highest version wins. Non-semver tags are always ranked last.

#### Scenario: Most specific tag wins

- **GIVEN** tags `[v4, v4.1, v4.1.0]` point to a SHA
- **WHEN** selecting the best tag
- **THEN** the result is `v4.1.0`

#### Scenario: Highest version wins among same precision

- **GIVEN** tags `[v3, v4, v5]` point to a SHA
- **WHEN** selecting the best tag
- **THEN** the result is `v5`

#### Scenario: Non-semver tags sort last

- **GIVEN** tags `[latest, v4]` point to a SHA
- **WHEN** selecting the best tag
- **THEN** the result is `v4`

#### Scenario: SHA with no tags uses the SHA as version

- **GIVEN** no tags point to a SHA
- **WHEN** resolving that SHA
- **THEN** the lock version is the SHA itself

---

### Requirement: Annotated tags dereference to commit SHAs

When a git ref points to an annotated tag object rather than a commit, resolution dereferences through the tag to obtain the underlying commit SHA. Users always get a commit SHA in the lock, never a tag object SHA.

---

### Requirement: Resolution returns metadata from the best available source

Each resolved action carries a reference type (release, tag, branch, or commit) and a date. The date is chosen from the most authoritative source available.

#### Scenario: Tag with a GitHub Release

- **GIVEN** a tag that has an associated GitHub Release
- **WHEN** resolving that tag
- **THEN** the reference type is Release and the date is the release's publication date

#### Scenario: Tag without a GitHub Release

- **GIVEN** a tag with no associated GitHub Release
- **WHEN** resolving that tag
- **THEN** the reference type is Tag and the date is the tag or commit date

#### Scenario: Branch ref

- **WHEN** resolving a branch reference
- **THEN** the reference type is Branch and the date is the commit date

#### Scenario: Direct SHA

- **WHEN** resolving a bare SHA
- **THEN** the reference type is Commit and the date is the commit date

---

### Requirement: Version specifier semantics follow manifest precision

The manifest version's precision determines the semver range used for safe upgrades. This gives users predictable control over how far upgrades can reach.

#### Scenario: Major precision uses caret range

- **GIVEN** manifest version `v4`
- **THEN** the specifier is `^4` (allows >= 4.0.0, < 5.0.0)

#### Scenario: Minor precision uses caret range

- **GIVEN** manifest version `v4.2`
- **THEN** the specifier is `^4.2` (allows >= 4.2.0, < 5.0.0)

#### Scenario: Patch precision uses tilde range

- **GIVEN** manifest version `v4.1.0`
- **THEN** the specifier is `~4.1.0` (allows >= 4.1.0, < 4.2.0)

---

### Requirement: Safe upgrade stays within the specifier range

The default upgrade mode constrains candidates to the semver range derived from the manifest version. Users opt into broader upgrades explicitly.

#### Scenario: Major precision stays within major

- **GIVEN** manifest version `v4` and candidates `[v4.2.1, v5.0.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.2.1` (v5.0.0 excluded by ^4 range)

#### Scenario: Minor precision stays within major

- **GIVEN** manifest version `v4.2` and candidates `[v4.3.0, v5.0.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.3.0`

#### Scenario: Patch precision stays within minor

- **GIVEN** manifest version `v4.1.0` and candidates `[v4.1.3, v4.2.0, v5.0.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.1.3`

---

### Requirement: Latest upgrade crosses major versions

With `--latest`, candidates are not constrained by the specifier range. Pre-release handling depends on whether the current manifest version is itself a pre-release.

#### Scenario: Latest crosses major

- **GIVEN** manifest version `v4` and candidates `[v4.2.1, v5.0.0, v6.1.0]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v6.1.0`

#### Scenario: Stable manifest excludes pre-releases

- **GIVEN** manifest version `v2` and candidates `[v2.2.1, v3.0.0, v3.0.0-beta.2]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v3.0.0` (pre-releases excluded)

#### Scenario: Pre-release manifest prefers stable

- **GIVEN** manifest version `v3.0.0-beta.2` and candidates `[v3.0.0, v3.1.0-dev.1]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v3.0.0` (stable preferred over pre-release)

#### Scenario: Pre-release manifest falls back to newer pre-release

- **GIVEN** manifest version `v3.1.0-dev.1` and candidates `[v3.1.0-dev.2, v3.1.0-dev.3]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v3.1.0-dev.3` (no stable exists; newest pre-release selected)

---

### Requirement: Upgrade floor uses the lock version

Candidates must be strictly greater than both the manifest version and the lock's resolved version. This prevents upgrading to a version the user already has.

#### Scenario: Lock version eliminates current candidate

- **GIVEN** manifest version `v4`, lock version `v4.2.1`, candidates `[v4.2.1, v4.3.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.3.0`

#### Scenario: No upgrade when already at latest

- **GIVEN** manifest version `v4`, lock version `v4.3.0`, candidates `[v4.2.1, v4.3.0]`
- **WHEN** upgrading in safe mode
- **THEN** no upgrade is available

#### Scenario: Missing lock version falls back to manifest

- **GIVEN** manifest version `v4`, no lock version, candidates `[v4.2.1, v4.3.0]`
- **WHEN** upgrading in safe mode
- **THEN** the result is `v4.3.0` (floor falls back to 4.0.0)

---

### Requirement: Non-semver versions are excluded from upgrades

Branch names, bare SHAs, and other non-semver refs cannot participate in version comparison. They are excluded from candidate selection.

#### Scenario: Non-semver manifest version

- **GIVEN** manifest version `main`
- **WHEN** upgrading
- **THEN** no upgrade candidate is returned

#### Scenario: Non-semver candidates filtered out

- **GIVEN** manifest version `v4` and candidates `[main, develop, v5.0.0]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v5.0.0`

---

### Requirement: Upgrade candidates are actual tags

The upgrade system returns actual tag names from the registry. It never constructs or fabricates tag names that do not exist.

#### Scenario: Result is an actual tag

- **GIVEN** manifest version `v4` and candidates `[v4, v4.2.1, v5.0.0]`
- **WHEN** upgrading with `--latest`
- **THEN** the result is `v5.0.0` (not a fabricated `v5`)

---

### Requirement: GitHub API works without an authentication token

Resolution works against public repositories without a token. When no token is configured, a one-time warning is emitted: unauthenticated requests are limited to 60 per hour.

---

### Requirement: SHA descriptions are deduplicated within a single run

When multiple entries reference the same SHA for the same action, the registry is queried at most once. The cached description is reused for subsequent lookups within the same run and discarded afterward.

---

## Guardrail: Error classification (recoverable vs. strict)

This classification is load-bearing because it determines whether a user sees a warning they can act on later, or a hard failure that blocks their workflow.

### Rule: Resolution errors are classified as recoverable or strict

| Error condition | Classification | User experience |
|---|---|---|
| Rate limited | Recoverable | Warning; action skipped, lock written without it |
| Auth required | Recoverable | Warning; action skipped, lock written without it |
| Action not found (404) | Strict | Hard failure; command exits with error |
| Server error (5xx) | Strict | Hard failure; command exits with error |

### Rule: Recoverable errors produce warnings; strict errors produce failures

When resolution encounters errors for multiple actions, each error is classified independently. Recoverable errors are logged as warnings and those actions are skipped. Only strict errors cause the command to fail.

#### Scenario: All errors are recoverable

- **GIVEN** all resolution failures are rate-limited or auth-required
- **WHEN** the command completes
- **THEN** warnings are logged and the lock is written without those entries

#### Scenario: Mix of recoverable and strict errors

- **GIVEN** some failures are recoverable and some are strict
- **WHEN** the command completes
- **THEN** recoverable errors are logged as warnings
- **AND** the command fails reporting only the strict errors

#### Scenario: All errors are strict

- **GIVEN** all resolution failures are not-found or server errors
- **WHEN** the command completes
- **THEN** the command fails with all strict errors reported
