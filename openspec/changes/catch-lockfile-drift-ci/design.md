## Context

The repo tracks three lockfiles, each with a different stability contract:

- `Cargo.lock` — content is a pure function of the dependency graph, decoupled from the cargo binary version. `cargo build`/`check` only rewrite it when the graph changes. Upgrading cargo does not drift it.
- `.config/mise.lock` — content depends on the **mise binary's** output format/metadata, not just the tools. A mise upgrade can rewrite generated fields (e.g. the `@generated` header URL `mise.jdx.dev` → `mise.en.dev`). The project intentionally floats mise to latest (no pin) and sets `locked = false` in `.config/mise.toml` to work around a `core:rust` catch-22 (under `locked = true`, mise *requires* rust in the lockfile but *cannot write* the `core:rust` entry on a cold install). With `locked = false`, `mise install` silently rewrites the lock in place.
- `.github/gx.lock` — `gx`'s own lockfile (gx is this project), resolving GitHub Actions versions. `gx tidy` asserts the manifest+lock match the workflow code.

The triggering failure: a push to `main` ran `release-plz`, whose `release-pr` command aborts on a dirty working tree. mise-action had silently rewritten `.config/mise.lock` (the `en.dev` header), dirtying the tree, so release-plz failed. Nothing caught the drift earlier.

CI: `.github/workflows/build.yml` runs on `pull_request` to `main`; every job already runs `jdx/mise-action`. `release-plz.yml` runs on push to `main`.

## Goals / Non-Goals

**Goals:**
- Catch lockfile drift at PR time, where the fix is a one-line commit, instead of in the release pipeline, where it blocks a release.
- Use each tool's own verification mechanism where one exists and is safe.
- Keep mise unpinned so the project keeps following new mise features.

**Non-Goals:**
- Pinning the mise version (explicitly rejected — we want latest).
- Removing or revisiting `locked = false` / the `core:rust` catch-22 workaround.
- Changing any `gx` CLI behavior, flag, or output.
- Changing `release-plz.yml` itself; the fix keeps the lock current upstream of it.

## Decisions

**Decision 1: Per-tool native checks, not a single generic `git diff`.**
Each lockfile gets verified by the tool that owns it, because native checks give precise, actionable failure messages ("Cargo.lock out of date" vs "something changed") and don't false-positive on incidental files.
- `Cargo.lock` → `cargo … --locked` ("assert Cargo.lock will remain unchanged").
- `.github/gx.lock` → `gx tidy`.
- _Alternative considered_: one whole-tree `git diff --exit-code` job. Simpler and zero-maintenance, but opaque failures and whole-tree sensitivity. Rejected as the primary mechanism; a *scoped* diff is still used for mise (Decision 3).

**Decision 2: Add `--locked` to existing cargo invocations rather than a new job.**
Add `--locked` to `cargo check` (build.yml) and to the cargo calls inside the `clippy` and `test` mise tasks (`.config/mise/tasks/`). Putting the flag in the task (not just the workflow) means it applies wherever the task runs — local and CI. ~Zero added cost; reuses runs CI already performs.
- _Alternative_: a dedicated `cargo verify --locked` job. Rejected — redundant compute for no extra signal.

**Decision 3: mise gets DETECTION (scoped diff), not native `--locked`.**
`MISE_LOCKED=1` / `mise install --locked` cannot be used: on a cold CI runner it triggers the `core:rust` catch-22 (`rust@1.95.0 is not in the lockfile`) — the exact failure `locked = false` exists to avoid. Local tests that "passed" did so only against a warm cache where mise short-circuits resolution; the project's own notes prescribe a cold-data-dir repro to see the real failure. Therefore: after mise-action's normal (unlocked) install, run `git diff --exit-code .config/mise.lock`; on drift, fail the PR with a message to run `mise install` and commit the regenerated lock.
- _Alternative_: flip `locked = true` and add rust to the lock manually. Rejected — fragile (`mise lock` strips rust) and reintroduces the catch-22.

**Decision 4: Commit the regenerated `.config/mise.lock` now, as a separate step.**
The `en.dev` header is already regenerated locally. Committing it unblocks the pending release immediately and is independent of the CI-guard work, so it ships first.

## Risks / Trade-offs

- **mise drift still requires a manual commit when it happens** → Acceptable and intended: the guard converts a silent release-breaker into a loud, early, one-commit PR fix. The catch-22 forbids auto-prevention for mise.
- **`gx tidy` needs gx available in the CI job** → Use the existing `mise run install` task (puts gx in `~/.cargo/bin`) before invoking `gx tidy`.
- **`cargo --locked` could surface a genuinely stale `Cargo.lock` on unrelated PRs** → That is the desired behavior; the fix is the same one-line `cargo update`/commit, caught early.
- **Scoped mise diff only watches `.config/mise.lock`** → If a future fourth lockfile appears, it won't be covered automatically. Acceptable; adding a line is cheap and explicit beats magic.

## Automated Test Strategy

This is a CI-configuration change; verification is the CI run itself, not unit tests.
- **Critical path**: open a PR and confirm (a) `cargo --locked` steps pass on a clean `Cargo.lock` and fail on a deliberately stale one; (b) `gx tidy` passes when gx.lock matches workflows; (c) the mise diff step is green when the lock is committed.
- **Negative test**: temporarily revert the `.config/mise.lock` header (or simulate drift) on a throwaway branch and confirm the diff step fails the PR with the intended guidance — proving the guard actually catches the original failure mode.
- **No new test infrastructure** is required; the existing `build.yml` PR jobs are the harness.

## Observability

- **How failures surface**: each guard fails its CI step with a non-zero exit, marking the PR check red before merge — the inverse of the original silent drift that only appeared at release time.
- **Messages**: `cargo --locked` and `gx tidy` emit their own descriptive errors. The mise diff step must print an explicit remediation line (e.g. "`.config/mise.lock` drifted — run `mise install` and commit the result") so the failure is self-explanatory; a bare `git diff --exit-code` failure is otherwise cryptic.
- **No silent failure path**: every guard is a hard CI failure, not a warning. There is no log-and-continue branch.

## Migration Plan

1. Commit the regenerated `.config/mise.lock` (unblocks release-plz immediately).
2. Add `--locked` to `cargo check` and the `clippy`/`test` mise tasks.
3. Add the `gx tidy` step (after `mise run install`) and the `git diff --exit-code .config/mise.lock` step to the PR jobs in `build.yml`.
4. Open a PR; verify all guards behave (incl. the negative test above).
- **Rollback**: revert the workflow/task edits; guards are additive and carry no runtime/data risk.

## Open Questions

- Should the cargo `--locked` and the mise diff live in one consolidated "lockfiles" PR job, or stay attached to the existing per-tool jobs? Leaning toward keeping `--locked` on existing cargo jobs (free) and grouping `gx tidy` + mise diff into one small job — to be finalized during implementation.
