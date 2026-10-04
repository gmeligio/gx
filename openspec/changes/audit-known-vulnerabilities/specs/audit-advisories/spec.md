## ADDED Requirements

### Requirement: Locked actions are checked against published advisories

`gx audit` SHALL compare every action recorded in `gx.lock` against the GitHub Advisory Database for the `ACTIONS` ecosystem, and SHALL emit an error-level `known-vulnerability` finding for each locked action whose recorded version falls inside a published advisory's affected version range.

The comparison SHALL be keyed on the action's repository and its recorded **version**. It SHALL NOT be keyed on the pinned commit SHA: advisories carry no SHA-shaped ranges, and looking a compromised action up by SHA returns nothing.

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

When an action **has published advisories** but gx cannot decide whether they apply — the locked version does not parse as a version, the advisory range does not parse, or the locked version is too imprecise to place against the range — gx SHALL NOT treat it as "not affected". gx SHALL emit a `known-vulnerability` finding at **warn** level stating that applicability could not be determined.

Warn rather than error: the action may well be unaffected, and failing a build on an unknown is the false alarm that costs a security tool its credibility. Silence would be the opposite and worse failure — a check reporting "clean" for a reason the user never sees. Warn is the honest middle, visible without being fatal.

This is deliberately narrow. An action with no advisories at all is not reported however imprecise its version, because there is nothing whose applicability is in question.

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
