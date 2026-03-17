## User Value

Users can install and update gx via Homebrew with correct, verified binaries.

---

## Installation & Updates

### Requirement: Formula is available on Homebrew after release

#### Scenario: User installs gx for the first time
- **GIVEN** a new gx version has been released
- **WHEN** a user runs `brew install <tap>/gx`
- **THEN** the formula resolves to the latest released version with correct binary URLs

#### Scenario: User upgrades gx after a new release
- **GIVEN** the user has a previous version of gx installed via Homebrew
- **AND** a new gx version has been released and the formula is updated
- **WHEN** the user runs `brew upgrade gx`
- **THEN** Homebrew fetches the new version with matching checksums

---

## Formula Integrity

### Requirement: Formula contains SHA256 checksums baked in at build time

The formula SHALL embed SHA256 checksums produced by cargo-dist during the build. The workflow SHALL NOT depend on `.sha256` sidecar files.

#### Scenario: Checksum verification passes on install
- **GIVEN** a released formula with embedded SHA256 checksums
- **WHEN** Homebrew downloads the binary archive
- **THEN** the SHA256 of the downloaded archive matches the checksum in the formula

---

## Bot Identity & Trust

### Requirement: Formula commits are authored by a verified bot identity

Formula updates SHALL be committed under a GitHub App bot identity, so that commits appear as verified and traceable to the release automation rather than a personal account.

#### Scenario: Formula commit is attributed to the bot
- **GIVEN** a release triggers the Homebrew publish workflow
- **WHEN** the formula update commit is created
- **THEN** the commit author is the GitHub App bot (not a personal user)
- **AND** the commit appears as verified in the tap repository

---

## Single Atomic Update

### Requirement: All formula changes are delivered in a single pull request

All formula file changes for a release SHALL be collected into a single commit and a single pull request, so that the tap repository history stays clean and reviewable.

#### Scenario: Multi-formula release produces one PR
- **GIVEN** a release updates more than one formula file
- **WHEN** the workflow completes
- **THEN** exactly one pull request is opened containing all formula changes
- **AND** the pull request is automatically merged
- **AND** the temporary branch is deleted after merge

---

## Guardrail: Commit Delivery via GraphQL API

### Requirement: No direct git push to the tap repository

The workflow SHALL use the GitHub GraphQL `createCommitOnBranch` mutation to deliver commits. Direct `git push` with token-injected remotes is prohibited.

**Rationale:** GraphQL-based commit delivery ensures atomic, auditable formula updates. Commits are automatically signed by GitHub, providing the verified status users see in the tap history. A failed mutation leaves no partial state, unlike a failed push.
