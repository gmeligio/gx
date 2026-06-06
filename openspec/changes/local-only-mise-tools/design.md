## Context

mise installs tools listed in `.config/mise.toml`. With `lockfile = true`, every `mise install` resolves and writes per-platform checksums/URLs into the committed `.config/mise.lock`. `jdx/mise-action` runs `mise install` by default on every CI job.

Two of the tools — `ttyd` and `github:charmbracelet/vhs` — exist only for local demo recordings; CI never needs them. Historically CI avoided installing them via a per-workflow `MISE_DISABLE_TOOLS: ttyd,github:charmbracelet/vhs` env (PR #69, after an earlier two-file split in #66 was reverted for duplicating the tool list). #86 swapped `release-plz.yml` from `setup-rust-toolchain` to `mise-action` but did not carry the env over, so that job installs `ttyd`/`vhs` on a cold runner, regenerates their checksums, and writes them back to `.config/mise.lock`. release-plz then sees a dirty tree and aborts.

The recurring failure mode is that exclusion is a per-workflow opt-in: invisible, unenforced, and forgotten on the next new workflow. This design makes exclusion structural.

Verified during research (mise 2026.6.0):
- mise environment overlays (`mise.{env}.toml`) only merge/override keys — there is **no syntax to remove** a tool declared in the base config. Confirmed by local repro: an empty `mise.ci.toml` under `MISE_ENV=ci` still resolves a base-declared tool. So "exclude in a CI overlay" is impossible.
- Lockfiles are per-config-file: `mise.toml` → `mise.lock`, `mise.local.toml` → `mise.local.lock` ([mise settings docs](https://mise.jdx.dev/configuration/settings.html)). Churn from local-only tools therefore lands in a different file than the committed lock.
- `mise.local.toml` / `mise.*.local.toml` are on the default load path but [intended to be gitignored](https://mise.jdx.dev/configuration/environments.html), so they are absent in a CI checkout.

## Goals / Non-Goals

**Goals:**
- The committed `.config/mise.lock` cannot be dirtied by local-only tools in any CI job — structurally, not by per-workflow discipline.
- Keep a single committed mise config (`.config/mise.toml`) — no duplicated tool/settings list (the duplication that #69 correctly eliminated).
- No per-workflow `MISE_DISABLE_TOOLS` guard anywhere, so a future new workflow cannot reintroduce this regression by omission.
- Local demo generation (`vhs`/`ttyd`) keeps working unchanged for contributors who have the local config.

**Non-Goals:**
- Solving the separate `core:rust` lockfile catch-22 (`.config/mise.toml` `locked = false`) — orthogonal, already documented in-file.
- Reintroducing a committed second config file (`mise.ci.toml`/`mise.test.toml`) — explicitly rejected.
- Changing any `gx` runtime behavior, CLI, or task definitions.

## Decisions

### 1. Local-only tools live in a gitignored `.config/mise.local.toml`

Move `ttyd` and `github:charmbracelet/vhs` from `.config/mise.toml` `[tools]` into `.config/mise.local.toml`, and gitignore both `.config/mise.local.toml` and `.config/mise.local.lock`.

CI checks out the repo without the gitignored file, so mise never discovers `ttyd`/`vhs` there and never installs them — exclusion is a property of the repo contents, not of each workflow's env. Local-tool checksum churn writes to `.config/mise.local.lock` (separate lockfile), never to the committed `.config/mise.lock`.

**Alternatives considered:**
- *`MISE_ENV` overlay to exclude in CI* — impossible; overlays cannot subtract a base-declared tool (verified).
- *Per-tool `os = [...]` filter* — `ttyd` fails on macOS, so `os` was once considered, but `ttyd`/`vhs` still install on Linux CI (where release-plz runs), so it does not stop the churn. `os` semantics are "platform-incompatible", not "local-only".
- *Committed second config (`mise.ci.toml`)* — reintroduces the tool/settings duplication #69 removed.
- *Add `MISE_DISABLE_TOOLS` to `release-plz.yml`* — the one-line patch, but keeps the fragile per-workflow pattern that caused two regressions.

### 2. Remove `MISE_DISABLE_TOOLS` from `build.yml` and `release.yml`

Once the local tools are gitignored-out, the env guard is dead weight; leaving it in invites cargo-culting it (or forgetting it) on new workflows. Remove it from both, and never add it to `release-plz.yml`.

### 3. Regenerate the committed lockfile to drop `ttyd`/`vhs`

After moving the tools, `.config/mise.lock` must no longer contain `ttyd`/`vhs` entries (otherwise `locked`-mode consumers and reviewers see stale state). Regenerate via the documented flow and commit the slimmed lock.

## Risks / Trade-offs

- **`.config/`-nested local file resolution is unverified** → mise docs show `mise.local.toml` and `.config/mise.{env}.toml` patterns but not the literal `.config/mise.local.toml` combination. *Mitigation:* first task verifies `mise config`/`mise ls` discovers `.config/mise.local.toml` before any other change; if it does not resolve under `.config/`, fall back to a root-level `mise.local.toml` (still gitignored, still separate lockfile).
- **Fresh clones lack the local tools** → a contributor running the demo task without `.config/mise.local.toml` gets a missing-tool error. *Mitigation:* document the expected gitignored file (and a copy-paste snippet) in the contributor docs / AGENTS.md; demo generation is a rare, maintainer-side task.
- **Silent re-regression if someone re-adds `ttyd`/`vhs` to `.config/mise.toml`** → the churn returns. *Mitigation:* the slimmed committed lock makes such a re-add show up as lock churn in review; optionally a lint/CI assertion (out of scope here) could enforce it.

## Migration Plan

1. Verify `.config/mise.local.toml` resolves under `.config/` (decision-gating check).
2. Create `.config/mise.local.toml` with `ttyd` + `vhs`; remove them from `.config/mise.toml`.
3. Gitignore `.config/mise.local.toml` and `.config/mise.local.lock`.
4. Regenerate and commit `.config/mise.lock` (now without `ttyd`/`vhs`).
5. Drop `MISE_DISABLE_TOOLS` from `build.yml` and `release.yml`.
6. Document the local config for contributors.

**Rollback:** revert the commit — restores `ttyd`/`vhs` to `.config/mise.toml`, the env guards, and the prior lock. No state outside the repo is touched.

## Automated Test Strategy

This is a CI/config change; verification is workflow-level, not unit-test-level.

- **Critical path:** the `Release-plz PR` job runs `mise install` then `release-plz release-pr` without dirtying `.config/mise.lock`. The authoritative test is the next push to `main` exercising release-plz on a cold cache — the exact condition that failed.
- **Pre-merge local checks:**
  - Confirm `mise install` in a clean checkout (local config absent) produces zero `git diff` on `.config/mise.lock` (`git status --porcelain .config/mise.lock` empty).
  - Confirm `mise ls` shows `ttyd`/`vhs` only when `.config/mise.local.toml` is present, and not when it is absent.
  - Confirm `mise run build` (and any demo task) still resolves locally with the local config present.
- **No new test infrastructure** is warranted for a tooling chore of this size.

## Observability

- **Primary failure surface:** the `Release-plz` workflow run on `main`. A regression re-appears as the same `release-plz` error — *"the working directory of this project has uncommitted changes … [".config/mise.lock"]"* — which is loud and blocks the release PR rather than failing silently.
- **Detectable in review:** any future re-addition of `ttyd`/`vhs` to `.config/mise.toml`, or a stray `.config/mise.lock` change, appears in the PR diff and in CI `git status` output.
- **Local signal:** a contributor missing `.config/mise.local.toml` gets an explicit mise missing-tool error when running the demo task, not a silent skip.
