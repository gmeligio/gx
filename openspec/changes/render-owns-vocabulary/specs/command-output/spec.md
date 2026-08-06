## ADDED Requirements

### Requirement: The render boundary owns user-facing vocabulary

Upstream producers — lint rules and command reports — SHALL emit structured data
(identifier, version, level, location) and SHALL NOT embed the noun naming the kind
of thing being described into a diagnostic message. The rendering boundary composes
the sentence the user reads.

**User value:** the maintainer running `gx lint` reads one consistent voice across
every rule, instead of `action actions/checkout uses tag reference…` from one rule
and `actions/checkout SHA … not found` from the next. The same constraint is what
lets gx describe a composite action file, or a future GitLab CI component, without
a rule hardcoding the wrong word for the artifact it just flagged.

This guardrail is load-bearing for the same reason as "Commands do not print
directly": a producer that composes its own sentence fragment reintroduces the
double-render and inconsistent-phrasing failures the single rendering boundary
exists to prevent.

#### Scenario: A rule reports a violation against an action
- **GIVEN** the `unpinned` rule flags `actions/checkout@v4` in `.github/workflows/ci.yml`
- **WHEN** the user runs `gx lint`
- **THEN** the diagnostic message names the identifier and the problem, e.g.
  `actions/checkout uses tag reference v4 instead of SHA pin`
- **AND** the message does not open with a noun naming the kind of thing
  (`action `, `workflow `, `component `)

#### Scenario: Every rule reports in the same voice
- **GIVEN** `gx lint` produces diagnostics from more than one rule in a single run
- **WHEN** the user reads the output
- **THEN** no diagnostic message carries a kind-noun prefix that another omits
