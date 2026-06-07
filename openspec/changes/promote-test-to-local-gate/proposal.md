## Why

A contributor who wants the happy-path "did I break anything?" answer today must run five separate commands (`mise run check`, `format:check`, `clippy:check`, `lint:size`, `test`). There is no single local gate that mirrors the PR-check jobs, so people either skip checks before pushing or memorize the list. Promoting `test` into a one-command aggregator of the fast verification set gives contributors a single `mise run test` that reproduces CI's verdict locally.

## What Changes

- **Promote `test` to a local-gate aggregator.** `test` becomes an empty-body aggregator task with `depends=["check", "format:check", "clippy:check", "lint:size", "test:unit"]`, mirroring the PR-check jobs in `build.yml`. mise runs the dependencies in parallel (default `--jobs`), so wall-clock ≈ the slowest member, not the sum.
- **Move the unit suite to a `test:unit` leaf.** The current `cargo test --locked --lib` body moves to a new `test:unit` task. This frees the `test` name for the aggregator and follows the `namespace:variant` convention established for `format:format` / `clippy:check`. (Like those, `test` becomes a directory: `test/test` = aggregator, `test/unit` = leaf.)
- **Repoint the two existing consumers of `test`** to `test:unit`: the CI `Unit Tests` job (`build.yml`) runs `mise run test:unit`, and the `test-all` aggregator's `depends` becomes `["test:unit", "integ", "e2e"]`. Both want the *unit suite*, not the gate.
- **Document the set's one duplication with a sync comment.** The aggregator's `depends` and `build.yml`'s separate PR-check jobs both enumerate the check set (CI keeps separate named jobs per the `CI reports check failures per check` requirement). A `# keep in sync with build.yml PR-check jobs` comment marks the link. No enforcement guard is added — the set churns rarely and a guard's maintenance cost would exceed the drift it prevents at this churn rate (revisit if drift is actually felt).

No **BREAKING** user-facing changes — this is dev tooling only. `mise run test` changes meaning (now the gate, not just unit tests); the unit suite is one keystroke away at `mise run test:unit`.

## Capabilities

### New Capabilities
<!-- None. -->

### Modified Capabilities
- `task-execution-consistency`: adds a requirement that a single local command runs the fast verification set by aggregating the existing per-check tasks. The three existing requirements (single-definition, mutate-vs-verify, per-check CI jobs) are unchanged — the local gate composes them by reference and CI keeps its separate named jobs.

## Impact

- **`.config/mise/tasks/test`** → becomes the directory `test/`: `test/test` (aggregator, empty body + `depends` + sync comment) and `test/unit` (`cargo test --locked --lib`, the old body). Task names `test` and `test:unit`.
- **`.config/mise/tasks/test-all`** → `depends` changes from `["test", ...]` to `["test:unit", "integ", "e2e"]`.
- **`.github/workflows/build.yml`** → the `Unit Tests` job's `mise run test` becomes `mise run test:unit`. No other job changes; the parallel per-check job structure is preserved.
- No changes to application code, CLI behavior, public APIs, or dependencies. `lint:size` is included explicitly in the gate (it is `--test code_health`; `test:unit` is `--lib`, so no double-run).
