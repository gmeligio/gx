## Why

`.config/mise.lock` depends on two inputs: `mise.toml` and the version of mise that writes it, because each mise release can pick different assets (for example, gnu vs musl). Today two writers disagree:

- the contributor's local mise, which upgrades independently through the OS package manager;
- CI's mise, which is pinned through `min_version` and rewrites the lockfile during `mise install`.

So the `Lockfile` job and release-plz fail whenever the two versions differ (#228, #230). See `research.md`.

## What Changes

- Make CI read-only on `.config/mise.lock`. Upgrade `jdx/mise-action` from `^3` to `^5`. Since v4.1 it installs with `--locked` automatically, so CI verifies the lockfile and never rewrites it.
- **BREAKING** Stop pinning mise in CI. Remove the `MISE_VERSION` env and `version:` inputs from every workflow; mise-action then installs the newest stable mise that is at least 24h old. Delete:
  - `min_version`
  - the `mise-version` task and its pre-commit hook
- **BREAKING** Delete the `Lockfile` CI job and the `lock:check` task, and drop `lock:check` from the local `test` gate. A stale or missing lock entry now fails the locked install in every CI job.
- Run the `mise-lockfile` pre-commit hook only when `.config/mise.toml` or `rust-toolchain.toml` changes, so a local mise upgrade alone no longer rewrites the lockfile.
- Remove the stale `locked = false` workaround if a locked CI install of `core:rust` passes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `lockfile-integrity`:
  - The mise lockfile's CI backstop moves from a dirty-tree diff to a locked install.
  - The mise hook stops running on every commit.
  - Mise version drift between local and CI is no longer a failure.
- `task-execution-consistency`:
  - `lock:check` leaves the CI job list, the per-check job examples, and the local gate's membership.
  - The convergent-writer verification scenario no longer names `mise lock`.

## Impact

- Workflows: `.github/workflows/build.yml`, `release.yml`, `release-plz.yml`.
- `.github/gx.toml` and `.github/gx.lock` (mise-action `^5`).
- `.config/mise.toml`.
- Tasks: `.config/mise/tasks/mise-version`, `lock/check`, `test/_default`.
- Hooks: `.pre-commit-config.yaml`.
- Contributors need no specific mise version. Any recent mise writes a lockfile CI accepts.
- Contributor-facing: the `Lockfile` check disappears from PRs. Lockfile errors surface in whichever job installs tools first.
