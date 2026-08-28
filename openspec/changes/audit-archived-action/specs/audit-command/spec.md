## ADDED Requirements

### Requirement: Audit reports dependency health, not vulnerabilities alone

`gx audit` SHALL report findings about the health of a locked dependency even when no
vulnerability is known, provided the finding rests on networked, time-varying knowledge
about the dependency rather than on a deterministic fact about the user's own files.

This is the boundary that separates `gx audit` from `gx lint`, and it is drawn around
*where the knowledge lives*, not around *whether a CVE exists*. A fact that gx can settle
by reading the user's repository belongs to lint: it is offline, deterministic, and changes
only when the user edits a file. A fact that requires asking GitHub, and whose answer can
change while the repository sits untouched, belongs to audit.

**User value:** the user asking "why is a repository being archived reported by a *security*
command?" deserves a straight answer, and it is this: an archived repository is the reason a
future vulnerability would be unfixable. Reporting it only once an advisory lands is
reporting it too late — at that point the user's options have already narrowed to one, and
they have lost the lead time in which migration was a planned piece of work rather than an
incident. The precedent is ordinary: `npm audit` reports deprecated packages, and
`cargo audit` reports unmaintained crates, neither of which is a CVE.

The cost of drawing the boundary here rather than at "vulnerabilities only" is bounded:
`audit-command-shell` ships no configuration surface, so a check that proves unwelcome can
be removed or moved without breaking a user's configuration file.

#### Scenario: A dependency-health finding is reported by audit, not lint
- **GIVEN** a `gx.lock` entry whose action comes from an archived repository
- **AND** a GitHub token is available
- **WHEN** the user runs `gx audit`
- **THEN** a finding is produced, even though no security advisory affects the action
- **WHEN** the user runs `gx lint` on the same repository
- **THEN** no finding about the archived repository is produced, and no network request is
  issued

---

### Requirement: archived-action check reports actions from archived repositories

The system SHALL provide an `archived-action` check that reports, at **warning** severity,
every `gx.lock` entry whose action is published from a repository GitHub reports as
archived. An entry whose repository is not archived SHALL NOT produce a finding.

The finding SHALL name the action and SHALL state the date of the repository's last push,
so the user can judge how long the dependency has been frozen. The finding SHALL direct the
user toward migrating to a maintained alternative rather than toward upgrading, because for
an archived repository no newer version can be published.

The severity is `warn`, not `error`. An archived dependency is a risk signal about the
future, not an exploit in the present; failing a build over it would stop work that is not
yet unsafe, and would give users a reason to disable the command.

**User value:** the maintainer still using `actions/setup-ruby` or `actions/create-release`
believes they have an ordinary, if old, dependency. They do not. If a vulnerability is
disclosed in it tomorrow, no patched version will ever be published, and their only
remaining option is a migration they have not planned or budgeted. The finding converts a
latent, invisible risk into a scheduled piece of maintenance, and the last-push date tells
them how urgent it is: a repository archived last month reads differently from one frozen
since 2021.

#### Scenario: An action from an archived repository produces a warning
- **GIVEN** a `gx.lock` entry whose action's repository is archived
- **AND** a GitHub token is available
- **WHEN** the user runs `gx audit`
- **THEN** an `archived-action` finding is produced at warning severity
- **AND** the finding names the action
- **AND** the finding states the date the repository was last pushed to
- **AND** the finding tells the user to migrate to a maintained alternative
- **AND** the command exits with code 0, because no error-level finding was produced

#### Scenario: An action from an active repository produces no finding
- **GIVEN** a `gx.lock` entry whose action's repository is not archived
- **AND** a GitHub token is available
- **WHEN** the user runs `gx audit`
- **THEN** no `archived-action` finding is produced for that entry

#### Scenario: The check name is the same in human and JSON output
- **GIVEN** a `gx.lock` entry whose action's repository is archived
- **WHEN** the user runs `gx audit` and `gx audit --json`
- **THEN** both outputs name the check `archived-action`, character for character

---

### Requirement: Subpath actions are checked against their base repository

The system SHALL look up repository metadata for an action's base repository. When a locked
action names a path inside a repository — for example `github/codeql-action/upload-sarif` —
the lookup SHALL use `github/codeql-action`, not the full action identifier.

**User value:** without this, every subpath action would fail its lookup, because no
repository exists at the subpath. Since a failed lookup is an error rather than a silent
pass, the user would see `gx audit` fail on a workflow that is perfectly fine — and
`github/codeql-action/*` is among the most widely used actions in the ecosystem, so this
would be most users' first experience of the command.

#### Scenario: A subpath action is looked up by its base repository
- **GIVEN** a `gx.lock` entry for `github/codeql-action/upload-sarif`
- **WHEN** the `archived-action` check runs
- **THEN** repository metadata is requested for `github/codeql-action`
- **AND** no request is made for a repository named after the full action identifier

#### Scenario: A subpath action in an archived repository is reported by its full name
- **GIVEN** a `gx.lock` entry for a subpath action whose base repository is archived
- **WHEN** the user runs `gx audit`
- **THEN** an `archived-action` finding is produced
- **AND** the finding names the action as the user wrote it, including the subpath, so they
  can find it in their workflow

---

### Requirement: A failed repository-metadata lookup is an error, not a clean result

The system SHALL surface an error-level finding, naming the action and the reason, whenever
repository metadata cannot be retrieved — because the request failed, was rejected, was rate
limited, was not found, or could not be parsed. It SHALL NOT treat such a failure as
evidence that the repository is not archived.

A repository that has been renamed or deleted answers with not-found, and is reported the
same way. gx does not follow a rename: a lock naming a repository that is no longer there
is itself worth the user's attention.

**User value:** the same principle that makes a missing token a hard failure rather than an
empty report. "We could not check" and "we checked and it is fine" are different statements,
and only one of them is a reason to stop worrying. Silently collapsing the first into the
second would make `gx audit` least reliable exactly when the network or the user's token is
in trouble — the moments a user is most likely to be relying on a green result.

#### Scenario: A failed lookup does not read as clean
- **GIVEN** a `gx.lock` entry whose repository metadata lookup fails
- **WHEN** the user runs `gx audit`
- **THEN** an error-level finding is produced naming the action and the reason the lookup
  failed
- **AND** the command exits with code 1

#### Scenario: One failed lookup does not suppress other findings
- **GIVEN** a `gx.lock` with one entry whose lookup fails and one entry whose repository is
  archived
- **WHEN** the user runs `gx audit`
- **THEN** both the error-level lookup failure and the warning-level archived finding are
  reported
- **AND** this holds whichever of the two entries is checked first

#### Scenario: A repository that no longer exists is an error, not a clean result
- **GIVEN** a `gx.lock` entry whose repository has been renamed or deleted, so the lookup
  answers not-found
- **WHEN** the user runs `gx audit`
- **THEN** an error-level finding is produced naming the action and the not-found reason

---

### Requirement: Repository metadata lookups go through a substitutable seam

The system SHALL retrieve repository metadata through an abstraction with two
implementations: one issuing real API requests, and one returning canned results for tests.
Checks SHALL depend on the abstraction, never on an HTTP client directly.

This is a separate seam from the advisory seam introduced by `audit-command-shell`. That one
answers "what advisories affect this package" over GraphQL; this one answers "what is the
state of this repository" over REST. They are different questions against different
endpoints, and merging them would force each caller to depend on the half it does not use.

**User value:** indirect but load-bearing, for the same reason the advisory seam is. Without
it, the code deciding whether a user's dependency is archived could only be exercised
against the live API — meaning it would be tested rarely and flakily, and the check that
tells a user their dependency is frozen could itself be quietly broken.

#### Scenario: The check runs against canned metadata with no network
- **GIVEN** the test implementation of the metadata seam, seeded with a known repository
  state
- **WHEN** the `archived-action` check runs against it
- **THEN** the check produces its findings with no network request issued

#### Scenario: A failed metadata request surfaces as an error value
- **GIVEN** the repository API returns an authentication, rate-limit, not-found, or
  malformed response
- **WHEN** the seam's real implementation processes that response
- **THEN** it yields an error value, never a default or assumed repository state
