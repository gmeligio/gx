## Lockfile Integrity

Contributors keep every tracked lockfile current automatically through local git hooks, with CI as the backstop.

---

### Requirement: Lockfiles are kept current by local git hooks

The project SHALL provide local git hooks (via prek, installed and pinned through mise) that regenerate every tracked lockfile (`Cargo.lock`, `.config/mise.lock`, `.github/gx.lock`) when its inputs change, so a contributor's lockfile is current before the commit lands. When a hook modifies a lockfile, the commit SHALL be blocked so the contributor re-stages the regenerated file. CI SHALL remain the enforcement backstop for commits made without the hooks installed.

**User value:** Maintainers and release consumers are protected from releases that silently break on lockfile drift; contributors get immediate local feedback and an auto-regenerated lock instead of a failed PR discovered later.

#### Scenario: Cargo.lock is regenerated when Cargo.toml changes

- **GIVEN** a contributor with the hooks installed
- **WHEN** they commit a change to `Cargo.toml`
- **THEN** the cargo lock-sync hook regenerates `Cargo.lock` if needed
- **AND** the commit is blocked so the contributor re-stages the updated `Cargo.lock`

#### Scenario: gx.lock is regenerated when workflows change

- **GIVEN** a contributor with the hooks installed
- **WHEN** they commit a change to a workflow file or `.github/gx.toml`
- **THEN** the `gx tidy` hook updates `.github/gx.lock` / `.github/gx.toml` to match
- **AND** the commit is blocked if anything changed, prompting a re-stage

#### Scenario: mise.lock drift is caught even without a config edit

- **GIVEN** a contributor whose mise binary has been upgraded
- **AND** the hooks are installed
- **WHEN** they make any commit
- **THEN** the mise hook runs the unlocked `mise install` and regenerates `.config/mise.lock` if the new binary rewrote it
- **AND** the hook does NOT use `--locked` / `MISE_LOCKED` (which would fail on the `core:rust` backend)

#### Scenario: Hooks are installed automatically per worktree

- **GIVEN** a fresh checkout or git worktree
- **WHEN** a session starts
- **THEN** the bootstrap runs `mise run setup`, which runs `prek install` for that worktree's hooks path
- **AND** the bootstrap is a no-op when the hook is already installed

#### Scenario: CI backstops a lock that bypassed the hooks

- **GIVEN** a commit made without the local hooks (bypassed or un-bootstrapped)
- **AND** a tracked lockfile that drifted as a result
- **WHEN** CI runs on the pull request
- **THEN** the corresponding `--locked` cargo check fails the PR before merge
