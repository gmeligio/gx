## ADDED Requirements

### Requirement: CI uses a dedicated mise config without local-only tools
The CI environment SHALL use `.config/mise.ci.toml` which MUST NOT include `vhs` or `ttyd` tools. The config SHALL include `cargo-dist`, `cargo-semver-checks`, and `cargo-deny`.

#### Scenario: CI installs only CI-relevant tools
- **WHEN** `mise install` runs in CI with `MISE_CONFIG_FILE=.config/mise.ci.toml`
- **THEN** only cargo-dist, cargo-semver-checks, and cargo-deny are installed

#### Scenario: CI does not fail on macOS runners
- **WHEN** the release workflow runs the `build-local-artifacts` job on `macos-15`
- **THEN** `mise install` succeeds without attempting to install `ttyd`

### Requirement: CI workflows set MISE_CONFIG_FILE at workflow level
Both `build.yml` and `release.yml` SHALL set `MISE_CONFIG_FILE: .config/mise.ci.toml` as a top-level `env` variable, ensuring all mise operations in the workflow use the CI config.

#### Scenario: All mise steps in build.yml use CI config
- **WHEN** any job in `build.yml` runs a mise command
- **THEN** the `MISE_CONFIG_FILE` environment variable is set to `.config/mise.ci.toml`

#### Scenario: All mise steps in release.yml use CI config
- **WHEN** any job in `release.yml` runs a mise command
- **THEN** the `MISE_CONFIG_FILE` environment variable is set to `.config/mise.ci.toml`

### Requirement: Tasks are defined as file tasks shared by both configs
All tasks currently defined as `[tasks.*]` in `.config/mise.toml` SHALL be extracted to executable scripts in `.config/mise/tasks/`. Both local and CI configs SHALL auto-discover these tasks.

#### Scenario: Local development discovers file tasks
- **WHEN** a developer runs `mise run build` locally (using `.config/mise.toml`)
- **THEN** mise discovers and executes the `build` task from `.config/mise/tasks/build`

#### Scenario: CI discovers file tasks
- **WHEN** CI runs `mise run clippy` with `MISE_CONFIG_FILE=.config/mise.ci.toml`
- **THEN** mise discovers and executes the `clippy` task from `.config/mise/tasks/clippy`

### Requirement: Local mise config retains all tools
`.config/mise.toml` SHALL continue to include all tools (cargo-dist, cargo-semver-checks, cargo-deny, vhs, ttyd) for local development. Inline `[tasks.*]` entries SHALL be removed since tasks are now file-based.

#### Scenario: Local config includes local-only tools
- **WHEN** a developer runs `mise install` locally
- **THEN** all tools including vhs and ttyd are installed
