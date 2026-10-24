## MODIFIED Requirements

### Requirement: mutable-ref check reports lock entries pinned to a branch

The `mutable-ref` check SHALL report each dependency in `.github/workflows/actions.lock` whose `ref` is a branch. A `ref` that parses as a version is a tag and is not reported; any other `ref` is classified with a lookup against GitHub.

#### Scenario: Branch-resolved entry produces a warning
- **GIVEN** a dependency `'octo/tool@<sha>'` with `ref: 'main'`, and `main` is a branch of `octo/tool`
- **WHEN** the user runs `gx audit`
- **THEN** a `mutable-ref` warning names `octo/tool` and `main`

#### Scenario: Version ref produces no finding
- **GIVEN** a dependency with `ref: 'v6.0.2'`
- **WHEN** the user runs `gx audit`
- **THEN** no `mutable-ref` finding is reported and no ref lookup is made

### Requirement: Audit the locked action set with gx audit

`gx audit` SHALL audit every dependency in `.github/workflows/actions.lock`, including transitive ones reached only through a composite action's `uses:`. An absent lockfile has nothing to audit.

#### Scenario: Transitive dependency is audited
- **GIVEN** a lockfile where `actions/setup-go@v5` appears only under another dependency's `uses:`
- **WHEN** the user runs `gx audit`
- **THEN** advisories for `actions/setup-go` at that commit are checked
