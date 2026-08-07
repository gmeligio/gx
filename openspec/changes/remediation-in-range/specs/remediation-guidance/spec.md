## ADDED Requirements

### Requirement: Suggest `gx upgrade` only when the fix is provably reachable

A user responding to a security advisory MUST be able to trust that any command
gx hands them actually resolves the vulnerability. `gx upgrade` moves an action
only within its manifest specifier, so gx SHALL suggest `gx upgrade <action>` as
remediation only when the advisory names a patched version **and** the manifest
specifier admits that version. When either condition fails, gx SHALL NOT emit a
command; it SHALL instead say what stands in the way.

The user who benefits is someone triaging a vulnerable action under time
pressure. What they notice: they never run a suggested command that leaves them
still vulnerable, and when no command is offered they are told which of the two
reasons applies, so they know whether to look for a migration or to widen the
specifier.

Classification SHALL be tolerant of the `v` prefix in either direction: advisory
identifiers commonly omit it (`3.0.0`) while gx tags carry it (`v3.0.0`), and
the verdict MUST NOT depend on which form the advisory used.

The decision rests on the manifest specifier and the advisory's patched version
alone. Whether the currently locked version is itself already patched is a
separate question — determining that an action is vulnerable at all is the
caller's job, and this decision assumes it has been answered. Scenarios below
name a locked version only to identify the real-world case; it is not an input.

#### Scenario: Patched version is inside the specifier's range

- **WHEN** the manifest pins `tj-actions/changed-files` at `^46`, the locked
  version is `v46.0.0`, and the advisory's first patched version is `46.0.1`
- **THEN** gx classifies the remediation as reachable by upgrade
- **AND** the user is offered `gx upgrade tj-actions/changed-files`

#### Scenario: Patched version is a later patch line within the same minor

- **WHEN** the manifest pins `shivammathur/setup-php` at `~2.37`, the locked
  version is `v2.36.0`, and the advisory's first patched version is `2.37.1`
- **THEN** gx classifies the remediation as reachable by upgrade
- **AND** the user is offered `gx upgrade shivammathur/setup-php`

#### Scenario: Advisory names no patched version

- **WHEN** an advisory for `reviewdog/action-setup` covers `= 1` and carries no
  first patched version
- **THEN** gx classifies the remediation as unavailable
- **AND** no command is suggested
- **AND** the user is told that no fixed version exists and migration is required

#### Scenario: Patched version requires a major bump

- **WHEN** the manifest pins `github/codeql-action` at `^2`, the locked version
  is `v2.26.11`, and the advisory's first patched version is `3.0.0`
- **THEN** gx classifies the remediation as out of range
- **AND** no command is suggested
- **AND** the user is told the fix is outside their specifier and requires a
  major bump

#### Scenario: Patched version is out of range under a `0.x` caret

- **WHEN** the manifest pins `aquasecurity/trivy-action` at `^0.34`, the locked
  version is `v0.30.0`, and the advisory's first patched version is `0.35.0`
- **THEN** gx classifies the remediation as out of range, because a caret on a
  `0.x` version is locked to that minor
- **AND** no command is suggested

#### Scenario: Advisory version carries a `v` prefix

- **WHEN** the manifest pins an action at `^46` and the advisory's first patched
  version is given as `v46.0.1` rather than `46.0.1`
- **THEN** gx reaches the same verdict as for the unprefixed form

### Requirement: An unusable patched version is reported as no fix, not as out of range

gx SHALL treat a patched version it cannot deliver — one it cannot interpret as
a version, or a prerelease, which no ordinary range admits — as though no
patched version were named. gx MUST NOT report such an advisory as having a fix
outside the user's range, because that would tell the user a wider specifier
would reach a fix when no reachable fix is known.

#### Scenario: Advisory patched version is not a recognizable version

- **WHEN** the manifest pins an action at `^2` and the advisory's first patched
  version cannot be interpreted as a version
- **THEN** no command is suggested
- **AND** the user is told that no fixed version is available and migration is
  required
- **AND** the user is NOT told that a fix exists outside their range

#### Scenario: Advisory patched version is a prerelease

- **WHEN** the manifest pins an action at `^2` and the advisory's first patched
  version is `2.1.0-beta.1`
- **THEN** no command is suggested
- **AND** the user is told that no fixed version is available and migration is
  required
- **AND** the user is NOT told that a major bump is required, because widening
  the specifier would not reach the prerelease either

### Requirement: The reported fixed version is canonical

Whatever form an advisory uses for its patched version, gx SHALL report it as a
concrete, lowercase-`v`-prefixed version. A user comparing the reported fix
against their manifest and lock — where every version gx prints is `v`-prefixed
— SHALL NOT have to reconcile a differently-shaped string, and SHALL NOT be
shown a range-shaped identifier in place of a version.

#### Scenario: Advisory uses an uppercase `V` prefix

- **WHEN** an advisory's first patched version is `V46.0.1`
- **THEN** the reported fixed version is `v46.0.1`

#### Scenario: Advisory names an imprecise version

- **WHEN** an advisory's first patched version is `2.37`
- **THEN** the reported fixed version is `v2.37.0`, not `v2.37`

### Requirement: Never suggest an upgrade for a reference an upgrade cannot move

gx SHALL NOT suggest `gx upgrade <action>` for a manifest entry that names a
branch or a bare commit SHA, even when the advisory names a patched version:
such an entry is not governed by a semver range, so `gx upgrade` cannot be
relied on to move it to a patched version.

Because the obstacle is the absence of a range rather than its width, gx MUST
NOT tell such a user that a major bump is what stands in the way. What must be
conveyed is that the entry has to become a semver range before `gx upgrade` can
move it.

#### Scenario: Manifest tracks a branch

- **WHEN** the manifest tracks an action at `main` and the advisory names a
  first patched version
- **THEN** no upgrade command is suggested
- **AND** the user is NOT told that a major bump is required

#### Scenario: Manifest pins a bare commit SHA

- **WHEN** the manifest pins an action to a 40-character commit SHA and the
  advisory names a first patched version
- **THEN** no upgrade command is suggested
- **AND** the user is NOT told that a major bump is required
