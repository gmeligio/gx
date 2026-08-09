## ADDED Requirements

### Requirement: The offline/networked split is enforced, not conventional

`gx lint` SHALL be fully offline: it SHALL NOT issue any network request, and its verdict
SHALL depend only on files in the repository. `gx audit` is where networked, time-varying
checks live.

This separation SHALL be enforced mechanically rather than by convention. The build SHALL
fail if lint-rule code acquires a dependency on an HTTP client or on the GitHub API modules,
and SHALL likewise fail if audit code acquires a dependency on the workflow scanner.

**User value:** the developer running `gx lint` in a pre-commit hook, on a plane, or in a
network-isolated CI runner gets the same answer every time and never waits on an API. The
guarantee already held in practice; the change here is that it can no longer be lost by
accident. Without enforcement, the first lint rule that reaches for the API to "just check
one thing" silently converts a fast, deterministic gate into a flaky, credential-dependent
one, and nothing catches it before release.

#### Scenario: A lint rule that makes a network call fails the build
- **GIVEN** a source file under `src/lint/` that imports `reqwest` or the GitHub API client
- **WHEN** the project's test suite runs
- **THEN** the code-health check fails, naming the file and the forbidden dependency

#### Scenario: Audit code that walks workflow files fails the build
- **GIVEN** a source file under `src/audit/` that imports the workflow scanner or the
  parsed-workflow types
- **WHEN** the project's test suite runs
- **THEN** the code-health check fails, naming the file and the forbidden dependency

#### Scenario: The enforcement itself is proven to fail
- **GIVEN** the code-health check that enforces this split
- **WHEN** a forbidden import is introduced in either direction
- **THEN** the check fails rather than passing silently
