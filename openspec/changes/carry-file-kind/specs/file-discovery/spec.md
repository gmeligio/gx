## ADDED Requirements

### Requirement: A managed file's kind is established at discovery, not inferred from its path

gx SHALL decide which schema a managed file follows at the moment the file is discovered,
and SHALL carry that decision with the file thereafter. No later stage SHALL re-derive a
file's kind from its path.

A file's kind SHALL therefore be correct regardless of which directory the file lives in.
Where a file is found determines its kind; where it happens to sit in the tree does not.

**User value:** a maintainer keeps a composite action somewhere other than
`.github/actions` — reached by a local `uses:` edge, or brought into scope by configuration
— and gx reads it as the composite action it is. Before this, kind was recomputed by
checking whether some ancestor directory was named `actions` under `.github`, so any action
definition outside that directory was silently treated as a workflow: gx parsed it under
the wrong schema, found none of its `uses:` references, and reported nothing. What the user
notices is that gx's coverage no longer depends on where they chose to put the file.

This requirement is what makes reaching those files safe; reaching them is separate work.

#### Scenario: An action definition outside `.github/actions` is read as an action
- **GIVEN** a file gx has discovered as an action definition
- **AND** the file's path is not under `.github/actions`
- **WHEN** gx parses it
- **THEN** its `runs.steps` references are discovered
- **AND** it is not evaluated as a workflow

#### Scenario: A workflow named action.yml is still a workflow
- **GIVEN** `.github/workflows/action.yml`
- **WHEN** gx scans the repository
- **THEN** the file is read under the workflow schema
- **AND** its `jobs.<id>.steps` references are discovered

#### Scenario: Kind does not change between discovery and use
- **GIVEN** any file gx discovers
- **WHEN** gx parses, lints, and rewrites that file in one run
- **THEN** every stage treats it as the same kind discovery assigned

---

### Requirement: An override or ignore entry naming an unscanned file is reported

gx SHALL report an error when a `gx.toml` override or a lint `ignore` entry names a file
path that gx does not scan. The error SHALL name the offending path and state that gx does
not scan it.

**User value:** a user who renames a directory, deletes a composite action, or mistypes a
path in `gx.toml` is told. Before this, an entry naming an unscanned file was accepted at
validation and then matched nothing at runtime — the user saw a config they had written,
believed it was in effect, and got silence. The specific trap was a path that *looked* like
an action definition but was never scanned, such as a non-`action.yml` file under
`.github/actions`: validation inspected the path's shape rather than the set of files gx
actually reads. What the user notices is that a stale entry fails loudly instead of doing
nothing.

#### Scenario: Override naming a file gx does not scan is rejected
- **GIVEN** `gx.toml` has an override naming `.github/actions/setup/steps.yml`
- **AND** gx does not scan that file, because only `action.yml` and `action.yaml` are
  action definitions
- **WHEN** the user runs a command that reads the manifest
- **THEN** an error naming `.github/actions/setup/steps.yml` is reported
- **AND** the error states that gx does not scan that file

#### Scenario: Override naming a scanned file is accepted
- **GIVEN** `gx.toml` has an override naming `.github/actions/setup/action.yml`
- **AND** gx scans that file
- **WHEN** the user runs a command that reads the manifest
- **THEN** the override is accepted and applies

#### Scenario: A composite step override remains valid without a job
- **GIVEN** `gx.toml` has an override
  `{ workflow = ".github/actions/setup/action.yml", step = 0, version = "^3" }`
- **WHEN** gx resolves the action at step 0 of that file
- **THEN** the override applies
- **AND** the absence of a `job` key is not an error
