## ADDED Requirements

### Requirement: Locked actions are checked against published advisories

`gx audit` SHALL compare every action recorded in `gx.lock` against the GitHub Advisory Database for the `ACTIONS` ecosystem, and SHALL emit an error-level `known-vulnerability` finding for each locked action whose recorded version falls inside a published advisory's affected version range.

The comparison SHALL be keyed on the action's repository and its recorded **version**. When an advisory package is a GitHub URL naming an action subpath, it SHALL apply only to that action, not unrelated subpaths. Repository names are case-insensitive; action subpaths retain their case. The comparison SHALL NOT be keyed on the pinned commit SHA: advisories carry no SHA-shaped ranges, and looking a compromised action up by SHA returns nothing.

#### Scenario: A pinned version inside an affected range is reported

- **WHEN** `gx.lock` pins `tj-actions/changed-files` at version `45.0.7` and an advisory declares that package affected in `<= 45.0.7`
- **THEN** `gx audit` emits one `known-vulnerability` finding at error level
- **AND** the process exits nonzero

#### Scenario: A pinned version outside every affected range is not reported

- **WHEN** `gx.lock` pins `tj-actions/changed-files` at version `46.0.1` and the only advisory for that package declares `<= 45.0.7`
- **THEN** `gx audit` emits no `known-vulnerability` finding for it

#### Scenario: An advisory for an action that is not locked is ignored

- **WHEN** the advisory set contains entries for packages absent from `gx.lock`
- **THEN** `gx audit` emits no finding for those packages

#### Scenario: A GitHub URL identifies a nested action

- **WHEN** an advisory names `https://github.com/pytorch/pytorch/.github/actions/filter-test-configs` with affected range `< 2.0.1`
- **AND** the lock records `pytorch/pytorch/.github/actions/filter-test-configs` at `v2.0.0`
- **THEN** audit emits a `known-vulnerability` error for that action
- **AND** that advisory does not match a different subpath or the repository root action

#### Scenario: A non-GitHub URL does not identify a GitHub action

- **WHEN** a package URL has a host other than `github.com`
- **THEN** it does not match a locked GitHub action merely because its URL path has the same repository and subpath

### Requirement: Withdrawn advisories do not produce findings

`gx audit` SHALL ignore advisories with a non-null withdrawal timestamp. Response completeness SHALL be checked before withdrawn entries are removed.

#### Scenario: A withdrawn advisory and an active advisory cover the same pin

- **WHEN** both an active advisory and a withdrawn advisory cover a locked action version
- **THEN** only the active advisory produces a finding

#### Scenario: A complete response contains only withdrawn advisories

- **WHEN** every returned advisory is withdrawn and the response contains the full advertised count
- **THEN** the lookup succeeds with no active advisories

#### Scenario: A response containing withdrawn advisories is truncated

- **WHEN** the advertised count exceeds the number of returned nodes, including withdrawn nodes
- **THEN** the lookup fails rather than reporting a complete result

### Requirement: A finding identifies the advisory actionably

A `known-vulnerability` finding SHALL name the affected action, the locked version, the GHSA identifier, the severity as GitHub classifies it, the affected version range, and a link to the advisory. When the advisory names a first patched version, the finding SHALL include it.

The link is required, not decorative: a security finding a user cannot independently verify is one they must either take on faith or ignore, and both are failures. It lets a user confirm the claim against the published advisory rather than trusting gx's range arithmetic.

#### Scenario: The finding carries advisory identity

- **WHEN** a `known-vulnerability` finding is produced for `tj-actions/changed-files` at `45.0.7` from `GHSA-mrrh-fwg8-r2c3` (HIGH, `<= 45.0.7`, first patched `46.0.1`)
- **THEN** the message contains `tj-actions/changed-files`, `45.0.7`, `GHSA-mrrh-fwg8-r2c3`, the severity, the range `<= 45.0.7`, `46.0.1`, and the advisory permalink

#### Scenario: An advisory with no fix is still reported

- **WHEN** a matching advisory has no `firstPatchedVersion`
- **THEN** the finding is still emitted with the GHSA id, severity, and range, and omits any patched-version claim

### Requirement: Version-range matching is performed locally

Range matching SHALL be evaluated inside gx against the locked version. gx SHALL NOT delegate version filtering to OSV.dev, whose `GitHub Actions` version filter returns an empty result for known-vulnerable versions and would therefore report a false "clean".

Matching SHALL treat a leading `v` as insignificant on either side, since gx records tag-shaped versions such as `v4.2.1` while advisories use bare identifiers such as `4.2.1`. Matching SHALL accept the partial-version forms advisories actually use, including bare majors (`< 41`) and major.minor.

#### Scenario: Boundaries of an exclusive upper bound

- **WHEN** the affected range is `< 46.0.1`
- **THEN** `46.0.0` matches, `46.0.1` does not, and `46.0.2` does not

#### Scenario: Boundaries of a two-sided range

- **WHEN** the affected range is `>= 2.25.0, < 2.37.1`
- **THEN** `2.25.0` and `2.37.0` match, while `2.24.9` and `2.37.1` do not

#### Scenario: Boundaries of an inclusive upper bound

- **WHEN** the affected range is `<= 0.24.0`
- **THEN** `0.24.0` matches and `0.24.1` does not

#### Scenario: The `v` prefix does not change the verdict

- **WHEN** a locked version is written `v45.0.7` and the affected range is `<= 45.0.7`
- **THEN** it matches, identically to the unprefixed `45.0.7`

#### Scenario: A bare-major range is evaluated

- **WHEN** the affected range is `< 41`
- **THEN** `40.0.0` matches and `41.0.0` does not

### Requirement: An unusable version or range never silently passes

When an action **has published advisories**, gx SHALL emit a `known-vulnerability` finding at **warn** level if the locked version or advisory range cannot be parsed, a prerelease does not match explicitly, or a partial version's zero-padded value does not match but its version line could overlap the range. These cases SHALL NOT silently become "not affected".

Partial versions whose zero-padded value matches SHALL conservatively produce an error-level finding, even if the actual version could be newer and unaffected. This deliberate over-reporting is distinct from the unmatched-but-overlapping cases that warn.

This is deliberately narrow. An action with no advisories at all is not reported however imprecise its version, because there is nothing whose applicability is in question.

#### Scenario: A matching padded version is conservatively affected

- **WHEN** a locked `v2` is compared with `< 2.5.0`
- **THEN** the padded `2.0.0` matches and audit emits an error-level finding
- **AND** this verdict does not claim that the actual version behind the partial tag is known

#### Scenario: A strict partial lower bound excludes the entire locked line

- **WHEN** `v2` is compared with `> 2`, or `v2.5` with `> 2.5`
- **THEN** audit reports no finding, because those ranges exclude the entire locked version line
- **AND** `v2` against `> 2.5` and `v2.5` against `> 2.5.3` remain undetermined

#### Scenario: A branch-pinned entry cannot be range-matched

- **WHEN** `gx.lock` records a locked version of `main` for an action that has published advisories
- **THEN** `gx audit` emits a warn-level finding that the action has advisories whose applicability could not be determined
- **AND** the run does not fail on that finding alone

#### Scenario: An uninterpretable advisory range is surfaced

- **WHEN** an advisory's affected range cannot be parsed
- **THEN** `gx audit` reports the advisory as undetermined for the locked action rather than treating it as not applicable

#### Scenario: A partial version cannot be placed against a lower bound

- **WHEN** `gx.lock` records `v2` and an advisory declares that package affected in `>= 2.5.0, < 3.0.0`
- **THEN** `gx audit` reports the action as undetermined, because the `v2` tag may resolve above or below `2.5.0`
- **AND** it does NOT report the action as unaffected

#### Scenario: A partial version overlaps an equality range

- **WHEN** `gx.lock` records `v2` or `v2.5` and an advisory declares that package affected in `= 2.5.0`
- **THEN** audit reports the pin as affected or undetermined, never unaffected
- **AND** `v3` and `v2.4` remain unaffected by that equality range

#### Scenario: Dependency prerelease exclusions are not evidence of safety

- **WHEN** a locked prerelease such as `v45.0.7-beta.1` is compared with `<= 45.0.7`
- **THEN** audit does not report it as unaffected solely because a dependency-range matcher excludes prereleases
- **AND** if applicability cannot be established, audit emits an undetermined warning

#### Scenario: An unparseable version on an action with no advisories stays silent

- **WHEN** `gx.lock` records a locked version of `main` for an action with no published advisories
- **THEN** `gx audit` emits no `known-vulnerability` finding for it

### Requirement: A failed advisory lookup fails the command

When the advisory lookup does not succeed — network failure, rejected credentials, or an unparseable response — `gx audit` SHALL exit with an error and SHALL NOT print a report. An empty advisory result from a successful lookup is distinct: it means "checked, nothing found".

#### Scenario: A rejected query aborts the run

- **WHEN** the advisory lookup fails
- **THEN** `gx audit` terminates with an error naming the failure
- **AND** no findings report is printed, in human or `--json` mode

#### Scenario: A successful empty result is a clean report

- **WHEN** the advisory lookup succeeds and returns no advisories
- **THEN** `gx audit` reports no `known-vulnerability` findings and exits zero

### Requirement: Audit credentials are checked before repository configuration

For an audit invocation, gx SHALL check for `GITHUB_TOKEN` before loading repository configuration or creating a local log file. Help and argument parsing SHALL remain available without a token.

#### Scenario: Missing credentials take precedence over malformed configuration

- **WHEN** `GITHUB_TOKEN` is absent and the repository manifest or lock file is malformed
- **THEN** `gx audit` fails with the actionable missing-token error
- **AND** JSON mode emits no findings document and creates no log file

### Requirement: JSON audit does not create a local log

`gx audit --json` SHALL write its successful report to stdout without creating local audit log files. Human-mode audit SHALL retain its existing log behavior.

#### Scenario: JSON output has no filesystem log side effect

- **WHEN** a successful audit runs with `--json`
- **THEN** stdout contains one JSON report
- **AND** no audit log file is created, even if a log path would not have been printed

### Requirement: The advisory set is fetched in one request

`gx audit` SHALL retrieve the `ACTIONS` ecosystem advisory set in a single query rather than one query per locked action. The set is small enough to fetch wholesale — measured at 63 advisories across 47 packages on one page — so per-action querying would multiply requests without adding information.

When no actions are locked there is nothing to check, so gx SHALL issue no advisory query at all. This keeps `gx audit` on an empty lock from failing for a network reason when it has no work to do.

#### Scenario: Request count does not scale with lock size

- **WHEN** `gx audit` runs against a lock containing many actions
- **THEN** exactly one advisory query is issued regardless of how many actions are locked

#### Scenario: An empty lock issues no query

- **WHEN** `gx audit` runs against a lock with no entries
- **THEN** no advisory query is issued
- **AND** the command reports no findings and exits zero
