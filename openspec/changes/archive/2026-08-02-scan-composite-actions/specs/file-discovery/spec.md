## ADDED Requirements

### Requirement: gx discovers workflow files and composite action files

gx SHALL find managed `uses:` references in two kinds of file: workflow files
matching `.github/workflows/*.yml` and `.github/workflows/*.yaml`, and
composite action definitions matching `.github/actions/**/action.yml` and
`.github/actions/**/action.yaml`. Composite action discovery SHALL be
recursive, so a definition nested at any depth beneath `.github/actions` is
found.

Only a file named `action.yml` or `action.yaml` is a composite action
definition; other YAML files under `.github/actions` SHALL NOT be read as
action definitions.

**User value:** a maintainer who factors repeated setup steps out of their
workflows into a composite action keeps every supply-chain guarantee gx
provides. Before this, moving a `uses:` into `.github/actions/setup/action.yml`
made it invisible: `gx tidy` pruned it from `gx.toml` and `gx.lock`, `gx
upgrade` stopped advancing it, and `gx lint` stopped flagging it. What they
notice is that deduplicating their workflows no longer silently shrinks gx's
coverage.

#### Scenario: Action used only inside a composite action is discovered
- **GIVEN** a repository whose only `uses:` reference to `actions/setup-node` is
  in `.github/actions/setup/action.yml` under `runs.steps`
- **AND** `runs.using` is `composite`
- **WHEN** gx scans the repository
- **THEN** `actions/setup-node` is among the discovered actions
- **AND** its location is the file `.github/actions/setup/action.yml`

#### Scenario: Composite action nested below the top level is discovered
- **GIVEN** a composite action defined at `.github/actions/ci/setup/action.yml`
- **WHEN** gx scans the repository
- **THEN** its `uses:` references are discovered

#### Scenario: The .yaml extension is discovered
- **GIVEN** a composite action defined at `.github/actions/setup/action.yaml`
- **WHEN** gx scans the repository
- **THEN** its `uses:` references are discovered

#### Scenario: A non-action YAML file beside a definition is not read
- **GIVEN** `.github/actions/setup/action.yml` and
  `.github/actions/setup/config.yml`
- **WHEN** gx scans the repository
- **THEN** only `action.yml` is read as an action definition
- **AND** no error is reported for `config.yml`

### Requirement: Composite steps are located by file and step, without a job

A `uses:` reference found in a composite action's `runs.steps` SHALL be located
by its file path and its zero-based index within `runs.steps`, and SHALL NOT be
attributed to any job. gx SHALL NOT fabricate a job identifier for a schema
that has no jobs.

**User value:** a user narrowing a lint `ignore` entry or a `gx.toml` version
override to one composite step addresses it by the file and step they can see
in their editor. A synthetic job name would be a value they never wrote and
cannot find in their own file.

#### Scenario: Composite step diagnostic identifies file and line
- **GIVEN** `.github/actions/setup/action.yml` has an unpinned
  `uses: actions/checkout@v4` at line 12
- **WHEN** the user runs `gx lint`
- **THEN** the diagnostic identifies `.github/actions/setup/action.yml:12`

#### Scenario: Composite step override needs no job
- **GIVEN** `gx.toml` has an override
  `{ workflow = ".github/actions/setup/action.yml", step = 0, version = "^3" }`
- **WHEN** gx resolves the action at step 0 of that file
- **THEN** the override applies
- **AND** the absence of a `job` key is not an error

### Requirement: Only composite actions contribute managed references

gx SHALL read `runs.steps` only when `runs.using` is `composite`. An action
definition with any other `runs.using` value SHALL contribute no actions and
SHALL NOT produce an error or warning.

**User value:** a repository containing a JavaScript or Docker action gets no
spurious diagnostics about a file that legitimately has no `uses:` steps to
manage.

#### Scenario: A JavaScript action contributes nothing, silently
- **GIVEN** `.github/actions/tool/action.yml` with `runs.using: node20`
- **WHEN** gx scans the repository
- **THEN** no actions are discovered from that file
- **AND** no error or warning is reported

#### Scenario: An action definition with no `using` key contributes nothing
- **GIVEN** `.github/actions/tool/action.yml` that declares `runs.steps` but
  omits `runs.using`
- **WHEN** gx scans the repository
- **THEN** no actions are discovered from that file
- **AND** no error or warning is reported

### Requirement: Local and docker references are skipped in every file kind

gx SHALL skip a `uses:` value beginning with `.` (a local action reference such
as `./.github/actions/setup`) or with `docker://` wherever it appears,
including inside a composite action that references another composite action.
Such references SHALL NOT be added to `gx.toml` or `gx.lock` and SHALL NOT be
resolved against the registry.

**User value:** a user whose composite actions call one another sees only real
registry actions in their manifest, not paths into their own repository that
have no version to manage.

#### Scenario: Composite action referencing another composite action
- **GIVEN** `.github/actions/build/action.yml` has
  `uses: ./.github/actions/setup` and `uses: actions/checkout@v4`
- **WHEN** gx scans the repository
- **THEN** `actions/checkout` is discovered
- **AND** `./.github/actions/setup` is not discovered and is not added to
  `gx.toml`

### Requirement: An unreadable composite action file is a per-file error

A composite action file that cannot be read or cannot be parsed as YAML SHALL
produce an error naming that file, and SHALL NOT abort the scan of the
remaining files. This matches the existing treatment of a malformed workflow
file.

**User value:** a user with one broken `action.yml` still gets results for
every other file, and is told exactly which file gx could not process rather
than silently getting incomplete coverage.

#### Scenario: One malformed composite action does not hide the rest
- **GIVEN** `.github/actions/broken/action.yml` contains invalid YAML
- **AND** `.github/actions/setup/action.yml` is valid and uses
  `actions/checkout@v4`
- **WHEN** gx scans the repository
- **THEN** an error naming `.github/actions/broken/action.yml` is reported
- **AND** `actions/checkout` is still discovered from `setup/action.yml`

### Requirement: Every discovered file is a candidate for rewriting

Any file gx discovers a managed reference in SHALL also be a file gx writes
pins to. gx SHALL NOT know about a reference it will not rewrite.

**User value:** a user running `gx upgrade` sees every managed action advance,
wherever it lives. An action that gx lists in `gx.lock` but silently declines
to pin in its own file is the failure this change exists to remove.

#### Scenario: Upgrade rewrites a pin inside a composite action
- **GIVEN** `.github/actions/setup/action.yml` pins `actions/checkout` to the
  SHA for `v4.2.1`
- **AND** a newer `v4.3.0` satisfies the manifest specifier
- **WHEN** the user runs `gx upgrade`
- **THEN** `.github/actions/setup/action.yml` is rewritten with the `v4.3.0`
  SHA and comment

#### Scenario: Tidy pins an unpinned composite reference
- **GIVEN** `.github/actions/setup/action.yml` has `uses: actions/checkout@v4`
- **WHEN** the user runs `gx tidy`
- **THEN** the file is rewritten to the resolved SHA with a `# v4...` comment
- **AND** the count of files updated reported in the summary includes it

### Requirement: The summary counts files, not workflows

The `gx tidy` and `gx upgrade` summary counter SHALL count every file gx
rewrote, of either kind, and its human-readable label SHALL name files rather
than workflows. The corresponding `--json` field name SHALL be retained
unchanged, so existing automation that reads it does not break.

**User value:** a user who rewrote three workflows and one composite action
sees `4 files`, which matches what changed on disk. `3 workflows` would
under-report and leave them wondering whether the composite file was written.

#### Scenario: Mixed rewrite is counted and labelled as files
- **GIVEN** a repository where `gx tidy` rewrites two workflow files and one
  composite action file
- **WHEN** the command completes
- **THEN** the summary reports 3 files updated
- **AND** the `--json` output reports the same count under its existing field
  name

### Requirement: Discovery order is deterministic

gx SHALL enumerate discovered files in a stable order that does not depend on
filesystem enumeration order: workflow files first, then composite action
files, each group ordered by path.

**User value:** a user comparing `gx tidy --json` output across runs, machines,
or CI containers sees the same ordering, so a diff reflects real changes rather
than directory-read order.

#### Scenario: Repeated runs list changes in the same order
- **GIVEN** a repository with several workflow files and several composite
  action files, all requiring updates
- **WHEN** the user runs `gx tidy` twice on unchanged inputs
- **THEN** the files are reported in the same order both times
- **AND** workflow files are reported before composite action files
