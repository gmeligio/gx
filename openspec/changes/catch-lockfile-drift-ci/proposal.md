## Why

The `release-plz` pipeline failed on a push to `main` because `.config/mise.lock` was dirty on the runner: release-plz aborts `release-pr` whenever the working tree has uncommitted changes. The lockfile drifted because mise floats to its latest version (intentional — we want new features), and a recent mise release changed a value baked into the lockfile's `@generated` header (`mise.jdx.dev` → `mise.en.dev`); with `locked = false` in `.config/mise.toml`, `mise install` silently rewrote the lockfile in place. The same class of silent drift can affect any tracked lockfile, and today nothing catches it until the release pipeline breaks — the worst place to discover it.

## What Changes

- Add PR-time guards so any lockfile drift fails a pull request (cheap, recoverable) instead of the release pipeline (late, blocking).
- `Cargo.lock`: add `--locked` to the cargo invocations CI already runs (`cargo check`, and the `clippy`/`test` mise tasks) so cargo asserts the lock is unchanged — prevention, ~zero cost.
- `.github/gx.lock`: run `gx tidy` in CI (dogfooding — `gx` is this project) to assert the manifest and lock match the workflow code.
- `.config/mise.lock`: cannot use mise's native `--locked` install — on a cold CI runner it triggers the rust `core:rust` catch-22 (`rust@1.95.0 is not in the lockfile`), the very reason `locked = false` exists. Instead, **detect** drift: after mise-action's normal (unlocked) install, run `git diff --exit-code .config/mise.lock` and fail the PR with guidance to commit the regenerated lock.
- Commit the already-regenerated `.config/mise.lock` (the `en.dev` header) as an immediate, separate step so the pending release unblocks.
- Keep mise unpinned — this change makes drift safe to follow, it does not freeze the version.

## Capabilities

### New Capabilities

- `lockfile-integrity`: a project-direction guarantee that every tracked lockfile is verified in CI before merge, so lockfile drift fails a PR check instead of the release pipeline. The traceable user value (cf. `homebrew-release`): maintainers and release consumers are protected from releases that silently break on lockfile drift. This does not add, remove, or modify any `gx` CLI command, flag, or output — `gx tidy` is used as an existing capability, not introduced here.

### Modified Capabilities

_None._ No `gx` requirement or spec-level behavior changes.

## Impact

- `.github/workflows/build.yml` — add lockfile-guard step(s) on the PR jobs (`gx tidy`; `git diff --exit-code .config/mise.lock` after mise-action).
- `.config/mise/tasks/clippy`, `.config/mise/tasks/test` — pass `--locked` to cargo so the flag applies wherever the task runs (local and CI).
- `.config/mise.lock` — commit the regenerated `en.dev` header to unblock release-plz.
- No application code, dependencies, or public API affected. No change to the `release-plz.yml` workflow itself; the fix is to keep the lock current upstream of it.
