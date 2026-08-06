## ADDED Requirements

### Requirement: Diagnostic producers do not embed kind-nouns

Upstream producers — lint rules and command reports — SHALL emit structured data
(identifier, version, level, location) and SHALL NOT embed the noun naming the kind
of thing being described into a diagnostic message. Choosing user-facing vocabulary
is reserved to the rendering boundary; a producer that has already committed to a
noun has taken that choice away from it.

**User value:** the maintainer running `gx lint` reads one consistent voice across
every rule, instead of `action actions/checkout uses tag reference…` from one rule
and `actions/checkout SHA … not found` from the next. It also keeps gx from
asserting the wrong word: the same rules now fire on composite action files, and
are planned to fire on GitLab CI components, so a message that hardcodes "action"
is a message that will eventually mislabel the artifact it just flagged.

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
- **THEN** no diagnostic message opens with a kind-noun, so all rules read alike
- **AND** consistency is achieved by absence — a run in which every message carried
  the same noun would be uniform but still violates this requirement
