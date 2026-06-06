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

- [ ] 3.1 Add `.config/mise/tasks/setup` that runs `prek install`
- [ ] 3.2 Add committed `.claude/settings.json` with a `SessionStart` hook that runs `mise run setup` guarded on `git rev-parse --git-path hooks`/pre-commit (no-op when already installed)
- [ ] 3.3 Add `!.claude/settings.json` to `.gitignore` and verify with `git check-ignore` that it is now tracked
- [ ] 3.4 Run `mise run setup`; confirm `prek install` wrote the hook into this worktree's hooks dir

## 4. Lockfile-maintenance hooks in `.pre-commit-config.yaml`

- [ ] 4.1 Add a cargo lock-sync hook gated on `files: ^Cargo\.toml$` (regenerate `Cargo.lock`)
- [ ] 4.2 Add a `gx tidy` hook gated on `files: ^\.github/(workflows/.*|gx\.toml)$`
- [ ] 4.3 Add a mise hook with `always_run: true`, `pass_filenames: false`, running unlocked `mise install` (NOT `--locked`/`MISE_LOCKED`); emit a one-line note when it regenerates the lock
- [ ] 4.4 Decide the mise hook stage (`pre-commit` vs `pre-push`) and set it

## 5. Verification

- [ ] 5.1 Edit `Cargo.toml` → commit → confirm the cargo hook regenerates `Cargo.lock`, blocks, and a re-stage commits clean
- [ ] 5.2 Edit a workflow → commit → confirm `gx tidy` hook updates `.github/gx.lock` and blocks until re-staged
- [ ] 5.3 Simulate mise drift (revert the `mise.lock` header) → commit → confirm the mise hook regenerates it and blocks
- [ ] 5.4 In a fresh worktree, start a session → confirm `SessionStart` runs `mise run setup` and installs the hook; re-run → confirm it's a no-op
- [ ] 5.5 Confirm `prek run --all-files` (or equivalent) passes on a clean tree
