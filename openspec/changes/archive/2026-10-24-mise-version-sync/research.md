# Research: keep mise current while local and CI stay in sync

## Context

The `Lockfile` job (`.github/workflows/build.yml:107-114`) runs `mise run lock:check`, which is `git diff --exit-code -- .config/mise.lock`. It fails on PRs such as #228 because two mise versions write the lockfile:

- Local mise comes from Fedora dnf (2026.10.13) and upgrades on its own schedule.
- CI mise is pinned to 2026.8.5. `min_version` in `.config/mise.toml:7` is sed-copied into `env.MISE_VERSION` of 3 workflows by `.config/mise/tasks/mise-version`, triggered by a pre-commit hook (`.pre-commit-config.yaml:36-42`).

```
local (dnf mise 2026.10.x)                 CI (mise-action v3.6.1, mise 2026.8.5)
pre-commit `mise-lockfile` hook            `mise install` (locked=false, rewrites lock)
always_run: `mise lock` ──▶ mise.lock ◀── write
                               │
                        lock:check: git diff ──▶ FAIL when the two writers disagree
```

## Findings

### Model (Phase 0 step 1)

- **The drift is a different binary, not formatting.** In the failed jobs on #228, prek's `linux-x64` entry flips between the gnu asset (local 2026.10) and the musl asset (CI 2026.8.5). mise changed libc/asset selection several times in 2026 ([v2026.3.4](https://newreleases.io/project/github/jdx/mise/release/v2026.3.4), [v2026.9.16](https://newreleases.io/project/github/jdx/mise/release/v2026.9.16)). So `mise.lock` is computed from mise.toml plus the mise version, and the mise version is not declared anywhere as an input.
- **`min_version` is used as an exact pin.** mise defines it as the "oldest mise release that can use this file", either a hard floor or a soft warning ([config](https://mise.jdx.dev/configuration.html)). A floor can't stop a newer local mise.
- **One concept lives in five places:** mise.toml, the `MISE_VERSION` env in 3 workflows, the sync task, the pre-commit hook, and generated-file comments. None of them covers the local install.
- **The lockfile has two writers.** mise's model is that developers write it, and CI runs `mise install --locked` and never writes ([mise.lock](https://mise.jdx.dev/dev-tools/mise-lock.html)). Here `locked = false` (`.config/mise.toml:26`) lets CI write too, so `lock:check` compares two writers rather than detecting tool drift.
- **The `locked = false` rationale is likely stale.** The lockfile already has a `[[tools.rust]]` entry, and `MISE_LOCKED=1 mise install --dry-run` passes locally on 2026.10.13. Not yet verified in CI.
- **`lockfile_platforms` is unset.** The platforms already in the file get reused ([mise lock](https://mise.jdx.dev/cli/lock.html)).

### Structure (Phase 0 step 2)

- **The drift gets in through the pre-commit hook.** `mise-lockfile` (`.pre-commit-config.yaml:43-49`) has `always_run: true`, so it runs `mise lock` with whatever local mise is installed on every commit, even ones that don't touch tools.
- **gx's own pin freezes the fix.** `.github/gx.toml` pins `"jdx/mise-action" = "^3"`, so CI stays on v3.6.1 (2026-01). Upstream is v5.2.3. Two releases in between matter ([releases](https://github.com/jdx/mise-action/releases)):
  - v4.1.0 passes `--locked` automatically when a mise.lock exists, so CI no longer writes the lock.
  - v5.0.0 defaults to the newest stable mise that is at least 24h old.
- **mise-action does not read `min_version`.** The sed task is the only link between them.
- **Renovate is not configured.** Its mise manager would run `mise lock` with its own mise version, adding a third writer ([renovate mise readme](https://github.com/renovatebot/renovate/blob/main/lib/modules/manager/mise/readme.md)).

```
mise version ──▶ asset selection ──▶ mise.lock content
     ▲                                     │
 undeclared                       2 writers (local hook, CI install)
```

## Options

| Approach | Pros | Cons |
|---|---|---|
| A. Keep the exact pin and sync local to it | Byte-identical lockfile | dnf can't hold a version; adds a bootstrap script or self-update step |
| B. Single writer = developer; CI only reads; mise floats on both sides | Mostly deletions; follows mise's documented model; no version to sync | Lock reflects the author's mise; an older CI mise reads a newer lock (unverified) |
| C. Single writer = CI via autofix.ci | Local is irrelevant | Adds an app and a workflow; every mise release produces bot lock commits |

```
   B                                        C
local hook ──write──▶ mise.lock      CI `mise lock` ──autofix.ci──▶ mise.lock
CI v5 --locked ──read─┘              local ──read──┘
```

## Recommendation

**Option B.** Root cause: two writers and an undeclared mise version. Removing the second writer is simpler than syncing versions, which dnf cannot hold anyway.

1. Run `gx upgrade --latest` to move `jdx/mise-action` to `^5`.
2. Delete the `version:` input and `MISE_VERSION` env from the 3 workflows, the `mise-version` task, and its hook. Delete the `lockfile` job and `lock:check`, since a stale or missing entry now fails `mise install --locked` instead.
3. Change `min_version` to `{ hard = "<oldest that parses the lock>" }`, or drop it.
4. Re-test `locked = true` in CI. If it passes, delete the core:rust comment and memory.
5. Scope the `mise-lockfile` hook to `files: ^(\.config/mise\.toml|rust-toolchain\.toml)$` so a mise upgrade alone doesn't churn the lock.

```
BEFORE: min_version ─sed─▶ 3× MISE_VERSION ─▶ mise-action v3 (writes) ─▶ diff vs local ─▶ FAIL
AFTER:  local mise (latest) ─▶ mise.lock ─▶ mise-action v5 (latest−24h, --locked, reads only)
```

## Open questions

These need CI evidence. Docs and source don't settle them.

- Does mise latest−24h accept a lock written by a newer local mise under `--locked`? Release notes for v4.1.0/v5.0.0 don't say. Test with a draft PR.
- Does `locked = true` (or the action's automatic `--locked`) still trip the core:rust catch-22 on a clean runner? It passes locally on 2026.10.13.

## Test results (2026-10-24)

Environment: isolated `MISE_DATA_DIR`, main's lockfile as written by mise 2026.10.13, run with mise 2026.8.5.

- Plain `mise install`, which is what CI runs today: rewrites prek `linux-x64` from the gnu asset to musl, leaving a dirty tree. This reproduces both the Lockfile and release-plz failures.
- `mise install --locked`, which mise-action v4.1+ runs automatically: leaves the lockfile unchanged and installs the locked gnu asset; `prek 0.4.4` runs. So an older CI mise can read a lock written by a newer local mise.
- `install --locked --dry-run` with the full config, rust included: passes on both 2026.8.5 and 2026.10.13. A dry run may skip the strict rust check, so confirm on a real runner.

## PR #230 review

PR #230 pins CI to whichever mise version last ran the lock hook.

- It keeps all 5 copies of the version and edits 3 workflows from a hook.
- Its hook runs `git add` on mise.toml and the workflows, which stages any unrelated unstaged hunks in those files.
- It raises the hard `min_version` to the newest local mise, so any older mise errors: bots, other machines, or a dnf build that lags upstream.
- Every local mise upgrade produces a commit touching 4 files.
- CI still writes the lockfile.

## Next steps

Run `openspec-propose mise-version-sync`. This is CI/tooling, so per `openspec/config.yaml` it needs no spec.
