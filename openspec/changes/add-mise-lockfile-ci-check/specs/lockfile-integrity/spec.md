## MODIFIED Requirements

### Requirement: Lockfiles are kept current by local git hooks

The project SHALL provide local git hooks (via prek, installed and pinned through mise) that regenerate every tracked lockfile (`Cargo.lock`, `.config/mise.lock`, `.github/gx.lock`) when its inputs change, so a contributor's lockfile is current before the commit lands. When a hook regenerates `.config/mise.lock`, it SHALL re-stage the regenerated file so the commit succeeds on retry without a manual `git add`. When a hook modifies `Cargo.lock` or `.github/gx.lock`, the commit SHALL be blocked so the contributor re-stages the regenerated file. CI SHALL remain the enforcement backstop for commits made without the hooks installed, and SHALL verify `.config/mise.lock` directly rather than deferring to the release pipeline.

**User value:** Contributors get immediate local feedback and an auto-regenerated lock instead of a failed PR discovered later. `Cargo.lock` has a CI backstop (`cargo --locked`) and `.config/mise.lock` has one too (`mise run lock:check`), so drift in either fails the pull request that surfaces it, naming the offending field, instead of surfacing much later as a release-pipeline abort whose error message points at the wrong cause. The mise check inherits CI's tool-cache state, so it detects drift on the same cold-cache runs that produce it rather than guaranteeing detection on every run. `.github/gx.lock` still has no CI check; its writer is gx compiled from this repo's own source, so it cannot rewrite the lockfile independently of a contributor's commit.

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
- **THEN** the mise hook runs the unlocked mutating lock task (`mise run lock`) and regenerates `.config/mise.lock` if the new binary rewrote it
- **AND** the hook does NOT use `--locked` / `MISE_LOCKED` (which would fail on the `core:rust` backend)

#### Scenario: A regenerated mise.lock is re-staged in one shot

- **GIVEN** a contributor with the hooks installed
- **AND** a `.config/mise.lock` that their mise binary rewrites
- **WHEN** they commit
- **THEN** the hook regenerates the lockfile and re-stages it with `git add`
- **AND** prek reports the hook as having modified files, so the regeneration is visible rather than silent
- **AND** re-running the commit succeeds without a manual `git add`

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

#### Scenario: CI backstops a drifted mise.lock, whatever its origin

- **GIVEN** a `.config/mise.lock` that does not match what `mise install` produces under CI's install conditions
- **AND** the drift originated either from a hook-bypassing commit or from CI itself, where a cold tool cache makes mise reinstall and rewrite entries that a warm cache leaves untouched
- **WHEN** CI runs on the pull request
- **THEN** the `Lockfile` job fails the PR before merge
- **AND** the failure output includes the diff, naming the field that changed

#### Scenario: gx.lock drift has no CI backstop

- **GIVEN** a commit made without the local hooks (bypassed or un-bootstrapped)
- **AND** a drifted `.github/gx.lock` as a result
- **WHEN** CI runs on the pull request
- **THEN** no CI job flags the drift (there is no `gx tidy` verification in CI)
- **AND** the drift is caught only later, when the release pipeline aborts on a dirty working tree
- **AND** this is an accepted trade-off: unlike mise, the `gx tidy` writer is compiled from this repo's pinned source, so it cannot rewrite the lockfile independently of a contributor's commit
