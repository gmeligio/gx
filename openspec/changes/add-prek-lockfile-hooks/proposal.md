## Why

The `release-plz` pipeline failed on a push to `main` because `.config/mise.lock` was dirty on the runner: release-plz aborts `release-pr` whenever the working tree has uncommitted changes. The lockfile drifted because mise floats to its latest version (intentional — we want new features), and a recent mise release changed a value baked into the lockfile's `@generated` header (`mise.jdx.dev` → `mise.en.dev`); with `locked = false` in `.config/mise.toml`, `mise install` silently rewrote the lockfile in place. The same class of silent drift can affect any tracked lockfile. The cheapest, earliest place to fix it is **on the contributor's machine before the commit lands** — so the lock is already current by the time it reaches a PR or the release pipeline.

## What Changes

- Adopt **prek** (a fast, single-binary, Rust drop-in for the pre-commit framework) to run git hooks locally, installed and pinned via mise. The repo already has a `.pre-commit-config.yaml`; prek consumes it unchanged.
- Add lockfile-maintenance hooks to `.pre-commit-config.yaml` that **regenerate** each lockfile when its inputs change; prek blocks the commit when a hook modifies a file, so the contributor re-stages the refreshed lock — the lock is current before it leaves the machine:
  - `Cargo.lock` — regenerate when `Cargo.toml` changes.
  - `.github/gx.lock` / `.github/gx.toml` — run `gx tidy` when workflows or the manifest change (dogfooding — `gx` is this project).
  - `.config/mise.lock` — run the unlocked `mise install` (never `--locked`/`MISE_LOCKED`, which hits the `core:rust` catch-22) so a mise **binary upgrade** that rewrites the lock is caught even on commits that touch no config.
- **Bootstrap prek per worktree**: a `mise run setup` task runs `prek install`, and a committed `.claude/settings.json` `SessionStart` hook runs it automatically (guarded, worktree-aware) so every checkout/worktree gets the hook installed without manual steps. Requires a `.gitignore` allow-list entry so `.claude/settings.json` is tracked.
- Keep the existing CI cargo `--locked` checks (already on the branch) as the enforcement backstop for commits made without the local hooks (bypass / un-bootstrapped clone). This change is the **local** layer; CI remains the guarantee.
- Keep mise unpinned — this makes drift safe to follow locally, it does not freeze the version.

## Capabilities

### New Capabilities

- `lockfile-integrity`: a project-direction guarantee that every tracked lockfile is kept current by local git hooks before a commit lands, with CI as the enforcement backstop. Traceable user value (cf. `homebrew-release`): maintainers and release consumers are protected from releases that silently break on lockfile drift, and contributors get fast local feedback instead of a failed PR. This does not add, remove, or modify any `gx` CLI command, flag, or output — `gx tidy` is used as an existing capability, not introduced here.

### Modified Capabilities

_None._ No `gx` requirement or spec-level behavior changes.

## Impact

- `.config/mise.toml` — add `aqua:j178/prek` to `[tools]` (pinned).
- `.config/mise/tasks/setup` (new) — runs `prek install` for the current worktree.
- `.pre-commit-config.yaml` — add lockfile-maintenance hooks (cargo lock-sync, `gx tidy`, unlocked `mise install`) alongside the existing `cargo-fmt` / `cargo-clippy` hooks.
- `.claude/settings.json` (new, committed) — `SessionStart` hook that runs `mise run setup` when the worktree's pre-commit hook is missing.
- `.gitignore` — allow-list `.claude/settings.json` (the `.claude/*` rule otherwise ignores it).
- No application code, dependencies, or public API affected. No change to `release-plz.yml`. The CI cargo `--locked` checks already on the branch are retained.
