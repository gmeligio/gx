## Why

On 2026-08-03 the Release-plz PR job failed with `the working directory of this project has uncommitted changes: [".config/mise.lock"]` ([run 30849375177](https://github.com/gmeligio/gx/actions/runs/30849375177/job/91805400264)). Because `.config/mise.toml` sets `lockfile = true`, every `mise install` is a lockfile *writer*, and CI's `mise-action` runs one unconditionally in all 16 call sites. Something in that write dirtied the tree, and release-plz aborted.

`lockfile-integrity` already predicts this outcome and accepts it ("mise.lock / gx.lock drift has no CI backstop … caught only later, when the release pipeline aborts on a dirty working tree"), on the reasoning recorded in Decision 5 of `2026-06-06-add-prek-lockfile-hooks`: the only way drift reaches CI is a bypassed or un-installed local hook, which is implausible for a solo, always-bootstrapped repo.

That premise held and the gap bit anyway, via a path Decision 5 did not consider: **the drift originated in CI, not in a contributor's commit**. The local hooks were installed and the committed lockfile was correct — no hook installation could have prevented this, because nothing about the commit was wrong. What differed was CI's own install conditions.

The evidenced discriminator is the mise **cache miss**, not any property of the commit:

| Run | mise | cache | tools installed fresh | result |
|---|---|---|---|---|
| [30849375177](https://github.com/gmeligio/gx/actions/runs/30849375177) | 2026.8.1 | **miss** | 10 install lines (all 5 tools) | ✗ dirty lockfile |
| 30849646481 | 2026.8.1 | hit | 2 (rust only) | ✓ |
| 30851398527 | 2026.8.1 | hit | 2 (rust only) | ✓ |

On a cache hit only `rust` reinstalls, and the `core:rust` backend writes no lock entry; the four archive-backed tools (prek, shellcheck, cargo-deny, cargo-dist) are the ones that touch the lockfile, and only on a cold install. So the failure is intermittent by construction: it needs a cache miss, which happens on cache eviction, a cache-key change, or a new mise release.

The exact field that changed is **not yet identified**, and the mise version's role is unconfirmed: mise 2026.8.1 has since run three times on `main` without dirtying the lockfile, so "CI's mise is newer than the developer's" is not on its own sufficient. A cold install under 2026.7.13 with an isolated data dir was also reproduced clean. The trigger therefore appears to need a cold install *and* something version-specific together, which could not be tested locally (mise binary downloads truncate in the investigation environment). This change does not depend on resolving that: `git diff --exit-code` detects the drift whatever writes it, and prints the field — which is precisely what the current failure mode hides. This is the third instance of the class (#66, #112) and the first to break a release; the first two were each diagnosed by hand after the fact.

The deliberately-omitted CI job is worth re-adding because the recorded trade-off assumed drift could only enter through a contributor's commit. It can also enter through CI's install conditions, where the local hooks have no reach.

## What Changes

- Add a `lock` mise task wrapping the mutating `mise install`, and a `lock:check` variant that runs it and asserts `.config/mise.lock` is unchanged. This follows the codified mutate-locally / verify-in-CI pattern (`format`/`format:check`, `clippy`/`clippy:check`) and closes a standing violation: the `mise-lockfile` hook is the only hook that invokes a tool inline instead of via `mise run <task>`.
- Add a `Lockfile` job to `build.yml` running `mise run lock:check`, so drift fails the PR that surfaces it instead of the next release.
- Add `lock:check` to the `test` local-gate `depends`, keeping the gate in sync with the PR-check jobs as its comment requires.
- Change the `mise-lockfile` prek hook to re-stage after regenerating (`mise run lock && git add -u`), matching `cargo-fmt`, `cargo-linter`, and `gx-lockfile`. **BREAKING** to the current `lockfile-integrity` requirement that a hook-modified lockfile SHALL block the commit — for `.config/mise.lock` it now auto-fixes and re-stages.

No lockfile regeneration is needed: `main` is green and `.config/mise.lock` is already a fixed point for the current mise (`mise install` produces no diff).

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
- `.config/mise.lock` — unchanged (already a fixed point for the current mise)
- `openspec/specs/lockfile-integrity/spec.md`, `openspec/specs/task-execution-consistency/spec.md`
- No application code, CLI, public API, or dependency changes. Adds ~30s of parallel CI wall-clock; no change to the serial critical path.
