## MODIFIED Requirements

### Rule: Resolution errors are classified as recoverable or strict

Classification is two-dimensional. A single "recoverable" bit cannot express both
of the decisions callers make, so each resolution error carries two independent
properties:

- **Skippable** — may the current run continue without this action? A skippable
  error is reported as a warning and the lock is written without that entry. A
  non-skippable (strict) error fails the command.
- **Retryable** — would repeating the identical request plausibly succeed? Only a
  retryable error may be re-issued by a caller that retries.

The two are independent: rate limiting is both skippable and retryable, whereas a
missing or rejected credential is skippable (the run continues without that action)
but never retryable, because repeating a request with the same absent credential
cannot produce a different outcome.

| Error condition | Skippable | Retryable | User experience |
|---|---|---|---|
| Rate limited | Yes | Yes | Warning; action skipped, lock written without it |
| Auth required | Yes | No | Warning; action skipped, lock written without it |
| Action not found (404) | No | No | Hard failure; command exits with error |
| Server error (5xx) | No | No | Hard failure; command exits with error |

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

### Rule: Recoverable errors produce warnings; strict errors produce failures

When resolution encounters errors for multiple actions, each error is classified
independently. Skippable errors are logged as warnings and those actions are
skipped. Only strict (non-skippable) errors cause the command to fail.

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

## ADDED Requirements

### Requirement: Resolution failures state the remedy, not the vendor

A skipped resolution is often the only output a user reads, so its message MUST
lead with what the user can do about it. The message MUST name the forge the
request went to — a workflow may reference more than one — but the forge MUST be
carried as data on the error rather than written into the message of a
forge-specific variant, so that adding a forge adds no failure variants.

The message MUST NOT be the only signal a user gets that a credential is missing;
the existing pre-run warning about unauthenticated access is unchanged.

#### Scenario: Rate limit message names the forge and the remedy

- **GIVEN** resolution is skipped because the GitHub rate limit is exhausted
- **WHEN** the skip is reported
- **THEN** the message identifies GitHub as the forge that was rate limited
- **AND** the message tells the user the limit resets and that a token raises it

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
