## ADDED Requirements

### Requirement: Workflow-schema rules do not run on composite action files

The system SHALL evaluate the rules that read workflow-schema structure —
`missing-permissions`, `excessive-permissions`, `dangerous-trigger`,
`missing-concurrency`, `pr-head-checkout`, `unprotected-secrets`,
`dangling-reference`, `invalid-expression`, and `run-shellcheck` — against
workflow files only. A composite action definition has no `on:` block, no
top-level `permissions:` block, and no jobs; these rules SHALL NOT produce
diagnostics for such a file.

`run-shellcheck` is included because it reaches `run:` bodies through
workflow-schema structure. Extending it to a composite action's `runs.steps` is
deferred rather than impossible: that schema has no `defaults:` block, so the
shell precedence chain the rule depends on needs its own decision first.

**User value:** a maintainer who adopts composite actions gets the new pinning
coverage without a wall of meaningless diagnostics telling them their
`action.yml` is missing a `permissions:` block it is not allowed to have.

#### Scenario: Composite action is not flagged for a missing permissions block
- **GIVEN** `.github/actions/setup/action.yml` with `runs.using: composite` and
  no `permissions:` block
- **WHEN** the user runs `gx lint` with `missing-permissions` enabled
- **THEN** no diagnostic is produced for that file
- **AND** workflow files missing a `permissions:` block are still flagged

#### Scenario: Composite action is not flagged by trigger or job-graph rules
- **GIVEN** `.github/actions/setup/action.yml` with `runs.using: composite`
- **WHEN** the user runs `gx lint`
- **THEN** `dangerous-trigger`, `missing-concurrency`, `unprotected-secrets`,
  `pr-head-checkout`, and `dangling-reference` produce no diagnostics for that
  file

#### Scenario: run-shellcheck does not analyze composite run bodies
- **GIVEN** `.github/actions/setup/action.yml` with `runs.using: composite` and
  a `run:` step containing a shell issue shellcheck would report
- **WHEN** the user runs `gx lint` with `run-shellcheck` enabled
- **THEN** no diagnostic is produced for that file
- **AND** equivalent `run:` bodies in workflow files are still analyzed

### Requirement: Widened coverage may fail a previously passing repository

gx SHALL NOT suppress diagnostics from newly covered composite action files,
and SHALL NOT gate the widened coverage behind a flag. A repository that passes
`gx lint` today MAY therefore begin reporting diagnostics — typically
`unpinned` — and begin exiting non-zero. A user who wants the previous behavior
SHALL narrow it explicitly with an `ignore` entry naming the file.

**User value:** the maintainer whose composite actions were never pinned is
told so. Defaulting to silence to protect a green build would preserve exactly
the blind spot this change exists to close, and the user could not tell the
difference between "covered and clean" and "not covered".

#### Scenario: Previously green repository reports newly covered violations
- **GIVEN** a repository that exits 0 on `gx lint` before this change
- **AND** `.github/actions/setup/action.yml` references `actions/checkout@v4`
  unpinned
- **WHEN** the user runs `gx lint` after upgrading gx
- **THEN** an `unpinned` diagnostic is produced for that file
- **AND** the command exits non-zero per the existing error exit-code contract

#### Scenario: User opts out explicitly
- **GIVEN** the same repository
- **AND** `unpinned` has `ignore = [{ workflow = ".github/actions/setup/action.yml" }]`
- **WHEN** the user runs `gx lint`
- **THEN** no diagnostic is produced for that file

## MODIFIED Requirements

### Requirement: unpinned rule

The system SHALL detect when a workflow file or a composite action file references an action using a tag (e.g., `@v4`) instead of a SHA-pinned reference (e.g., `@abc123 # v4`).

#### Scenario: Action is SHA-pinned
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4`
- **WHEN** `unpinned` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Action uses tag reference
- **GIVEN** `ci.yml` has `actions/checkout@v4`
- **WHEN** `unpinned` rule runs
- **THEN** an error diagnostic is produced identifying the file and action

#### Scenario: Unpinned action inside a composite action
- **GIVEN** `.github/actions/setup/action.yml` has `actions/checkout@v4` under `runs.steps`
- **WHEN** `unpinned` rule runs
- **THEN** an error diagnostic is produced identifying `.github/actions/setup/action.yml` and the action

### Requirement: unsynced-manifest rule

The system SHALL detect when the set of actions referenced across all scanned files — workflow files and composite action files — does not match the set of actions in the manifest.

#### Scenario: Action in workflow but not in manifest
- **GIVEN** `ci.yml` uses `actions/cache` but `gx.toml` does not list `actions/cache`
- **WHEN** `unsynced-manifest` rule runs
- **THEN** an error diagnostic is produced: action found in workflow but missing from manifest

#### Scenario: Action in manifest but used only in a composite action
- **GIVEN** `gx.toml` lists `actions/setup-node` and the only reference to it is in `.github/actions/setup/action.yml`
- **WHEN** `unsynced-manifest` rule runs
- **THEN** no diagnostic is produced, because the action is referenced

#### Scenario: Action in manifest but not referenced anywhere
- **GIVEN** `gx.toml` lists `actions/setup-go` but no workflow file and no composite action file uses it
- **WHEN** `unsynced-manifest` rule runs
- **THEN** an error diagnostic is produced: action in manifest but unused

#### Scenario: Manifest and scanned files are in sync
- **GIVEN** every action in `gx.toml` appears in at least one scanned file and vice versa
- **WHEN** `unsynced-manifest` rule runs
- **THEN** no diagnostic is produced

### Requirement: Ignore targets for rules

The system SHALL support ignore entries in rule configuration using typed keys: `action`, `workflow`, and `job`. Multiple keys in a single entry compose as intersection (narrowing scope). The `workflow` key holds a file path and SHALL match either a workflow file or a composite action file.

#### Scenario: Ignore a specific action
- **GIVEN** `unpinned` rule has `ignore = [{ action = "actions/internal-tool" }]`
- **WHEN** user runs `gx lint` and `actions/internal-tool` is unpinned in a workflow
- **THEN** no diagnostic is produced for `actions/internal-tool`
- **AND** other unpinned actions still produce diagnostics

#### Scenario: Ignore scoped to workflow and job
- **GIVEN** `sha-mismatch` rule has `ignore = [{ action = "actions/checkout", workflow = ".github/workflows/legacy.yml", job = "compat" }]`
- **WHEN** `actions/checkout` has a SHA mismatch in `legacy.yml` job `compat`
- **THEN** no diagnostic is produced for that specific location
- **WHEN** `actions/checkout` has a SHA mismatch in `legacy.yml` job `build`
- **THEN** a diagnostic IS produced (different job, not covered by ignore)

#### Scenario: Ignore scoped to workflow only
- **GIVEN** `unpinned` rule has `ignore = [{ workflow = ".github/workflows/experimental.yml" }]`
- **WHEN** any action is unpinned in `experimental.yml`
- **THEN** no diagnostic is produced for actions in that workflow

#### Scenario: Ignore scoped to a composite action file
- **GIVEN** `unpinned` rule has `ignore = [{ workflow = ".github/actions/setup/action.yml" }]`
- **WHEN** any action is unpinned in that composite action file
- **THEN** no diagnostic is produced for actions in that file
- **AND** unpinned actions in workflow files still produce diagnostics
