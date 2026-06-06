## Lockfile Integrity

Contributors keep every tracked lockfile current automatically through local git hooks, with CI as the backstop.

---

### Requirement: Lockfiles are kept current by local git hooks

The project SHALL provide local git hooks (via prek, installed and pinned through mise) that regenerate every tracked lockfile (`Cargo.lock`, `.config/mise.lock`, `.github/gx.lock`) when its inputs change, so a contributor's lockfile is current before the commit lands. When a hook modifies a lockfile, the commit SHALL be blocked so the contributor re-stages the regenerated file. CI SHALL remain the enforcement backstop for commits made without the hooks installed.

**User value:** Contributors get immediate local feedback and an auto-regenerated lock instead of a failed PR discovered later. `Cargo.lock` additionally has a CI backstop (`cargo --locked`); `.config/mise.lock` and `.github/gx.lock` have no CI check, so for those two the local hooks are the only line of defense before the release pipeline (which aborts on a dirty tree) catches the drift.

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

#### Scenario: CI backstops a drifted Cargo.lock that bypassed the hooks

- **GIVEN** a commit made without the local hooks (bypassed or un-bootstrapped)
- **AND** a drifted `Cargo.lock` as a result
- **WHEN** CI runs on the pull request
- **THEN** the `cargo --locked` check fails the PR before merge

#### Scenario: mise.lock / gx.lock drift has no CI backstop

- **GIVEN** a commit made without the local hooks (bypassed or un-bootstrapped)
- **AND** a drifted `.config/mise.lock` or `.github/gx.lock` as a result
- **WHEN** CI runs on the pull request
- **THEN** no CI job flags the drift (there is no `gx tidy` / unlocked `mise install` verification in CI)
- **AND** the drift is caught only later, when the release pipeline aborts on a dirty working tree
- **AND** this is an accepted trade-off for a solo, always-bootstrapped repo (see design Decision 5); a CI verification job was deliberately not re-added
