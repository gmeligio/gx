## MODIFIED Requirements

### Requirement: Lockfiles are kept current by local git hooks

The project SHALL provide local git hooks (prek, installed and pinned through mise) that regenerate every tracked lockfile (`Cargo.lock`, `.config/mise.lock`, `.github/gx.lock`) when its inputs change, so a contributor's lockfile is current before the commit lands.

- `.config/mise.lock`'s inputs are `.config/mise.toml` and `rust-toolchain.toml`. The mise version that writes the lockfile SHALL NOT count as an input. The hook SHALL also run when `.config/mise.lock` itself changes, so a hand-edited or merge-conflicted lock is repaired.
- When a hook regenerates `.config/mise.lock`, it SHALL re-stage the regenerated file so the commit succeeds on retry without a manual `git add`.
- When a hook modifies `Cargo.lock` or `.github/gx.lock`, the commit SHALL be blocked so the contributor re-stages the regenerated file.

CI SHALL remain the enforcement backstop for commits made without the hooks installed. CI SHALL install tools in locked mode, set explicitly in the workflows rather than inherited from an action default, so it reads `.config/mise.lock` without ever rewriting it. CI SHALL accept a lockfile written by a mise release newer or older than its own, provided both use the same lockfile format. Local and CI are not required to run the same mise version.

**User value:**
- Contributors get immediate local feedback and an auto-regenerated lock instead of a failed PR discovered later.
- Contributors can upgrade mise whenever they like; a different local mise version never fails a pull request.
- `Cargo.lock` has a CI backstop (`cargo --locked`), and `.config/mise.lock` has one too (a locked tool install). A lockfile missing a tool or version it needs fails the pull request that surfaces it, instead of surfacing later as a release-pipeline abort.
- `.github/gx.lock` still has no CI check. Its writer is gx compiled from this repo's own source, so it cannot rewrite the lockfile independently of a contributor's commit.

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

#### Scenario: mise.lock is regenerated when tool inputs change

- **GIVEN** a contributor with the hooks installed
- **WHEN** they commit a change to `.config/mise.toml`, `rust-toolchain.toml`, or `.config/mise.lock`
- **THEN** the mise hook runs the unlocked mutating lock task (`mise run lock`) and regenerates `.config/mise.lock`
- **AND** the hook does NOT use `--locked` / `MISE_LOCKED`, because the hook exists to write the lockfile

#### Scenario: A mise upgrade alone does not rewrite mise.lock

- **GIVEN** a contributor whose mise binary has been upgraded
- **AND** the hooks are installed
- **WHEN** they commit changes that touch none of `.config/mise.toml`, `rust-toolchain.toml`, or `.config/mise.lock`
- **THEN** the mise hook does not run
- **AND** `.config/mise.lock` is not part of the commit

#### Scenario: A regenerated mise.lock is re-staged in one shot

- **GIVEN** a contributor with the hooks installed
- **AND** a commit that changes a mise tool input
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

#### Scenario: CI backstops a stale mise.lock that bypassed the hooks

- **GIVEN** a commit that adds a tool or changes a tool version in `.config/mise.toml` or `rust-toolchain.toml`, without regenerating `.config/mise.lock`
- **WHEN** CI installs the project's tools on the pull request
- **THEN** the locked install fails the job before merge

#### Scenario: CI never rewrites mise.lock

- **GIVEN** a `.config/mise.lock` written by a mise release newer or older than the one CI runs, using the same lockfile format
- **WHEN** any CI job installs the project's tools
- **THEN** CI installs the locked asset for its platform and verifies its checksum
- **AND** `.config/mise.lock` is unchanged in the working tree afterwards, so release pipelines never abort on a dirty tree caused by tool installation

#### Scenario: gx.lock drift has no CI backstop

- **GIVEN** a commit made without the local hooks (bypassed or un-bootstrapped)
- **AND** a drifted `.github/gx.lock` as a result
- **WHEN** CI runs on the pull request
- **THEN** no CI job flags the drift (there is no `gx tidy` verification in CI)
- **AND** the drift is caught only later, when the release pipeline aborts on a dirty working tree
- **AND** this is an accepted trade-off: the `gx tidy` writer is compiled from this repo's pinned source, so it cannot rewrite the lockfile independently of a contributor's commit
