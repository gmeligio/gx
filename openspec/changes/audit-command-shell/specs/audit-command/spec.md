## ADDED Requirements

### Requirement: Audit the locked action set with gx audit

The system SHALL provide a `gx audit` subcommand that checks the actions recorded in
`gx.lock` against networked, time-varying knowledge (security advisories and repository
state) and reports findings without modifying any file.

`gx audit` SHALL derive the set of actions it checks from `gx.lock` entries alone. It SHALL
NOT read, parse, or traverse workflow files.

**User value:** the maintainer who followed the ecosystem's advice and pinned every action
to a SHA is precisely the user Dependabot and Renovate stop covering. `gx audit` is how they
learn a pinned SHA has become dangerous. Deriving the action set from the lock rather than
from workflows means every future improvement to how gx discovers actions — composite
traversal, job-level `uses:` — reaches audit at no cost, and audit can never drift into a
second, staler notion of which actions the project uses.

#### Scenario: Clean lock reports nothing and exits zero
- **GIVEN** a repository whose `gx.lock` entries have no findings
- **WHEN** the user runs `gx audit`
- **THEN** the command prints a summary stating no issues were found
- **AND** exits with code 0

#### Scenario: Error-level finding fails the command
- **GIVEN** a repository where at least one check produces an error-level finding
- **WHEN** the user runs `gx audit`
- **THEN** the command prints every finding
- **AND** exits with code 1

#### Scenario: Warning-level findings alone do not fail the command
- **GIVEN** a repository where checks produce only warn-level findings
- **WHEN** the user runs `gx audit`
- **THEN** the command prints every finding
- **AND** exits with code 0

#### Scenario: Empty lock has nothing to audit
- **GIVEN** a repository with no `gx.lock`, or a `gx.lock` with no entries
- **WHEN** the user runs `gx audit`
- **THEN** no finding is produced
- **AND** the command exits with code 0

#### Scenario: Workflow contents do not change the audited set
- **GIVEN** a repository whose `gx.lock` records exactly one action
- **AND** whose workflow files reference a different, unlocked action
- **WHEN** the user runs `gx audit`
- **THEN** only the locked action is audited
- **AND** the unlocked action referenced solely in workflows produces no finding

---

### Requirement: A missing GitHub token is a loud, actionable failure

`gx audit` requires a GitHub API token. When no token is available, the system SHALL abort
before running any check, SHALL print an error naming the environment variable to set, and
SHALL exit with a non-zero code. It SHALL NOT report a clean or partial result.

This differs deliberately from gx's other GitHub-backed commands, which tolerate an absent
token and fall back to unauthenticated requests.

**User value:** the CI engineer who adds `gx audit` to a pipeline and forgets the token gets
a red build with the fix in the message, not a green build that certifies nothing. For a
security command, a false "clean" is the worst possible outcome — it converts an absent
check into an affirmative, wrong assurance.

#### Scenario: No token configured
- **GIVEN** the `GITHUB_TOKEN` environment variable is not set
- **WHEN** the user runs `gx audit`
- **THEN** the command exits with a non-zero code
- **AND** the error message names `GITHUB_TOKEN` as the variable to set
- **AND** no finding output and no clean summary is printed

#### Scenario: No token configured in JSON mode
- **GIVEN** the `GITHUB_TOKEN` environment variable is not set
- **WHEN** the user runs `gx audit --json`
- **THEN** the command exits with a non-zero code
- **AND** stdout does not contain a JSON document reporting a clean result

---

### Requirement: gx audit --json emits one machine-readable document

`gx audit --json` SHALL print exactly one JSON document to stdout and SHALL suppress every
human-facing line — spinner, CI notice, and log-file path — that would otherwise interleave
with it. The document SHALL carry the findings and the counts a consumer needs to decide
whether the run passed.

**User value:** the CI engineer piping `gx audit --json` into `jq` or a PR-comment step gets
a parseable document on the first try, with no progress noise to strip.

#### Scenario: JSON mode on a clean repository
- **GIVEN** a repository with no findings
- **WHEN** the user runs `gx audit --json`
- **THEN** stdout parses as a single JSON document
- **AND** the document reports zero findings

#### Scenario: JSON mode with findings
- **GIVEN** a repository with at least one finding
- **WHEN** the user runs `gx audit --json`
- **THEN** stdout parses as a single JSON document listing each finding with its check name,
  severity, and message

#### Scenario: JSON mode suppresses progress output
- **GIVEN** any repository
- **WHEN** the user runs `gx audit --json`
- **THEN** stdout contains no CI notice, log-file path, or summary line outside the JSON
  document

---

### Requirement: Checks are identified by a stable kebab-case name

Every audit finding SHALL carry the name of the check that produced it, drawn from one
closed set of kebab-case identifiers. The name shown in human output and the name carried in
`--json` SHALL be the same string.

**User value:** the user who sees a finding can search for its name, and the CI engineer
filtering `--json` output matches on the same identifier the terminal showed them. One
source for both names means they cannot drift apart across releases.

#### Scenario: Finding names match across output modes
- **GIVEN** a repository producing a finding from a given check
- **WHEN** the user runs `gx audit` and `gx audit --json`
- **THEN** the check name in the human output equals the check name in the JSON document
