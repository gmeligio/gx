## ADDED Requirements

### Requirement: A single local command runs the fast verification set

There SHALL be one mise task that, when run, executes the fast local verification set (compile check, format verification, lint verification, file-size budgets, and unit tests) by depending on the existing per-check tasks rather than re-spelling their commands. Running it SHALL reproduce the verdict of the CI PR-check jobs for those checks. The task SHALL aggregate by reference (its `depends` list names other tasks), so adding or changing a check's command happens once in that check's own task and the gate picks it up automatically.

**User value:** A contributor gets the happy-path "did I break anything?" answer with one command instead of memorizing and running five. Because the gate composes the same tasks CI runs, "passes locally" predicts "passes in CI" for the fast checks. Because it aggregates by reference, the gate cannot drift from the individual checks' commands — only the *membership* of the set is duplicated (with CI's separate jobs), and that is documented with a sync comment.

#### Scenario: One command runs the fast verification set

- **GIVEN** a contributor with mise available
- **WHEN** they run the local gate task (`mise run test`)
- **THEN** mise runs the compile check, format verification, lint verification, file-size budget check, and unit tests by resolving the task's `depends`
- **AND** any single failing member fails the gate
- **AND** the members run in parallel (mise default), so wall-clock approximates the slowest member rather than the sum

#### Scenario: The gate composes checks by reference, not by copied commands

- **GIVEN** the local gate task and the per-check tasks it depends on
- **WHEN** a check's command or flags change in its own task
- **THEN** the gate picks up the change automatically because it names the task in `depends`, not a copied command
- **AND** no check command is duplicated between the gate and the individual check tasks

#### Scenario: The unit suite remains independently runnable

- **GIVEN** the unit-test suite has its own task (`test:unit`)
- **WHEN** CI's Unit Tests job or the `test-all` aggregator needs only unit tests
- **THEN** it invokes `mise run test:unit` (the leaf), not the local gate
- **AND** the gate and the leaf each map to exactly one job (the gate aggregates; the leaf runs `cargo test --locked --lib`)
