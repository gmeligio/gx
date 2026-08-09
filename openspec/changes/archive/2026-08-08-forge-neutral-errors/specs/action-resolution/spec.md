> **Note on the enclosing guardrail heading.** These rules sit under
> `## Guardrail: Error classification (recoverable vs. strict)`, whose title and
> rationale paragraph use "recoverable" for a property this change renames to
> "skippable", so at sync time that heading becomes
> `## Guardrail: Error classification (skippable vs. strict)` and its rationale
> paragraph is restated in that vocabulary. Left as-is, the section title would
> misstate its own content.

> Both rule headers below are reproduced verbatim from the main spec so they match
> exactly. Their titles keep the word "recoverable" while their bodies now say
> "skippable"; at sync time both titles are restated in the skippable/strict
> vocabulary along with the guardrail heading above. The rename is deliberately not
> expressed as a RENAMED delta, because pairing a rename with a MODIFIED block for
> the same heading would ask the sync step to process one heading twice.

## MODIFIED Requirements

### Rule: Resolution errors are classified as recoverable or strict

Classification answers one question: **may the current run continue without this
action?** A skippable error is reported as a warning and the lock is written
without that entry. A non-skippable (strict) error fails the command.

Skippable does NOT mean retryable. A missing or rejected credential is skippable —
the run continues without that action — but repeating the identical request cannot
produce a different outcome. The classification carries no promise that a caller
may re-issue a skippable request; a caller that retries MUST decide that from the
specific failure, not from skippability.

| Error condition | Classification | User experience |
|---|---|---|
| Rate limited | Skippable | Warning; action skipped, lock written without it |
| Auth required | Skippable | Warning; action skipped, lock written without it |
| Action not found (404) | Strict | Hard failure; command exits with error |
| Server error (5xx) | Strict | Hard failure; command exits with error |

#### Scenario: Rate limiting is skippable

- **GIVEN** resolution fails because the forge's rate limit is exhausted
- **WHEN** the error is classified
- **THEN** it is skippable, so the run continues and the lock is written without that action

#### Scenario: Missing authorization is skippable

- **GIVEN** resolution fails because no credential is configured for the forge
- **WHEN** the error is classified
- **THEN** it is skippable, so the run continues and the lock is written without that action
- **AND** the classification does not imply the request may be retried

#### Scenario: Strict errors are not skippable

- **GIVEN** resolution fails with a not-found or server error
- **WHEN** the error is classified
- **THEN** it is not skippable, and the command fails

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
