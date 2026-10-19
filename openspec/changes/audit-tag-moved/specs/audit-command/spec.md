## ADDED Requirements

### Requirement: tag-moved check reports tags that no longer point where they were pinned

The system SHALL provide a `tag-moved` check that, for every `gx.lock` entry resolved to a tag
or a release, resolves that tag live against GitHub and compares the commit it points at now
with the SHA the lock recorded. When the two differ, the system SHALL produce a finding at
**error** severity that names the action, the version label, the SHA recorded in the lock, and
the SHA the tag points at now.

The tag resolved SHALL be the version label the lock records for the entry — the same string
gx wrote when it pinned the action.

An action identified by a subpath, such as `github/codeql-action/upload-sarif`, SHALL have its
tag resolved against the repository named by the first two path segments. Tags belong to a
repository, not to a directory inside it, so resolving against the full identifier would name a
repository that does not exist — and under the rule below that miss would be reported as an
unverifiable tag, accusing a healthy dependency.

Annotated tags SHALL be dereferenced to their target commit before comparison. An annotated
tag that has not moved SHALL NOT produce a finding.

Entries resolved to a branch, or to a bare commit with no version, SHALL be skipped. Neither
names a tag whose movement could be measured. An entry SHALL likewise be skipped when its
version label is itself a commit SHA, whatever its recorded reference kind, because such a
label names no tag to resolve.

**User value:** every major Actions supply-chain compromise — `tj-actions/changed-files`
(CVE-2025-30066, 23,000+ repositories), `aquasecurity/trivy-action` (GHSA-69fq-xp46-6x23, 75 of
76 tags), `codfish/semantic-release-action`, `actions-cool/issues-helper` — has the same
signature: an attacker repoints existing tags at a malicious commit. A maintainer who pinned to
a SHA survives the attack but has no way to learn it happened; one whose workflow still names
the tag is running the attacker's code. Detecting movement requires knowing what the tag pointed
to *when it was pinned*, and `gx.lock` is the only record of that in this ecosystem. The
severity is `error`, not `warn`, because a moved tag is never a benign configuration choice the
way a branch pin is — it is either an upstream force-push or an attack, and both warrant
stopping a build.

Precision matters more than recall here: a false positive accuses a maintainer of a
supply-chain attack. That is why annotated tags are dereferenced and why branch and bare-commit
entries are skipped rather than guessed at.

#### Scenario: A moved tag is reported at error severity with both SHAs
- **GIVEN** a `gx.lock` entry resolved to a tag, recording SHA `aaa…`
- **AND** that tag now points at commit `bbb…` on GitHub
- **WHEN** the user runs `gx audit`
- **THEN** a `tag-moved` finding is produced at error severity
- **AND** the message names the action, the version label, the locked SHA, and the current SHA
- **AND** the command exits with code 1

#### Scenario: A tag that still points where it was pinned produces no finding
- **GIVEN** a `gx.lock` entry resolved to a tag, recording SHA `aaa…`
- **AND** that tag still points at commit `aaa…` on GitHub
- **WHEN** the user runs `gx audit`
- **THEN** a lookup IS issued for that tag
- **AND** no `tag-moved` finding is produced

#### Scenario: An unmoved annotated tag produces no finding
- **GIVEN** a `gx.lock` entry resolved to an annotated tag, recording the SHA of the commit
  the tag object targets
- **AND** the lookup resolves that tag to the commit the tag object targets, not to the tag
  object's own SHA
- **WHEN** the user runs `gx audit`
- **THEN** no finding is produced

#### Scenario: An entry whose version label is a commit SHA is skipped
- **GIVEN** a `gx.lock` entry recording `ref_type` of `tag` but whose version label is a
  40-character commit SHA rather than a tag name
- **WHEN** the user runs `gx audit`
- **THEN** no finding is produced for it
- **AND** no tag lookup is issued for it, so a nonexistent tag is never reported as missing

#### Scenario: A subpath action resolves against its repository root
- **GIVEN** a `gx.lock` entry for `github/codeql-action/upload-sarif` resolved to a tag
- **AND** that tag still points at the SHA the lock recorded
- **WHEN** the user runs `gx audit`
- **THEN** the tag is looked up against `github/codeql-action`
- **AND** no finding is produced, neither `tag-moved` nor `tag-unverified`

#### Scenario: A release-resolved entry is checked like a tag
- **GIVEN** a `gx.lock` entry whose resolved reference is a release
- **AND** the underlying tag now points at a different commit
- **WHEN** the user runs `gx audit`
- **THEN** a `tag-moved` finding is produced

#### Scenario: A branch-resolved entry is skipped, not misreported
- **GIVEN** a `gx.lock` entry whose resolved reference is a branch
- **WHEN** the user runs `gx audit`
- **THEN** no `tag-moved` finding is produced for it, whatever the branch now points at
- **AND** no tag lookup is issued for it

#### Scenario: A bare-commit entry is skipped, not misreported
- **GIVEN** a `gx.lock` entry whose resolved reference is a bare commit with no version tag
- **WHEN** the user runs `gx audit`
- **THEN** no `tag-moved` finding is produced for it
- **AND** no tag lookup is issued for it

#### Scenario: An entry whose lock records no reference kind is skipped
- **GIVEN** a `gx.lock` entry that records no `ref_type`
- **WHEN** the user runs `gx audit`
- **THEN** no `tag-moved` finding is produced for it, because gx cannot establish it names a tag

---

### Requirement: A tag that could not be verified is reported under its own check name

The system SHALL produce a finding for an entry whose live tag lookup fails — network error,
rate limit, rejected credentials, a malformed response, or a tag absent upstream — rather than
omitting it, at **error** severity so the command exits non-zero.

That finding SHALL carry the check name `tag-unverified`, distinct from `tag-moved`. A
`tag-moved` finding SHALL mean the tag was resolved and had moved; it SHALL NOT be used for an
entry gx could not resolve.

**User value:** two things. First, for a security check "I could not look" and "I looked and it
is fine" must never render the same — silently dropping unreachable entries would convert a
rate-limited run into a false all-clear, the exact failure `gx audit` refuses a missing token to
avoid. Second, the CI engineer filtering `--json` on `check` must be able to tell "an action you
depend on was tampered with" from "GitHub rate-limited us". Those warrant completely different
responses — page someone versus retry the job — and collapsing them into one name would force
consumers to parse human-readable prose to tell them apart.

A tag absent upstream is reported rather than skipped for the same reason a failed request is:
gx cannot distinguish a maintainer cleaning up tags from an attacker deleting evidence, and a
tag vanishing from under a pin is a change to the world the user pinned against either way.

#### Scenario: A failed lookup produces an error-level finding under its own name
- **GIVEN** a `gx.lock` entry resolved to a tag
- **AND** the live lookup for that tag fails
- **WHEN** the user runs `gx audit`
- **THEN** a `tag-unverified` finding is produced at error severity
- **AND** the message states the tag could not be verified, and why
- **AND** no `tag-moved` finding is produced for that entry
- **AND** the command exits with code 1

#### Scenario: A tag that no longer exists upstream is reported
- **GIVEN** a `gx.lock` entry resolved to a tag
- **AND** that tag is absent from the upstream repository
- **WHEN** the user runs `gx audit`
- **THEN** a `tag-unverified` finding is produced at error severity naming the action and tag

#### Scenario: One entry's failure does not suppress other entries' findings
- **GIVEN** a `gx.lock` with two tag-resolved entries
- **AND** the lookup for the first fails while the second's tag has moved
- **WHEN** the user runs `gx audit`
- **THEN** both entries produce findings
- **AND** the first is `tag-unverified` while the second is `tag-moved`

#### Scenario: A consumer can separate tampering from unreachability in JSON
- **GIVEN** a run producing one moved tag and one unreachable tag
- **WHEN** the user runs `gx audit --json`
- **THEN** the two findings carry different `check` values
- **AND** neither requires parsing `message` to tell them apart

---

### Requirement: Live tag resolution goes through a substitutable seam

The system SHALL resolve tags to commits through an abstraction with two implementations: one
issuing real GitHub API requests, and one returning canned results for tests. The `tag-moved`
check SHALL depend on the abstraction, never on an HTTP client directly.

The real implementation SHALL reuse gx's existing annotated-tag dereferencing rather than
carrying a second copy of it.

**User value:** indirect but load-bearing, and the same argument the advisory seam makes. The
logic deciding whether a user's action was compromised is the last place a bug should reach
production, and the only way to exercise it — moved tag, unmoved tag, annotated tag, failed
lookup — with confidence is offline and deterministically. A false positive here accuses a
maintainer of a supply-chain attack; the annotated-tag path in particular must be provably
exercised, because a bug there would fire on legitimate, widely-used actions and destroy trust
in the check.

#### Scenario: The check runs against canned tag data with no network
- **GIVEN** the test implementation of the tag seam, seeded with known tag targets
- **WHEN** the `tag-moved` check runs against it
- **THEN** the check produces its findings with no network request issued

#### Scenario: The real implementation dereferences annotated tags
- **GIVEN** the real implementation resolving a tag whose ref points at a tag object
- **WHEN** it resolves that tag
- **THEN** it returns the commit the tag object targets, not the tag object's own SHA
