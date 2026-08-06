## ADDED Requirements

### Requirement: gx lint performs no network I/O

`gx lint` SHALL be fully offline. It SHALL NOT issue any network request, and its verdict
SHALL depend only on files in the repository. Running `gx lint` twice on an unchanged
working tree SHALL produce the same diagnostics regardless of network availability, API
rate limits, or the presence of a GitHub token.

Networked, time-varying checks belong to `gx audit`, which is a separate command precisely
so this guarantee holds.

**User value:** the developer running `gx lint` in a pre-commit hook, on a plane, or in a
network-isolated CI runner gets the same answer every time and never waits on an API. This
was already true in practice; making it a requirement means a future rule cannot quietly
take it away, which would turn a fast, deterministic gate into a flaky one.

#### Scenario: Lint succeeds with no network access
- **GIVEN** a repository that exits 0 on `gx lint`
- **AND** no network connectivity is available
- **WHEN** the user runs `gx lint`
- **THEN** the command produces the same diagnostics as it would with connectivity
- **AND** exits with the same code

#### Scenario: Lint verdict does not depend on a token
- **GIVEN** a repository with any set of workflows
- **WHEN** the user runs `gx lint` with `GITHUB_TOKEN` set, and again with it unset
- **THEN** both runs produce identical diagnostics and identical exit codes
