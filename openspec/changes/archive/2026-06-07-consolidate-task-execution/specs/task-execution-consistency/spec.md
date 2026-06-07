## ADDED Requirements

### Requirement: Developer checks have one definition invoked by every environment

Every project check (format, clippy, supply-chain/deny, lockfile tidy, compile check) SHALL be defined exactly once as a mise task. The local shell, the pre-commit hooks, and CI SHALL all invoke that check through `mise run <task>` rather than re-spelling the underlying `cargo`/`clippy`/`rustfmt` command. No `build.yml` step and no pre-commit hook SHALL invoke `cargo`, `clippy`, or `rustfmt` directly for a check that has a mise task.

**User value:** Contributors never get a CI failure caused by a check that passed locally but used different flags than CI — the drift class that caused the PR #102 `cargo-deny unmatched-skip-root` failure (the `-D warnings` flag lived only in the mise task). Changing a check's flags in one place moves every environment at once. This is the same contributor-facing guarantee as `lockfile-integrity`, applied to check definitions.

#### Scenario: A check's flags live in exactly one place

- **GIVEN** any check that runs in both CI and locally (format, clippy, deny, check)
- **WHEN** its command or flags need to change
- **THEN** the change is made once in the check's mise task
- **AND** CI and the pre-commit hook both pick up the change because they call `mise run <task>`, not a copied command

#### Scenario: CI contains no inline check commands

- **GIVEN** the CI workflow `build.yml`
- **WHEN** a check job runs
- **THEN** its `run:` step is `mise run <task>` (e.g. `mise run format:check`, `mise run clippy:check`, `mise run check`, `mise run deny`)
- **AND** no check job invokes `cargo fmt`, `cargo clippy`, or `rustfmt` directly

### Requirement: Local and pre-commit mutate; CI verifies

Checks that can fix code (format, clippy) SHALL mutate in the local and pre-commit environments and SHALL be verified non-mutating in CI. The mutating task and its `:check` verification variant SHALL share the same configuration (same `rustfmt.toml`, same `--all` scope, same lint set) so that only the verb differs. CI SHALL NOT modify the working tree.

**User value:** A contributor's commit is auto-formatted in one shot instead of being told it is misformatted; CI gates the immutable pushed commit and never silently rewrites contributor code. Because the mutate and verify variants share one configuration, they cannot disagree on what "formatted" means.

#### Scenario: Pre-commit fixes and re-stages in one shot

- **GIVEN** a contributor with the hooks installed
- **AND** a staged change containing misformatted Rust
- **WHEN** they commit
- **THEN** the `cargo-fmt` hook runs the mutating format task (`mise run format:format`), the result is re-staged with `git add`, and the commit succeeds without being rejected for formatting

#### Scenario: CI verifies without mutating

- **GIVEN** the CI Format and Clippy jobs
- **WHEN** they run
- **THEN** they invoke the non-mutating `mise run format:check` and `mise run clippy:check`
- **AND** a violation fails the job loudly with a diff/error
- **AND** the working tree is never modified by CI

### Requirement: CI reports check failures per check

The CI workflow SHALL run checks as separate parallel jobs so that a failure names the specific check that failed.

**User value:** A contributor sees exactly which check failed (Format, Clippy, Check, Deny, …) rather than one opaque aggregated step, and CI wall-clock stays at the parallel maximum rather than the serial sum of all checks.

#### Scenario: A failing check is identifiable

- **GIVEN** the CI pipeline with parallel per-check jobs
- **WHEN** one check fails
- **THEN** the corresponding named job (e.g. `Clippy`) reports the failure
- **AND** the other check jobs report their own independent status
