## Why

A contributor who wants the happy-path "did I break anything?" answer today must run five separate commands (`mise run check`, `format:check`, `clippy:check`, `lint:size`, `test`). There is no single local gate that mirrors the PR-check jobs, so people either skip checks before pushing or memorize the list. Promoting `test` into a one-command aggregator of the fast verification set gives contributors a single `mise run test` that reproduces CI's verdict locally.

## What Changes

- **Promote `test` to a local-gate aggregator.** `test` becomes an empty-body aggregator task with `depends=["check", "format:check", "clippy:check", "lint:size", "test:unit"]`, mirroring the PR-check jobs in `build.yml`. mise runs the dependencies in parallel (default `--jobs`), so wall-clock ≈ the slowest member, not the sum.
- **Move the unit suite to a `test:unit` leaf.** The current `cargo test --locked --lib` body moves to a new `test:unit` file task (`.config/mise/tasks/test/unit`), following the `namespace:variant` convention established for `format:format` / `clippy:check`.
- **Define the `test` gate as a TOML task** in `.config/mise.toml` (`[tasks.test]` with `depends`), not as a file. mise 2026.6.x has no primary-file-in-directory convention, so a file inside `test/` can only produce a namespaced name (`test/anything` → `test:anything`) — the bare `test` name is unreachable from within the directory. A TOML task named `test` coexists with the `test/` file tasks and resolves `mise run test` to the gate (verified). This is the only layout that keeps both the bare `test` gate and a `test:unit` leaf; the cost is one task defined in TOML alongside the otherwise all-file tasks (see design D4).
- **Repoint the two existing consumers of `test`** to `test:unit`: the CI `Unit Tests` job (`build.yml`) runs `mise run test:unit`, and the `test-all` aggregator's `depends` becomes `["test:unit", "integ", "e2e"]`. Both want the *unit suite*, not the gate.
- **Document the set's one duplication with a sync comment.** The aggregator's `depends` and `build.yml`'s separate PR-check jobs both enumerate the check set (CI keeps separate named jobs per the `CI reports check failures per check` requirement). A `# keep in sync with build.yml PR-check jobs` comment marks the link. No enforcement guard is added — the set churns rarely and a guard's maintenance cost would exceed the drift it prevents at this churn rate (revisit if drift is actually felt).

No **BREAKING** user-facing changes — this is dev tooling only. `mise run test` changes meaning (now the gate, not just unit tests); the unit suite is one keystroke away at `mise run test:unit`.

## Capabilities

### New Capabilities
<!-- None. -->

### Modified Capabilities
- `task-execution-consistency`: adds a requirement that a single local command runs the fast verification set by aggregating the existing per-check tasks. The three existing requirements (single-definition, mutate-vs-verify, per-check CI jobs) are unchanged — the local gate composes them by reference and CI keeps its separate named jobs.

## Impact

- **`.config/mise/tasks/test`** → becomes the directory `test/` holding the leaf `test/unit` (`cargo test --locked --lib`, the old body). Task name `test:unit`.
- **`.config/mise.toml`** → new `[tasks.test]` TOML task: `depends = ["check", "format:check", "clippy:check", "lint:size", "test:unit"]`, a description, and a `# keep in sync with build.yml PR-check jobs` comment. Task name `test` (the gate).
- **`.config/mise/tasks/test-all`** → `depends` changes from `["test", ...]` to `["test:unit", "integ", "e2e"]`.
- **`.github/workflows/build.yml`** → the `Unit Tests` job's `mise run test` becomes `mise run test:unit`. No other job changes; the parallel per-check job structure is preserved.
- No changes to application code, CLI behavior, public APIs, or dependencies. `lint:size` is included explicitly in the gate (it is `--test code_health`; `test:unit` is `--lib`, so no double-run).
