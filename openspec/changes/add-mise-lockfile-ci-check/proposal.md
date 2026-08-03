## Why

On 2026-08-03 the Release-plz PR job failed with `the working directory of this project has uncommitted changes: [".config/mise.lock"]` ([run 30849375177](https://github.com/gmeligio/gx/actions/runs/30849375177/job/91805400264)). CI's `mise-action` pulls whatever `https://mise.jdx.dev/VERSION` returns — that morning, `2026.8.1`, released the same day — and because `.config/mise.toml` sets `lockfile = true`, every `mise install` is a lockfile *writer*. The new binary rewrote `.config/mise.lock`, and release-plz aborted on the dirty tree.

`lockfile-integrity` already predicts this outcome and accepts it ("mise.lock / gx.lock drift has no CI backstop … caught only later, when the release pipeline aborts on a dirty working tree"), on the reasoning recorded in Decision 5 of `2026-06-06-add-prek-lockfile-hooks`: the only way drift reaches CI is a bypassed or un-installed local hook, which is implausible for a solo, always-bootstrapped repo.

That premise held and the gap bit anyway, via a path Decision 5 did not consider: **the drift originated in CI, not in a contributor's commit**. CI's floating mise was newer than any developer's, so no hook installation could have prevented it — the lockfile was correct for `2026.7.13` and wrong for `2026.8.1` at the moment CI ran. This is the third instance of the same class (#66, #112), and the first to break a release. The recorded trade-off no longer covers the failure mode, so the deliberately-omitted CI job is worth re-adding.

## What Changes

- Add a `lock` mise task wrapping the mutating `mise install`, and a `lock:check` variant that runs it and asserts `.config/mise.lock` is unchanged. This follows the codified mutate-locally / verify-in-CI pattern (`format`/`format:check`, `clippy`/`clippy:check`) and closes a standing violation: the `mise-lockfile` hook is the only hook that invokes a tool inline instead of via `mise run <task>`.
- Add a `Lockfile` job to `build.yml` running `mise run lock:check`, so drift fails the PR that surfaces it instead of the next release.
- Add `lock:check` to the `test` local-gate `depends`, keeping the gate in sync with the PR-check jobs as its comment requires.
- Change the `mise-lockfile` prek hook to re-stage after regenerating (`mise run lock && git add -u`), matching `cargo-fmt`, `cargo-linter`, and `gx-lockfile`. **BREAKING** to the current `lockfile-integrity` requirement that a hook-modified lockfile SHALL block the commit — for `.config/mise.lock` it now auto-fixes and re-stages.
- Regenerate `.config/mise.lock` under the current mise so `main` is green.

Out of scope: pinning mise's version (removes the drift's cause rather than detecting it — worth doing, but a separate concern), and a matching `tidy:check` for `.github/gx.lock` (its writer is gx-compiled-from-source, pinned by this repo, so it has no floating input and cannot drift this way).

## Capabilities

### New Capabilities
<!-- none — this change modifies requirements in two existing specs -->

### Modified Capabilities
- `lockfile-integrity`: the "mise.lock / gx.lock drift has no CI backstop" scenario no longer holds for `.config/mise.lock` — CI verifies it. The requirement that a hook-modified lockfile blocks the commit changes for `.config/mise.lock`, which now auto-fixes and re-stages.
- `task-execution-consistency`: `mise install` becomes a mise task (`lock`) with a `:check` verification variant, bringing the last inline tool invocation under the "one definition, every environment invokes it" requirement and extending the mutate/verify pair to lockfiles.

**Relevance gate:** CI/tooling changes normally skip specs. This one qualifies because it changes *requirements* in two existing specs rather than only their implementation: it contradicts a scenario `lockfile-integrity` explicitly records as accepted, and it reverses a documented design decision. Leaving the specs unamended would make them describe a system that no longer exists.

## Impact

- `.config/mise/tasks/lock/_default` (new), `.config/mise/tasks/lock/check` (new)
- `.config/mise/tasks/test/_default` — `depends` gains `lock:check`
- `.github/workflows/build.yml` — new `Lockfile` job (9 jobs total)
- `.pre-commit-config.yaml` — `mise-lockfile` hook entry and description
- `.config/mise.lock` — regenerated under current mise
- `openspec/specs/lockfile-integrity/spec.md`, `openspec/specs/task-execution-consistency/spec.md`
- No application code, CLI, public API, or dependency changes. Adds ~30s of parallel CI wall-clock; no change to the serial critical path.
