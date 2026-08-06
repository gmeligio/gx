## RENAMED Requirements

- FROM: `### Requirement: SHA descriptions are deduplicated within a single run`
- TO: `### Requirement: Registry lookups are deduplicated within a single run`

## MODIFIED Requirements

### Requirement: Registry lookups are deduplicated within a single run

Every query a command makes against the version registry SHALL be issued at most once per distinct set of arguments for the duration of that run. This covers all four registry queries: looking up the commit for a version, listing the tags that point at a SHA, listing all tags for an action, and describing a SHA. A repeated query with identical arguments SHALL be served from the run's memory without a network request, and SHALL return the same result the first query returned.

The user who benefits is anyone running `gx init`, `gx tidy`, or `gx upgrade` against a repository that references the same action from more than one workflow, or at more than one version — the common case. What they notice is that the command makes fewer GitHub API requests, so it is less likely to exhaust the 60 requests/hour unauthenticated limit and degrade into skip warnings with an incomplete lock.

The memory SHALL be discarded when the command finishes, so every run observes current registry state. Only successful results SHALL be reused; a failed query SHALL be retried if asked again, so a transient failure never poisons the rest of the run.

#### Scenario: Same action referenced by many workflows

- **GIVEN** ten workflows that all reference `actions/checkout@v4`
- **WHEN** `gx tidy` runs
- **THEN** the commit for `actions/checkout` at `v4` is looked up from the registry exactly once
- **AND** every workflow is updated with that same commit

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
