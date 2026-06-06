## 1. Carried over from the CI approach (already on the branch)

- [x] 1.1 Regenerate `.config/mise.lock` (`en.dev` header) and commit to unblock release-plz
- [x] 1.2 Add `--locked` to all CI cargo invocations (check, clippy, test, integ, e2e, deny) — kept as the enforcement backstop
- [x] 1.3 Prune the stale `actions-rust-lang/rustfmt` entry from `.github/gx.toml` and `.github/gx.lock` via `gx tidy`
- [x] 1.4 Revert the CI "Lockfiles" detect job (pivoting to local hooks)

## 2. Install prek via mise

- [x] 2.1 Add `aqua:j178/prek` (pinned) to `[tools]` in `.config/mise.toml`
- [x] 2.2 Run `mise install` and confirm `prek --version` resolves via the mise shim (prek 0.4.4)
- [x] 2.3 Confirm `mise.lock` gains a pinned, checksummed `prek` entry (then commit it)

## 3. Per-worktree bootstrap

- [x] 3.1 Add `.config/mise/tasks/setup` that runs `prek install`
- [x] 3.2 Add committed `.claude/settings.json` with a `SessionStart` hook that runs `mise run setup` guarded on `git rev-parse --git-path hooks`/pre-commit (no-op when already installed)
- [x] 3.3 Add `!.claude/settings.json` to `.gitignore` and verify with `git check-ignore` that it is now tracked
- [x] 3.4 Run `mise run setup`; confirm `prek install` wrote the hook (at the shared `.git/hooks`, which `git rev-parse --git-path hooks` also resolves to, so the guard aligns and worktrees work)

## 4. Lockfile-maintenance hooks in `.pre-commit-config.yaml`

- [x] 4.1 Add a cargo lock-sync hook gated on `files: ^Cargo\.toml$` (uses `cargo metadata` — re-resolves and rewrites `Cargo.lock` without upgrading versions)
- [x] 4.2 Add a `gx tidy` hook gated on `files: ^\.github/(workflows/.*|gx\.toml)$`
- [x] 4.3 Add a mise hook with `always_run: true`, `pass_filenames: false`, running unlocked `mise install` (NOT `--locked`/`MISE_LOCKED`)
- [x] 4.4 mise hook stage = **pre-push** (per decision); set `default_stages: [pre-commit]` so commit hooks run once at commit and the mise hook runs once at push; `setup` installs both shims (`prek install -t pre-commit -t pre-push`)

## 5. Verification

- [x] 5.1 Cargo hook: removed a `[[package]]` block from `Cargo.lock` (stale-but-valid) → `cargo metadata` regenerated it cleanly (exit 0); lock changed ⇒ prek blocks. Also confirmed a corrupt/staged stale lock makes the hook error and block the commit.
- [x] 5.2 gx hook: dropped an action block from `.github/gx.lock` → `gx tidy` restored it from the workflows; lock changed ⇒ prek blocks.
- [x] 5.3 mise hook: reverted the `mise.lock` header to `jdx.dev` (the original drift) → unlocked `mise install` regenerated it to `en.dev`; lock changed ⇒ pre-push hook blocks.
- [x] 5.4 Fresh throwaway worktree, hooks removed, config untrusted → hardened `SessionStart` (`mise trust` then `mise run setup`) installed both pre-commit + pre-push shims; re-run is a no-op. **Discovery: a fresh worktree is untrusted, so the bootstrap must run `mise trust` first** — added to the SessionStart command.
- [x] 5.5 `prek run --all-files --hook-stage pre-commit` passes on a clean tree (all 4 commit hooks); per-stage separation verified (mise only at pre-push).
