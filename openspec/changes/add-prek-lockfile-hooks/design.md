## Context

The repo tracks three lockfiles, each with a different drift trigger:

- `Cargo.lock` — drifts when `Cargo.toml` dependencies change. Refresh: `cargo update -w` / any resolving cargo command.
- `.github/gx.lock` (+ `.github/gx.toml`) — drifts when workflows or the manifest change. Refresh: `gx tidy`.
- `.config/mise.lock` — drifts when the **mise binary upgrades** (e.g. the `@generated` header `jdx.dev` → `en.dev`) OR when `.config/mise.toml` tools change. Refresh: unlocked `mise install`. This is the odd one: drift can happen with **zero repo edits**, just a newer mise.

The triggering incident: a mise binary upgrade rewrote `.config/mise.lock`, leaving the tree dirty, which made `release-plz release-pr` abort. The repo already uses the pre-commit framework (`.pre-commit-config.yaml` with `cargo-fmt` / `cargo-clippy`, `language: system`). This checkout is a **git worktree** (`GIT_DIR = …/.git/worktrees/gx`), and pre-commit + worktrees is a known footgun (hooks installed in the wrong dir, pre-commit#808).

Branch state: CI cargo `--locked` checks, the regenerated `mise.lock` header, and the `gx.lock` prune are kept; the earlier CI "Lockfiles" detect job was reverted in favor of this local-hooks approach.

## Goals / Non-Goals

**Goals:**
- Keep every tracked lockfile current on the contributor's machine, before the commit lands.
- Auto-install the hooks in every worktree/checkout with no manual step.
- Use prek (fast, single binary, no Python) via mise, consuming the existing config.
- Catch mise **binary-upgrade** drift, not just config-edit drift.

**Non-Goals:**
- Removing the CI cargo `--locked` checks — they stay as the enforcement backstop.
- Pinning the mise version.
- Revisiting `locked = false` / the `core:rust` catch-22.
- Changing any `gx` CLI behavior.

## Decisions

**Decision 1: prek over pre-commit, installed via mise (`aqua:j178/prek`).**
prek is a drop-in for `.pre-commit-config.yaml`, a single Rust binary (no Python runtime), and is in the mise/aqua registry — consistent with the repo's "all tools via mise" rule (`AGENTS.md`). It also "honors worktree-local `core.hooksPath`", sidestepping the pre-commit worktree bug.
- _Alternative_: stock pre-commit (Python). Rejected — extra runtime dependency, slower, worse worktree story.

**Decision 2: hooks regenerate (mutate), relying on prek's "modify → block → restage".**
prek blocks the commit when a hook changes a file ("Files were modified by this hook"). So a lockfile hook regenerates the lock, fails that commit, and the contributor re-stages — identical UX to the existing `cargo-fmt` hook. No separate verify mode needed.

**Decision 3: the mise hook is `always_run: true` and uses unlocked `mise install`.**
mise drift can be caused by a binary upgrade with no repo edit, so a `files:`-gated hook would miss it. It must run on every commit (or pre-push). It must NOT use `--locked`/`MISE_LOCKED` (cold-runner `core:rust` catch-22). The cargo and gx hooks, by contrast, are file-gated (`Cargo.toml`; `.github/workflows/**` + `gx.toml`) because their drift is always caused by a tracked edit.

**Decision 4: per-worktree bootstrap via `mise run setup` + committed `.claude/settings.json`.**
A `setup` mise task runs `prek install` (one source of truth, runnable by humans outside Claude). A committed `SessionStart` hook calls it, guarded on the **worktree's** hook path via `git rev-parse --git-path hooks` so it is correct in worktrees and a no-op once installed.
- _Alternative_: inline bash in settings.json (dentalex style). Rejected — prek isn't provisioned yet; provisioning belongs in mise, and a task is reusable outside Claude.

**Decision 5: keep CI cargo `--locked` as backstop.**
Local hooks are opt-in (bypassable with `-n`, absent on un-bootstrapped clones). CI is the only guarantee a clean lock reaches release. Local + CI = convenience + enforcement.

## Risks / Trade-offs

- **`always_run` mise hook adds latency to every commit** → mise install is near-instant when tools are present; if friction is felt, move that hook to the `pre-push` stage (drift only matters before it leaves the machine).
- **Hooks only help if installed** → the `SessionStart` bootstrap + `mise run setup` close this for Claude sessions and manual setup; CI backstops the rest.
- **prek language parity is not 100%** → mitigated by using `language: system` hooks (plain shell-out), which prek fully supports.
- **`.claude/settings.json` could be ignored by `.gitignore`** → explicit allow-list entry required, verified by `git check-ignore`.

## Automated Test Strategy

Verification is behavioral, exercised locally:
- **Critical path**: with hooks installed, (a) edit `Cargo.toml` → commit → cargo lock-sync hook regenerates `Cargo.lock`, commit is blocked, re-stage succeeds; (b) edit a workflow → `gx tidy` hook updates `gx.lock`; (c) simulate a mise drift (revert the `mise.lock` header) → `mise install` hook regenerates it on the next commit and blocks.
- **Bootstrap test**: in a fresh worktree, start a session → `SessionStart` runs `mise run setup` → `prek install` writes the hook into that worktree's hooks dir; confirm with `git rev-parse --git-path hooks`.
- **Backstop test**: CI cargo `--locked` still fails a PR with a deliberately stale `Cargo.lock` (already covered by the retained CI checks).
- No new test framework; prek + git are the harness.

## Observability

- **Local**: prek prints which hook ran and "Files were modified by this hook" when a lock is regenerated, and blocks the commit — the failure is loud and self-explanatory, not silent.
- **mise hook**: should emit a one-line note when it regenerates the lock so the contributor understands why the commit was blocked (binary upgrade vs config change).
- **Bootstrap**: the `SessionStart` command echoes "prek hooks installed" / "already installed" so session start makes its action visible (mirrors the dentalex pnpm pattern).
- **CI backstop**: any lock that still slips through fails the corresponding `--locked` job with cargo's own descriptive error. No log-and-continue path.

## Migration Plan

1. Add `aqua:j178/prek` to `.config/mise.toml`; `mise install`.
2. Add `.config/mise/tasks/setup` running `prek install`.
3. Add lockfile hooks to `.pre-commit-config.yaml` (cargo lock-sync, `gx tidy`, unlocked `mise install`).
4. Add committed `.claude/settings.json` `SessionStart` bootstrap + `.gitignore` allow-list.
5. Run `mise run setup`; exercise the critical-path tests above.
- **Rollback**: remove the hooks/tool/task/settings; additive, no runtime or data risk. CI backstop is unaffected.

## Open Questions

- **Resolved:** the `always_run` mise hook runs at **pre-push** (one `mise install` per push, not per commit) — drift still can't leave the machine, without per-commit latency. `default_stages: [pre-commit]` keeps the cargo/gx hooks at commit time; `prek install` installs both the pre-commit and pre-push shims.
