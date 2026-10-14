## ADDED Requirements

### Requirement: The lock is GitHub's Actions lockfile

gx SHALL store its lock at `.github/workflows/actions.lock` in the GitHub Actions dependency lockfile format, schema `v0.0.3`, as defined by [`github/actions-lockfile`](https://github.com/github/actions-lockfile/blob/main/schema/lockfile-v0.0.3.json). gx does not restate the schema; where this spec and upstream disagree, upstream wins.

**User value:** a repository owner gets one lockfile that GitHub's runner enforces and that every tool reading the official format understands, instead of a gx-only file next to it.

#### Scenario: Tidy writes the GitHub lockfile
- **GIVEN** a repository with `.github/workflows/ci.yml` using `actions/checkout@v6`
- **WHEN** the user runs `gx tidy`
- **THEN** `.github/workflows/actions.lock` exists with `version: 'v0.0.3'`
- **AND** the upstream parser accepts it
- **AND** no `.github/gx.lock` is written

#### Scenario: Output matches gh actions-lock byte for byte below the header
- **GIVEN** a lockfile written by `gh actions-lock` for a set of SHA-pinned workflows
- **WHEN** gx reads it and writes it back with no logical change
- **THEN** the output is byte-identical to the input

### Requirement: Workflows stay pinned to commit SHAs

`gx tidy` SHALL keep rewriting each lockable `uses:` to a full commit SHA with the version as a trailing comment. The lockfile pin for it SHALL be keyed `owner/repo@<sha>`, with `ref` set to that version and `commit` set to `sha1-<sha>`.

**User value:** repositories where GitHub does not enforce the lockfile keep the immutability they have today; repositories where it does get enforcement on top.

#### Scenario: SHA pin and lock entry agree
- **GIVEN** `actions/checkout@^6` resolving to `v6.0.2` at `de0fac2e4500dabe0009e67214ff5f5447ce83dd`
- **WHEN** the user runs `gx tidy`
- **THEN** the workflow reads `uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2`
- **AND** the lockfile has a dependency `'actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd'` with `ref: 'v6.0.2'` and `commit: 'sha1-de0fac2e4500dabe0009e67214ff5f5447ce83dd'`

#### Scenario: Subpath actions share their repository's pin
- **GIVEN** a workflow using `actions/cache/save` and `actions/cache/restore` at the same commit
- **WHEN** the user runs `gx tidy`
- **THEN** the lockfile has one dependency keyed `actions/cache@<sha>`

### Requirement: Only workflows GitHub can enforce are onboarded

A workflow SHALL appear in `workflows:` if and only if it contains no step that uses a `./` local action. Its list SHALL be the sorted, deduplicated, transitive closure of the pins it depends on. Skipped workflows SHALL produce one warning each.

**User value:** gx never causes the runner to reject a workflow that runs today.

#### Scenario: Workflow using a local action is left out
- **GIVEN** `.github/workflows/ci.yml` with a step `uses: ./.github/actions/setup`
- **WHEN** the user runs `gx tidy`
- **THEN** `ci.yml` is not a key under `workflows:`
- **AND** gx warns that GitHub cannot enforce the lockfile for `ci.yml` yet

#### Scenario: Local reusable workflow call does not block onboarding
- **GIVEN** a workflow whose only local reference is `jobs.build.uses: ./.github/workflows/build.yml`
- **WHEN** the user runs `gx tidy`
- **THEN** the workflow is listed under `workflows:`

#### Scenario: Docker and reusable-workflow references are not locked
- **GIVEN** a workflow using `docker://alpine:3.20` and `octo/shared/.github/workflows/ci.yml@v1`
- **WHEN** the user runs `gx tidy`
- **THEN** neither appears in the lockfile

### Requirement: Lock entries carry repository identity

Every dependency SHALL record the repository's numeric `owner_id` and `repo_id` as reported by GitHub.

**User value:** if an action's owner or repository is deleted and the name re-registered by someone else, the runner refuses the pin instead of running the impostor's code.

#### Scenario: IDs are fetched once per repository
- **GIVEN** two workflows that use `actions/checkout` at different commits
- **WHEN** the user runs `gx tidy`
- **THEN** both dependencies carry the same `owner_id` and `repo_id`
- **AND** the repository is looked up at most once

### Requirement: Remote composite actions are locked transitively

For each locked action whose definition is a composite action, gx SHALL resolve and lock every remote `uses:` inside it, at the ref that file names, and list those pins in the dependency's `uses:`. gx SHALL NOT modify the remote definition or apply manifest specifiers to its children.

**User value:** the code a composite action pulls in at runtime is pinned and verified too, not just the action the user named.

#### Scenario: Composite child is locked under its own ref
- **GIVEN** a workflow using `cli/gh-extension-precompile` whose `action.yml` uses `actions/setup-go@v5`
- **WHEN** the user runs `gx tidy`
- **THEN** the lockfile has a dependency `'actions/setup-go@v5'`
- **AND** the precompile dependency lists `'actions/setup-go@v5'` under `uses:`
- **AND** the workflow's list includes `'actions/setup-go@v5'`

#### Scenario: A cycle is an error
- **GIVEN** remote composite actions that use each other
- **WHEN** the user runs `gx tidy`
- **THEN** gx fails naming the pins in the cycle and writes no lockfile

### Requirement: gx.lock is migrated once, without re-resolution

When `.github/workflows/actions.lock` is absent and `.github/gx.lock` exists, the first command that writes the lock SHALL carry every `gx.lock` action entry over at its recorded commit and version, write `actions.lock`, delete `gx.lock`, and report the migration. When both exist, gx SHALL use `actions.lock`, leave `gx.lock` untouched, and warn that it is stale.

**User value:** upgrading gx never changes which commit a workflow runs.

#### Scenario: Migration preserves commits
- **GIVEN** a `gx.lock` locking `actions/checkout` `v6.0.2` at `de0fac2e…`
- **WHEN** the user runs `gx tidy`
- **THEN** `actions.lock` has `actions/checkout@de0fac2e…` with `ref: 'v6.0.2'`
- **AND** no version lookup is made for it
- **AND** `.github/gx.lock` no longer exists
- **AND** the output names both files

#### Scenario: Both files present
- **GIVEN** both `.github/gx.lock` and `.github/workflows/actions.lock`
- **WHEN** the user runs `gx tidy`
- **THEN** only `actions.lock` is read and written
- **AND** gx warns that `.github/gx.lock` is stale and can be deleted

### Requirement: Unreadable lockfiles fail loudly

A lockfile with a `version` newer than gx supports, or one that does not parse against the schema, SHALL be a hard error naming the file and position. gx SHALL NOT treat it as empty or overwrite it. Lockfiles at `v0.0.1` and `v0.0.2` SHALL be read and written back as `v0.0.3`.

**User value:** a newer tool's pins, or a hand-edit gone wrong, are never silently discarded.

#### Scenario: Newer version is refused
- **GIVEN** a lockfile with `version: 'v0.1.0'`
- **WHEN** the user runs `gx tidy`
- **THEN** gx fails saying the lockfile was written by a newer tool
- **AND** the file is unchanged

#### Scenario: Unknown key is refused
- **GIVEN** a lockfile dependency with an extra key `checksum`
- **WHEN** the user runs `gx tidy`
- **THEN** gx fails naming the line of `checksum`

## MODIFIED Requirements

### Requirement: Files have no format version field

The manifest `gx.toml` contains no format version field. The lockfile's `version` field is owned by the upstream schema; gx always writes the latest version it supports.

#### Scenario: Manifest has no gx section
- **GIVEN** a manifest written by any current gx version
- **THEN** the file contains `[actions]` and optionally `[actions.overrides]` and `[lint]`
- **AND** there is no `[gx]` section

### Requirement: Entries are sorted lexicographically

Workflow keys, each workflow's pin list, dependency keys and each dependency's `uses:` list are sorted lexicographically, as `gh actions-lock` sorts them.

#### Scenario: Dependencies are sorted by pin key
- **GIVEN** dependencies `actions/setup-node@<sha>` and `actions/checkout@<sha>`
- **WHEN** the lock is written
- **THEN** `actions/checkout@<sha>` appears first

### Requirement: A spec is complete only when both tiers are fully populated

A pin is complete when its dependency has `ref`, `commit`, `owner_id` and `repo_id`, and, if the action is composite, `uses`. A manifest specifier is satisfied when the pin used by each workflow has a `ref` that satisfies it. Incomplete pins are repaired with the narrowest lookup that fills the gap.

#### Scenario: Missing IDs trigger only an ID lookup
- **GIVEN** a dependency with `ref` and `commit` but no `owner_id`
- **WHEN** the user runs `gx tidy`
- **THEN** gx looks up the repository IDs
- **AND** does not re-resolve the version

## REMOVED Requirements

### Requirement: Lock file uses two-tier resolution and action sections

**Reason**: The GitHub schema has no resolution tier. The specifier → version mapping is derived from each workflow's pin `ref` (design Decision 4).

**Migration**: automatic on first write (see "gx.lock is migrated once, without re-resolution").

### Requirement: Forward-compatible reads

**Reason**: The GitHub schema is closed (`additionalProperties: false`). An unknown key may be a security field a newer producer relies on, so it is an error, not ignorable. Still applies to the manifest.

**Migration**: none; replaced by "Unreadable lockfiles fail loudly".

### Requirement: Migration happens transparently on write

**Reason**: Superseded by the `gx.lock` → `actions.lock` migration. The flat-format reader remains only as an input to that migration.

**Migration**: automatic.
