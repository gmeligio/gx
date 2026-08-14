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

### Requirement: Registry lookups are deduplicated within a single run

Every query a command makes against the version registry SHALL be issued at most once per distinct set of arguments for the duration of that run. This covers looking up the commit for a version, listing all tags for an action, and describing a SHA. A repeated query with identical arguments SHALL be served from the run's memory without a network request, and SHALL return the same result the first query returned.

The user who benefits is anyone running `gx init`, `gx tidy`, or `gx upgrade` against a repository that references the same action from more than one workflow, or at more than one version — the common case. What they notice is that the command makes fewer GitHub API requests, so it is less likely to exhaust the 60 requests/hour unauthenticated limit and degrade into skip warnings with an incomplete lock.

The memory SHALL be discarded when the command finishes, so every run observes current registry state. Only successful results SHALL be reused; a failed query SHALL be retried if asked again, so a transient failure never poisons the rest of the run.

#### Scenario: Same version resolved from more than one manifest entry

- **GIVEN** a manifest where an override resolves `actions/checkout` at `v4` and the global entry resolves it at `v4` too
- **WHEN** `gx tidy` runs
- **THEN** the commit for `actions/checkout` at `v4` is looked up from the registry exactly once
- **AND** both lock entries record that same commit

#### Scenario: Same action appears more than once in an upgrade

- **GIVEN** a manifest whose entries resolve `actions/checkout` more than once during a single upgrade
- **WHEN** `gx upgrade` runs
- **THEN** the tag list for `actions/checkout` is fetched from the registry exactly once

#### Scenario: Repeated SHA description

- **GIVEN** two lock entries that reference the same SHA for the same action
- **WHEN** that SHA is described a second time
- **THEN** no registry request is made and the first description is reused

#### Scenario: Distinct arguments are not conflated

- **GIVEN** `actions/checkout` is looked up at `v4` and then at `v3`
- **WHEN** both lookups run in the same command
- **THEN** each issues its own registry request and returns its own commit

#### Scenario: A failed lookup is not reused

- **GIVEN** a lookup for an action fails because the registry is rate limited
- **WHEN** the same lookup is attempted again in the same run
- **THEN** the registry is queried again rather than replaying the failure

#### Scenario: Memory does not survive the run

- **GIVEN** `gx tidy` completed and resolved `actions/checkout@v4`
- **WHEN** `gx tidy` runs again later
- **THEN** the registry is queried again, so a newly published tag is observed

---

### Requirement: Rate-limited resolution is retried automatically within a bounded budget

The system SHALL retry a resolution request that failed because the forge's request quota was exhausted, and SHALL NOT retry any other failure.

A user running `gx tidy`, `gx init`, or `gx upgrade` without a token has 60 GitHub requests per hour. When that quota is exhausted mid-run, the action being resolved is dropped from the lock and the user must notice the warning and rerun by hand — even when the quota window was about to roll over.

The retry budget SHALL be bounded: after a fixed maximum number of additional attempts, the rate-limit error is returned to the caller and handled exactly as it is today (warning, action skipped). A genuinely exhausted quota therefore fails promptly rather than hanging.

Only quota exhaustion is retried. A rejected or absent credential SHALL NOT be retried: reissuing the identical request cannot produce a different outcome, so retrying it only delays a failure the user must fix themselves.

#### Scenario: A transient rate limit resolves without user intervention

- **GIVEN** a resolution request that fails with a rate-limit error and would succeed on the next attempt
- **WHEN** the user runs a command that resolves that action
- **THEN** the action resolves successfully and is written to the lock
- **AND** the user is not asked to rerun the command

#### Scenario: A persistently exhausted quota fails after a bounded number of attempts

- **GIVEN** a resolution request that fails with a rate-limit error on every attempt
- **WHEN** the user runs a command that resolves that action
- **THEN** the number of requests issued for that resolution is bounded by a fixed maximum
- **AND** the rate-limit error is returned, producing the existing warning and skip

#### Scenario: A missing credential is not retried

- **GIVEN** a resolution request that fails because no usable credential is configured
- **WHEN** the user runs a command that resolves that action
- **THEN** exactly one request is issued
- **AND** the auth error is returned immediately, producing the existing warning and skip

#### Scenario: A non-retryable failure is not retried

- **GIVEN** a resolution request that fails because the action does not exist
- **WHEN** the user runs a command that resolves that action
- **THEN** exactly one request is issued
- **AND** the command fails as it does today

---

### Requirement: The wait before a retry honors the forge's reset time but is capped

The system SHALL derive the wait from the forge's reported reset time when that time is available, and SHALL clamp every wait to a fixed maximum. When the reported reset time exceeds that maximum, the system SHALL NOT wait — the rate-limit error is returned immediately so the user gets their partial result and a warning now, rather than a stalled terminal.

The forge reports when its quota resets. Waiting exactly that long recovers a window that is about to roll over; but an exhausted unauthenticated quota can reset up to an hour out, and blocking a user's terminal for an hour is worse than failing.

When no reset time is available, the system SHALL fall back to a fixed backoff schedule that increases between attempts.

A reported reset time SHALL be treated as a floor on the wait rather than its exact value: the system SHALL wait at least the backoff schedule's step for that attempt. A forge reporting whole-second resets states a reset of zero whenever the quota window is about to turn over, and a local clock running ahead of the forge's reports one that has already passed. Taken literally, either would reissue the request instantly against a forge that is still rate limiting. Treating the reported time as a floor also keeps a reset restated on every attempt increasing rather than repeating the same wait.

#### Scenario: A near reset time is waited out

- **GIVEN** the forge reports its quota resets a few seconds from now, beyond the backoff step for this attempt
- **WHEN** a rate-limited request is retried
- **THEN** the retry waits approximately until that reset time before reissuing the request

#### Scenario: A distant reset time is not waited on

- **GIVEN** the forge reports its quota resets an hour from now
- **WHEN** a request fails with a rate-limit error
- **THEN** no wait occurs and the rate-limit error is returned immediately
- **AND** the user gets their partial lock and warning without a stalled terminal

#### Scenario: A reset time in the past does not retry without pausing

- **GIVEN** the forge reports a reset time at or earlier than the local clock's current time
- **WHEN** a rate-limited request is retried
- **THEN** the retry still waits the backoff step for that attempt rather than reissuing the request immediately

#### Scenario: A reset time restated on every attempt still increases the wait

- **GIVEN** the forge reports the same short reset time on each successive rate-limit response
- **WHEN** a request is retried more than once
- **THEN** each successive wait is longer than the previous one rather than repeating it

#### Scenario: A missing reset time falls back to increasing backoff

- **GIVEN** the forge reports no reset time with its rate-limit response
- **WHEN** a request is retried more than once
- **THEN** each successive wait is at least as long as the previous one
- **AND** every wait remains within the fixed maximum

---

### Requirement: A retry wait is announced to the user

The system SHALL report each retry wait through the same progress channel that carries other resolution progress, stating that the run is waiting on the forge's rate limit and for how long. A command that pauses for several seconds with no explanation reads as a hang.

The announcement SHALL travel the existing progress channel rather than a new one, so it inherits that channel's existing suppression in `--json` mode and cannot corrupt the single JSON document on stdout.

#### Scenario: The user sees why the command paused

- **GIVEN** a rate-limited request that will be retried after a wait
- **WHEN** the wait begins
- **THEN** a progress message naming the rate limit and the wait duration is emitted before the process sleeps

#### Scenario: The announcement precedes the wait rather than following it

- **GIVEN** a rate-limited request that will be retried after a wait
- **WHEN** the user watches the command
- **THEN** the explanation appears before the pause, not after it, so the pause is never an unexplained stall

---

## Guardrail: Error classification (skippable and retryable)

This classification is load-bearing because it determines whether a user sees a warning they can act on later, or a hard failure that blocks their workflow — and, separately, whether a caller may re-issue the request at all.

### Rule: Resolution errors are classified as skippable and retryable

Classification is two-dimensional. A single "recoverable" bit cannot express both of the decisions callers make, so each resolution error carries two independent properties:

- **Skippable** — may the current run continue without this action? A skippable error is reported as a warning and the lock is written without that entry. A non-skippable (strict) error fails the command.
- **Retryable** — would repeating the identical request plausibly succeed? Only a retryable error may be re-issued by a caller that retries.

**Skippable does NOT imply retryable**, and the two MUST be asked separately. A missing or rejected credential is skippable — the run continues without that action — but repeating the identical request cannot produce a different outcome. A caller that retries MUST gate on retryability, never on skippability.

| Error condition | Skippable | Retryable | User experience |
|---|---|---|---|
| Rate limited | Yes | Yes | Retried within a bounded budget (unless the forge reports a reset beyond the cap, in which case it is not retried at all); if still failing, warning and action skipped, lock written without it |
| Auth required | Yes | No | Warning; action skipped, lock written without it — never retried |
| Action not found (404) | No | No | Hard failure; command exits with error |
| Server error (5xx) | No | No | Hard failure; command exits with error |

Because rate limiting is the only retryable condition, it is the only one whose user experience begins with a retry. A rate-limited request is retried first and, only after the retry budget is spent, classified as skippable and skipped.

#### Scenario: Rate limiting is both skippable and retryable

- **GIVEN** resolution fails because the forge's rate limit is exhausted
- **WHEN** the error is classified
- **THEN** it is skippable, so the run continues and the lock is written without that action
- **AND** it is retryable, so a retrying caller may re-issue the request

#### Scenario: Missing authorization is skippable but not retryable

- **GIVEN** resolution fails because no credential is configured for the forge
- **WHEN** the error is classified
- **THEN** it is skippable, so the run continues and the lock is written without that action
- **AND** it is NOT retryable, because repeating the request without a credential cannot succeed

#### Scenario: Strict errors are neither skippable nor retryable

- **GIVEN** resolution fails with a not-found or server error
- **WHEN** the error is classified
- **THEN** it is neither skippable nor retryable, and the command fails

### Rule: Skippable errors produce warnings; strict errors produce failures

When resolution encounters errors for multiple actions, each error is classified independently. Skippable errors are logged as warnings and those actions are skipped. Only strict (non-skippable) errors cause the command to fail.

#### Scenario: All errors are recoverable

- **GIVEN** all resolution failures are rate-limited or auth-required
- **WHEN** the command completes
- **THEN** warnings are logged and the lock is written without those entries

#### Scenario: Mix of recoverable and strict errors

- **GIVEN** some failures are skippable and some are strict
- **WHEN** the command completes
- **THEN** skippable errors are logged as warnings
- **AND** the command fails reporting only the strict errors

#### Scenario: All errors are strict

- **GIVEN** all resolution failures are not-found or server errors
- **WHEN** the command completes
- **THEN** the command fails with all strict errors reported

---

## Guardrail: Failure messages

### Requirement: Resolution failures state the remedy, not the vendor

A skipped resolution is often the only output a user reads, so its message MUST lead with what the user can do about it. The message MUST name the forge the request went to — a workflow may reference more than one — but the forge MUST be carried as data on the error rather than written into the message of a forge-specific variant, so that adding a forge adds no failure variants.

The message MUST NOT be the only signal a user gets that a credential is missing; the existing pre-run warning about unauthenticated access is unchanged.

#### Scenario: Rate limit message names the forge and the remedy

- **GIVEN** resolution is skipped because the GitHub rate limit is exhausted
- **WHEN** the skip is reported
- **THEN** the message identifies GitHub as the forge that was rate limited
- **AND** the message names the environment variable that raises the limit
- **AND** the message does NOT claim when the limit resets, because that is not read from the response

#### Scenario: Auth message names the forge and the remedy

- **GIVEN** resolution is skipped because no GitHub credential is configured
- **WHEN** the skip is reported
- **THEN** the message identifies GitHub as the forge requiring authorization
- **AND** the message names the environment variable the user must set

#### Scenario: A second forge reuses the same failure variants

- **GIVEN** a forge other than GitHub reports rate limiting
- **WHEN** the error is constructed
- **THEN** it uses the same rate-limit variant with a different forge value
- **AND** no forge-specific failure variant is added
