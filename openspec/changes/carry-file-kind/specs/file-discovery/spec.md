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

#### Scenario: A composite step override remains valid without a job
- **GIVEN** `gx.toml` has an override
  `{ workflow = ".github/actions/setup/action.yml", step = 0, version = "^3" }`
- **WHEN** gx resolves the action at step 0 of that file
- **THEN** the override applies
- **AND** the absence of a `job` key is not an error
