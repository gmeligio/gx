## ADDED Requirements

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

### Requirement: The wait before a retry honors the forge's reset time but is capped

The system SHALL derive the wait from the forge's reported reset time when that time is available, and SHALL clamp every wait to a fixed maximum. When the reported reset time exceeds that maximum, the system SHALL NOT wait — the rate-limit error is returned immediately so the user gets their partial result and a warning now, rather than a stalled terminal.

The forge reports when its quota resets. Waiting exactly that long recovers a window that is about to roll over; but an exhausted unauthenticated quota can reset up to an hour out, and blocking a user's terminal for an hour is worse than failing.

When no reset time is available, the system SHALL fall back to a fixed backoff schedule that increases between attempts.

A reset time that has already passed, or that is reported as being in the past because the local clock runs ahead of the forge's, SHALL be treated as a zero-or-minimal wait rather than an error or a negative duration.

#### Scenario: A near reset time is waited out

- **GIVEN** the forge reports its quota resets a few seconds from now
- **WHEN** a rate-limited request is retried
- **THEN** the retry waits approximately until that reset time before reissuing the request

#### Scenario: A distant reset time is not waited on

- **GIVEN** the forge reports its quota resets an hour from now
- **WHEN** a request fails with a rate-limit error
- **THEN** no wait occurs and the rate-limit error is returned immediately
- **AND** the user gets their partial lock and warning without a stalled terminal

#### Scenario: A reset time in the past does not produce a negative wait

- **GIVEN** the forge reports a reset time earlier than the local clock's current time
- **WHEN** a rate-limited request is retried
- **THEN** the retry proceeds without waiting rather than failing or waiting a nonsensical duration

#### Scenario: A missing reset time falls back to increasing backoff

- **GIVEN** the forge reports no reset time with its rate-limit response
- **WHEN** a request is retried more than once
- **THEN** each successive wait is at least as long as the previous one
- **AND** every wait remains within the fixed maximum

### Requirement: A retry wait is announced to the user

The system SHALL report each retry wait through the same progress channel that carries other resolution progress, stating that the run is waiting on the forge's rate limit and for how long. A command that pauses for several seconds with no explanation reads as a hang.

The announcement SHALL travel the existing progress channel rather than a new one, so it inherits that channel's existing suppression in `--json` mode and cannot corrupt the single JSON document on stdout. No scenario is stated for the `--json` case: progress suppression predates this change and holds whether or not a retry occurs, so any such scenario would pass identically before and after.

#### Scenario: The user sees why the command paused

- **GIVEN** a rate-limited request that will be retried after a wait
- **WHEN** the wait begins
- **THEN** a progress message naming the rate limit and the wait duration is emitted before the process sleeps

#### Scenario: The announcement precedes the wait rather than following it

- **GIVEN** a rate-limited request that will be retried after a wait
- **WHEN** the user watches the command
- **THEN** the explanation appears before the pause, not after it, so the pause is never an unexplained stall

## MODIFIED Requirements

### Rule: Resolution errors are classified as recoverable or strict

| Error condition | Classification | User experience |
|---|---|---|
| Rate limited | Retryable, then recoverable | Retried within a bounded budget (unless the forge reports a reset beyond the cap, in which case it is not retried at all); if still failing, warning and action skipped, lock written without it |
| Auth required | Recoverable | Warning; action skipped, lock written without it — never retried |
| Action not found (404) | Strict | Hard failure; command exits with error |
| Server error (5xx) | Strict | Hard failure; command exits with error |

Retryability and recoverability are distinct questions. Recoverability decides whether the user sees a warning or a hard failure. Retryability decides whether repeating the identical request could plausibly change the outcome — true only for quota exhaustion. A rate-limited request is therefore retried first and, only after the retry budget is spent, classified as recoverable and skipped.
