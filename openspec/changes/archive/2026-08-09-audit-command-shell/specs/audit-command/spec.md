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

#### Scenario: Absent lock file has nothing to audit
- **GIVEN** a repository with no `gx.lock` file
- **AND** a GitHub token is available
- **WHEN** the user runs `gx audit`
- **THEN** no finding is produced
- **AND** the command exits with code 0

#### Scenario: Lock file with no entries has nothing to audit
- **GIVEN** a repository whose `gx.lock` exists but records no entries
- **AND** a GitHub token is available
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
- **AND** stdout contains no JSON document at all — not an empty one, and not one
  carrying an error field alongside zero findings

---

### Requirement: gx audit --json emits one machine-readable document

`gx audit --json` SHALL print exactly one JSON document to stdout. It SHALL suppress every
human-facing line that would otherwise interleave with that document on stdout — the CI
notice, the log-file path, and the summary — and SHALL suppress the stderr spinner, so
neither stream carries progress noise a consumer must strip.

The document SHALL have this shape, and these key names are the command's machine-readable
contract — once shipped, a consumer may rely on them:

```json
{
  "findings": [{ "check": "mutable-ref", "level": "warn", "message": "..." }],
  "error_count": 0,
  "warning_count": 1
}
```

`level` SHALL use the same lowercase spellings as gx's existing severity vocabulary
(`error`, `warn`). `findings` SHALL be present and empty — never omitted or null — when
nothing was found, so a consumer can index it unconditionally.

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

#### Scenario: JSON mode suppresses progress output on both streams
- **GIVEN** any repository
- **WHEN** the user runs `gx audit --json`
- **THEN** stdout contains no CI notice, log-file path, or summary line outside the JSON
  document
- **AND** no spinner is emitted on stderr

#### Scenario: JSON mode writes no local log file
- **GIVEN** the `CI` environment variable is not set, so a local run would normally log
- **WHEN** the user runs `gx audit --json`
- **THEN** no log file is created for the run

---

### Requirement: Checks are identified by a stable kebab-case name

Every audit finding SHALL carry the name of the check that produced it, drawn from one
closed set of kebab-case identifiers. The name shown in human output and the name carried in
`--json` SHALL be the same string.

**User value:** the user who sees a finding can search for its name, and the CI engineer
filtering `--json` output matches on the same identifier the terminal showed them. One
source for both names means they cannot drift apart across releases.

Audit check names SHALL occupy a namespace separate from lint rule names. An audit check
name SHALL NOT be accepted as a key in the `[lint.rules]` configuration table.

#### Scenario: Finding names match across output modes
- **GIVEN** a repository whose lock contains a branch-resolved entry
- **WHEN** the user runs `gx audit` and `gx audit --json`
- **THEN** both outputs name the check `mutable-ref`, character for character

#### Scenario: An audit check name is not valid lint configuration
- **GIVEN** `gx.toml` contains `mutable-ref = { level = "error" }` under `[lint.rules]`
- **WHEN** the manifest is parsed
- **THEN** parsing fails with an error identifying the unrecognized rule name

---

### Requirement: mutable-ref check reports lock entries pinned to a branch

The system SHALL provide a `mutable-ref` check that reports, at **warning** severity, every
`gx.lock` entry whose resolved reference is a branch. Entries resolved to a tag, a release,
or a bare commit SHALL NOT produce a finding.

The severity is `warn`, not `error`, so the check does not fail a build. Tracking a branch is
a configuration gx itself supports and records; the user may have chosen it deliberately.

**User value:** the maintainer who pinned `@main` believes they have a pin. They do not — the
SHA recorded in the lock today is not what `@main` resolves to tomorrow, so the reproducibility
the lock file exists to provide does not hold for that entry, and neither does any SHA-based
guarantee gx makes elsewhere. The finding tells them which entries are exceptions. It warns
rather than errors because gx cannot tell a deliberate branch-tracker from an accidental one,
and failing the build on a supported configuration would be gx overruling the user.

#### Scenario: Branch-resolved entry produces a warning
- **GIVEN** a `gx.lock` entry whose resolved reference is a branch
- **WHEN** the user runs `gx audit`
- **THEN** a `mutable-ref` finding is produced at warning severity, naming the action
- **AND** the command exits with code 0, because no error-level finding was produced

#### Scenario: Tag, release, and commit pins produce no finding
- **GIVEN** a `gx.lock` whose entries resolve only to tags, releases, or bare commits
- **WHEN** the user runs `gx audit`
- **THEN** no `mutable-ref` finding is produced

---

### Requirement: Advisory lookups go through a substitutable seam

The system SHALL query GitHub security advisories through an abstraction with two
implementations: one issuing real API requests, and one returning canned results for tests.
Checks SHALL depend on the abstraction, never on an HTTP client directly.

Advisory queries SHALL use GitHub's GraphQL API, which is why `gx audit` requires a token:
that endpoint rejects unauthenticated requests outright.

**User value:** indirect but load-bearing. It is what lets every advisory-consuming check be
tested offline and deterministically. Without it, the checks that decide whether a user's
action is vulnerable would be exercised only against the live API — meaning they would be
tested rarely, flakily, or not at all, and a bug in the code that decides "vulnerable or not"
would reach users. For a security command, an untestable check is an untrustworthy one.

#### Scenario: Checks run against canned advisory data with no network
- **GIVEN** the test implementation of the advisory seam, seeded with known advisories
- **WHEN** an advisory-consuming check runs against it
- **THEN** the check produces its findings with no network request issued

#### Scenario: A failed advisory query surfaces as an error value
- **GIVEN** the advisory API returns an authentication, rate-limit, or malformed response
- **WHEN** the seam's real implementation processes that response
- **THEN** it yields an error value, never an empty or partial list of advisories

The end-to-end consequence — `gx audit` exiting non-zero on a failed advisory query — is
specified by the first check that consumes the seam. This change ships no such check, so
asserting it here would test a code path nothing reaches.
