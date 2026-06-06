## ADDED Requirements

### Requirement: Committed lockfiles are verified before merge

The project's CI SHALL verify that every tracked lockfile (`Cargo.lock`, `.config/mise.lock`, `.github/gx.lock`) is in sync with its source of truth on each pull request, so that lockfile drift fails a PR check rather than the release pipeline. Each lockfile SHALL be checked by its owning tool's native mechanism where that mechanism is safe to run in CI; where it is not, drift SHALL be detected by comparing the committed lockfile against the lockfile produced by the standard install.

**User value:** Maintainers and release consumers are protected from releases that silently break on lockfile drift; a maintainer who opens a PR learns immediately, with an actionable message, that a lockfile needs to be regenerated and committed.

#### Scenario: Cargo.lock is verified by cargo

- **GIVEN** a pull request whose CI runs cargo commands
- **WHEN** `cargo` is invoked with `--locked`
- **THEN** the job fails if running the command would modify `Cargo.lock`
- **AND** passes when `Cargo.lock` already matches the dependency graph

#### Scenario: gx.lock is verified by gx

- **GIVEN** a pull request that changes workflow files or action versions
- **WHEN** CI runs `gx tidy`
- **THEN** the job fails if `.github/gx.lock` or the manifest no longer matches the workflow code
- **AND** passes when manifest, lock, and workflows agree

#### Scenario: mise.lock drift is detected without triggering the rust catch-22

- **GIVEN** a pull request whose CI runs `mise install` (unlocked, as configured)
- **WHEN** the install completes and `.config/mise.lock` is compared against the committed version
- **THEN** the job fails if the lock was rewritten (drift)
- **AND** the failure message instructs the maintainer to run `mise install` and commit the regenerated `.config/mise.lock`
- **AND** the check does NOT use mise's `--locked` install mode (which would fail on the `core:rust` backend)

#### Scenario: A drifted lockfile blocks the PR, not the release

- **GIVEN** a tracked lockfile that has drifted from its source of truth
- **WHEN** CI runs on the pull request
- **THEN** the corresponding lockfile check fails before merge
- **AND** the release pipeline is never reached with an out-of-date lockfile
