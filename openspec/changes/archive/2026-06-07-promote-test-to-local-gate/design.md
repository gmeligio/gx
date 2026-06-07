## Context

The just-archived `consolidate-task-execution` change made mise tasks the single source of truth for every check and gave each a fast `:check` (verify) variant. What it did not add is a *single local entry point* that runs the fast set. Today a contributor runs five commands by hand; this change adds one task that aggregates them.

Three facts established during research (all verified against mise 2026.6.0 in this repo):
- **mise aggregator tasks already exist here.** `test-all` is an empty-body task with `depends=["test", "integ", "e2e"]` — the exact pattern this change reuses.
- **`depends` runs in parallel by default** (`mise run --jobs` defaults to 8). A gate over `{check, format:check, clippy:check, lint:size, test:unit}` has wall-clock ≈ the slowest member (`test:unit`, ~9.5s) not the sum (~14s). Parallel output interleaves; only matters on failure.
- **The dir-namespacing mechanic is known.** Making `test` an aggregator-with-a-namespaced-leaf turns `test` into a directory (`test/test` = aggregator, `test/unit` = leaf), exactly as `format`/`clippy` became directories last change. mise has no primary-file-in-dir convention, so the bare `test` name is the aggregator file `test/test`.

## Goals / Non-Goals

**Goals:**
- One local command (`mise run test`) runs the fast verification set, reproducing the CI PR-check verdict for those checks.
- The gate aggregates the existing per-check tasks by reference — zero command duplication; changing a check's flags happens once in that check's task.
- The unit suite stays independently runnable for CI and `test-all`, which need *unit tests*, not the gate.
- Lowest long-term maintenance for the chosen scope (Level 1): a sync comment, no enforcement guard.

**Non-Goals:**
- A drift-enforcement test asserting the gate's `depends` matches `build.yml`'s jobs (Level 2). Rejected for now — see Decisions D3.
- Deriving CI's jobs from the mise list via a dynamic matrix (Level 3). Rejected — see D3.
- Including the slow checks (`integ`, `e2e`, `deny`) in the gate. The gate is the *fast* happy path; `test-all` already covers the full test suite.
- Changing any check's command, CI's per-check job structure, or the pre-commit hooks.

## Decisions

### D1. `test` becomes the aggregator; the unit suite moves to `test:unit`
The name `test` is overloaded the moment it must mean both "the unit suite" (what CI's Unit Tests job and `test-all` need) and "my local gate" (what the contributor wants to type). Resolution: give each meaning its own name. `test` = the gate (`depends` only), `test:unit` = the leaf (`cargo test --locked --lib`). Each name maps to exactly one job, which is the property that keeps the task set legible as it grows.
- *Alternative — keep `test` as the unit suite and name the gate something else (`verify`/`gate`):* viable and zero-breakage, but the user explicitly wants to type `test` for the gate. Rejected on that preference.

### D4. The `test` gate is a TOML task; the `test:unit` leaf is a file
mise 2026.6.x has **no primary-file-in-directory convention** (verified, same finding as the archived `consolidate-task-execution` D7): a file inside `test/` only ever produces a namespaced task name — `test/test` → `test:test`, `test/default` → `test:default` — and with the directory present but no bare-`test` definition, `mise run test` errors with "no task". So the directory layout that worked for `format`/`clippy` (which did *not* need a bare name — both bare names were retired there) cannot deliver the bare `test` name this change requires.
The only layout that keeps **both** bare `test` (the gate) **and** `test:unit` (the leaf) is to define `test` *outside* the directory: a `[tasks.test]` block in `.config/mise.toml`. Verified: a TOML `test` task coexists with the `test/unit` file task; `mise run test` resolves to the gate, `mise run test:unit` to the leaf.
- *Trade-off:* this introduces one TOML-defined task into an otherwise all-file task set (`.config/mise/tasks/`). Accepted because it is the only way to honor the explicit "type `mise run test`" requirement; the divergence is a single, well-commented block, and the leaf + every other task stay file-based.
- *Alternative — gate named `verify` (all-file):* keeps one definition style but abandons the `test` name the user asked for. Rejected on that preference.
- *Alternative — gate as bare file `test`, leaf as bare file `unit` (not `test:unit`):* all-file and bare `test` both work, but drops the `test:unit` namespacing the spec specifies. Rejected to keep the leaf consistent with `format:format`/`clippy:check`.
- *Alternative — make `test` the gate but keep unit tests inline in its body + depend on the rest:* leaves `test-all` dragging the linters along (wrong scope), forcing a leaf rename anyway. Half-measure.

### D2. The gate includes `lint:size` explicitly
`clippy:check` deliberately dropped its `lint:size` dependency (CI's integ job runs `code_health`). So in the gate, `lint:size` must be listed explicitly to be covered. It does not double-run: `lint:size` is `cargo test --test code_health` while `test:unit` is `--lib` — disjoint targets.

### D3. Document the set's duplication with a comment; do NOT add an enforcement guard or a derived matrix
The check *set* is enumerated in two shapes: the gate's `depends` (collapsed, for the human) and `build.yml`'s separate named jobs (spread, required by the existing `CI reports check failures per check` requirement). These can drift when a *job is added/removed*.
- *Level 2 (guard test):* a `code_health`-style test asserting `build.yml` PR-check jobs ⊆ gate `depends`. Catches drift loudly but is ~20 lines of test coupled to the workflow YAML's structure.
- *Level 3 (derived matrix):* CI reads the set from `mise tasks ls --json` and builds a matrix. Removes drift entirely, but the jobs are not uniform (Format/Clippy need `setup-rust-toolchain`; E2E needs `GITHUB_TOKEN`), so a matrix needs per-entry conditionals — trading a readable 7-job file for an opaque one, and losing the plain legibility the per-check-jobs requirement values.
- **Chosen — Level 1 (comment):** the set has low churn (the archived change touched these jobs and the set is stable), so the drift window is small. A `# keep in sync with build.yml PR-check jobs` comment marks the link. The guard is reversible insurance — a self-contained test that can be added later *if* drift is actually felt — so choosing Level 1 now does not foreclose it.

## Risks / Trade-offs

- **`mise run test` changes meaning** (was unit tests, now the gate) → Mitigation: the unit suite is one keystroke away at `mise run test:unit`; the change is dev-tooling only with no external consumers (grep confirms the only `mise run test` references are `build.yml` and `test-all`, both repointed).
- **Set drift between the gate and `build.yml` (D3)** → Mitigation: sync comment + low churn; revisit with the Level-2 guard if drift occurs. Accepted deliberately as the lowest-maintenance option for the current churn rate.
- **Parallel `depends` interleaves output** → Mitigation: irrelevant for a green/red gate; output is only read on failure, and each member's failure is attributable by its task label.
- **Forgetting to repoint a `test` consumer** → Mitigation: only two exist (`build.yml` Unit Tests job, `test-all` depends); both are in the task list and verified by grep after the edit.

## Automated Test Strategy

No new application tests; this is task-graph plumbing verified by exercising the tasks and the pipeline it feeds:
- **Critical path:** `mise run test` runs check + format:check + clippy:check + lint:size + test:unit and exits non-zero if any fails; confirm by deliberately breaking one member (e.g. a lint) and seeing the gate fail naming that member.
- **Leaf integrity:** `mise run test:unit` runs exactly `cargo test --locked --lib` (the old `test` behavior) — confirm output matches the prior unit-test run.
- **Consumer repointing:** push the branch and confirm CI's Unit Tests job (now `mise run test:unit`) is green, and `mise run test-all` still resolves (`test:unit`, `integ`, `e2e`).
- **No double-run / no drift:** confirm `lint:size` runs once under the gate and that every `build.yml` PR-check job has a corresponding member in the gate's `depends` (manual check, per D3 — no automated guard this change).

## Observability

- **Gate failures are attributable:** mise labels each dependency's output with its task name (`[clippy:check] …`), so a failing member is named even though they run in parallel.
- **CI unchanged and still per-check:** the Unit Tests job runs `test:unit`; all other PR-check jobs are untouched, so CI continues to name the specific failing check (the existing requirement holds).
- **No silent drift mitigation beyond the comment:** D3 accepts that set-membership drift is caught by humans, not a test — the sync comment is the only signal. This is a conscious trade documented here so a future maintainer who feels drift knows the Level-2 guard is the intended escalation.

## Migration Plan

1. Convert `.config/mise/tasks/test` (file) into a directory: `test/unit` gets the old body (`cargo test --locked --lib`). Add the gate as a `[tasks.test]` block in `.config/mise.toml` with `depends=[check, format:check, clippy:check, lint:size, test:unit]` + the sync comment (D4).
2. Repoint `test-all` `depends` → `["test:unit", "integ", "e2e"]`.
3. Repoint `build.yml` Unit Tests job → `mise run test:unit`.
4. Verify locally (Automated Test Strategy), then push and confirm CI green.

Rollback: revert the touched files (the `.config/mise/tasks/test` directory move, `test-all`, `build.yml`); no state migrated, pure git revert.
