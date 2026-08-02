## ADDED Requirements

### Requirement: An action referenced only by a composite action is retained

When deciding whether an action entry is still referenced, gx SHALL count
references from composite action files as well as from workflow files. `gx
tidy` SHALL NOT remove from `gx.toml` or `gx.lock` an action whose only
remaining reference is inside a composite action definition.

**User value:** a maintainer who moves a shared `uses:` step out of two
workflows and into `.github/actions/setup/action.yml` runs `gx tidy` and finds
the action still managed. Before this, tidy reported it as removed, the pin
stayed in the file and kept working, and nothing warned — so the action
silently stopped being upgraded, linted, and audited.

#### Scenario: Action moved into a composite action survives tidy
- **GIVEN** `gx.toml` and `gx.lock` contain `actions/setup-node`
- **AND** the only `uses: actions/setup-node` reference in the repository is in
  `.github/actions/setup/action.yml` under `runs.steps`
- **WHEN** the user runs `gx tidy`
- **THEN** `actions/setup-node` remains in `gx.toml` and `gx.lock`
- **AND** it is not reported as removed

#### Scenario: Action referenced nowhere is still removed
- **GIVEN** `gx.toml` contains `actions/setup-go`
- **AND** no workflow file and no composite action file references it
- **WHEN** the user runs `gx tidy`
- **THEN** `actions/setup-go` is removed from `gx.toml` and `gx.lock`

#### Scenario: Init derives the manifest from composite references too
- **GIVEN** a repository with no `gx.toml`
- **AND** `.github/actions/setup/action.yml` uses `actions/checkout@v4`
- **WHEN** the user runs `gx init`
- **THEN** `actions/checkout` appears in the generated `gx.toml` and `gx.lock`

## MODIFIED Requirements

### Requirement: Workflow comments show the resolved version

When gx writes a pinned action reference to a workflow file or a composite action file, the inline YAML comment SHALL show the resolved version from the lock, not a specifier-derived string.

#### Scenario: Version annotation uses resolved version
- **GIVEN** a manifest specifier `^4` resolved to version `v4.2.1` with SHA `abc123...`
- **WHEN** gx writes the workflow file
- **THEN** the output is `uses: actions/checkout@abc123... # v4.2.1`

#### Scenario: Bare SHA specifier has no annotation
- **GIVEN** a manifest specifier that is a bare SHA
- **WHEN** gx writes the workflow file
- **THEN** the output is `uses: actions/checkout@abc123...` with no inline comment

#### Scenario: Composite action file is annotated identically
- **GIVEN** a manifest specifier `^4` resolved to version `v4.2.1` with SHA `abc123...`
- **AND** the reference lives in `.github/actions/setup/action.yml` under `runs.steps`
- **WHEN** gx writes that file
- **THEN** the output is `uses: actions/checkout@abc123... # v4.2.1`
