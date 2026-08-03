## MODIFIED Requirements

### Requirement: Developer checks have one definition invoked by every environment

Every project check (format, clippy, supply-chain/deny, lockfile tidy, lockfile sync, compile check) SHALL be defined exactly once as a mise task. The local shell, the pre-commit hooks, and CI SHALL all invoke that check through `mise run <task>` rather than re-spelling the underlying `cargo`/`clippy`/`rustfmt`/`mise` command. No `build.yml` step and no pre-commit hook SHALL invoke `cargo`, `clippy`, `rustfmt`, or `mise install` directly for a check that has a mise task.

**User value:** Contributors never get a CI failure caused by a check that passed locally but used different flags than CI — the drift class that caused the PR #102 `cargo-deny unmatched-skip-root` failure (the `-D warnings` flag lived only in the mise task). Changing a check's flags in one place moves every environment at once. This is the same contributor-facing guarantee as `lockfile-integrity`, applied to check definitions.

#### Scenario: A check's flags live in exactly one place

- **GIVEN** any check that runs in both CI and locally (format, clippy, deny, check, lock)
- **WHEN** its command or flags need to change
- **THEN** the change is made once in the check's mise task
- **AND** CI and the pre-commit hook both pick up the change because they call `mise run <task>`, not a copied command

#### Scenario: CI contains no inline check commands

- **GIVEN** the CI workflow `build.yml`
- **WHEN** a check job runs
- **THEN** its `run:` step is `mise run <task>` (e.g. `mise run format:check`, `mise run clippy:check`, `mise run check`, `mise run deny`, `mise run lock:check`)
- **AND** no check job invokes `cargo fmt`, `cargo clippy`, `rustfmt`, or `mise install` directly

#### Scenario: Pre-commit hooks contain no inline check commands

- **GIVEN** the pre-commit hook configuration
- **WHEN** a hook runs a check that has a mise task
- **THEN** its entry invokes `mise run <task>` rather than the underlying tool
- **AND** this includes the lockfile-sync hook, which invokes `mise run lock` rather than `mise install` directly

### Requirement: Local and pre-commit mutate; CI verifies

Checks that can fix their target (format, clippy, lockfile sync) SHALL mutate in the local and pre-commit environments and SHALL be verified non-mutating in CI. The mutating task and its `:check` verification variant SHALL share the same configuration (same `rustfmt.toml`, same `--all` scope, same lint set, same lockfile settings) so that only the verb differs. CI SHALL NOT modify the working tree. A `:check` variant whose underlying tool can only verify by writing SHALL run the mutating command and then assert the target is unchanged, failing with the resulting diff.

**User value:** A contributor's commit is auto-formatted and its lockfiles auto-synced in one shot instead of being told they are wrong; CI gates the immutable pushed commit and never silently rewrites contributor code. Because the mutate and verify variants share one configuration, they cannot disagree on what "correct" means.

#### Scenario: Pre-commit fixes and re-stages in one shot

- **GIVEN** a contributor with the hooks installed
- **AND** a staged change containing misformatted Rust
- **WHEN** they commit
- **THEN** the `cargo-fmt` hook runs the mutating format task (`mise run format`), the result is re-staged with `git add`, and the commit succeeds without being rejected for formatting

#### Scenario: CI verifies without mutating

- **GIVEN** the CI Format and Clippy jobs
- **WHEN** they run
- **THEN** they invoke the non-mutating `mise run format:check` and `mise run clippy:check`
- **AND** a violation fails the job loudly with a diff/error
- **AND** the working tree is never modified by CI

#### Scenario: A write-only tool is verified by asserting no change

- **GIVEN** a check whose tool cannot verify without writing (`mise install`, which must write the lockfile because `locked = false` is required by the `core:rust` backend)
- **WHEN** its `:check` variant runs in CI
- **THEN** it runs the mutating task and then asserts the target file is unchanged
- **AND** a difference fails the job with the diff printed, naming the field that changed
- **AND** the assertion is scoped to that lockfile, so unrelated writes by other tooling do not trip it

### Requirement: CI reports check failures per check

The CI workflow SHALL run checks as separate parallel jobs so that a failure names the specific check that failed.

**User value:** A contributor sees exactly which check failed (Format, Clippy, Check, Deny, Lockfile, …) rather than one opaque aggregated step, and CI wall-clock stays at the parallel maximum rather than the serial sum of all checks.

#### Scenario: A failing check is identifiable

- **GIVEN** the CI pipeline with parallel per-check jobs
- **WHEN** one check fails
- **THEN** the corresponding named job (e.g. `Clippy`, `Lockfile`) reports the failure
- **AND** the other check jobs report their own independent status

### Requirement: A single local command runs the fast verification set

There SHALL be one mise task that, when run, executes the fast local verification set (compile check, format verification, lint verification, file-size budgets, lockfile verification, and unit tests) by depending on the existing per-check tasks rather than re-spelling their commands. Running it SHALL reproduce the verdict of the CI PR-check jobs for those checks. The task SHALL aggregate by reference (its `depends` list names other tasks), so adding or changing a check's command happens once in that check's own task and the gate picks it up automatically.

**User value:** A contributor gets the happy-path "did I break anything?" answer with one command instead of memorizing and running six. Because the gate composes the same tasks CI runs, "passes locally" predicts "passes in CI" for the fast checks. Because it aggregates by reference, the gate cannot drift from the individual checks' commands — only the *membership* of the set is duplicated (with CI's separate jobs), and that is documented with a sync comment.

#### Scenario: One command runs the fast verification set

- **GIVEN** a contributor with mise available
- **WHEN** they run the local gate task (`mise run test`)
- **THEN** mise runs the compile check, format verification, lint verification, file-size budget check, lockfile verification, and unit tests by resolving the task's `depends`
- **AND** any single failing member fails the gate
- **AND** the members run in parallel (mise default), so wall-clock approximates the slowest member rather than the sum

#### Scenario: The gate composes checks by reference, not by copied commands

- **GIVEN** the local gate task and the per-check tasks it depends on
- **WHEN** a check's command or flags change in its own task
- **THEN** the gate picks up the change automatically because it names the task in `depends`, not a copied command
- **AND** no check command is duplicated between the gate and the individual check tasks

#### Scenario: The unit suite remains independently runnable

- **GIVEN** the unit-test suite has its own task (`test:unit`)
- **WHEN** CI's Unit Tests job or the `test:all` aggregator needs only unit tests
- **THEN** it invokes `mise run test:unit` (the leaf), not the local gate
- **AND** the gate and the leaf each map to exactly one job (the gate aggregates; the leaf runs `cargo test --locked --lib`)
