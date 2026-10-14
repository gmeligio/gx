## ADDED Requirements

### Requirement: Resolution yields repository identity and composite children

Resolving a repository action for the lock SHALL also yield the repository's numeric owner and repo IDs and, when the action at the resolved commit is a composite action, the remote `uses:` references inside its definition (`action.yml`, else `action.yaml`, at the action's subpath). Both lookups SHALL go through the same caching and retrying registry as version lookups, and follow the same error classification.

**User value:** the lockfile gx writes is complete enough for GitHub's runner to enforce without the user running a second tool.

#### Scenario: Leaf action has no children
- **GIVEN** an action whose `action.yml` declares `runs.using: node24`
- **WHEN** it is resolved
- **THEN** no children are recorded

#### Scenario: Rate-limited child lookup leaves the pin incomplete
- **GIVEN** a composite action whose `action.yml` read is rate-limited after retries
- **WHEN** the user runs `gx tidy`
- **THEN** gx warns and writes the pin without `uses:`
- **AND** the next `gx tidy` completes it
