## 1. Verify resolution (decision gate) — VERIFIED, no fallback needed

- [x] 1.1 Confirmed mise 2026.6.0 discovers `.config/mise.local.toml` (`mise config` lists it; `mise ls --current --json` includes its tools only when the file is present, excludes them when absent). The `.config/`-nested form works — root-level fallback not required. Note: use `mise ls --current --json` for the active set; plain `mise ls` also lists installed-but-inactive global versions.
- [x] 1.2 Confirmed lockfile separation: local tools write to `.config/mise.local.lock`, never `.config/mise.lock`; a CI-style `mise install` with the local file absent leaves `.config/mise.lock` byte-for-byte unchanged. (mise only updates an existing lockfile — `.config/mise.local.lock` must be `touch`ed to materialize, but it is gitignored so this is harmless.)

## 2. Split tool config

- [x] 2.1 Create `.config/mise.local.toml` with `[tools]` `github:charmbracelet/vhs = "0.10.0"` and `ttyd = "1.7.3"`
- [x] 2.2 Remove `github:charmbracelet/vhs` and `ttyd` from `[tools]` in `.config/mise.toml`
- [x] 2.3 Add `.config/mise.local.toml` and `.config/mise.local.lock` to `.gitignore`

## 3. Regenerate committed lockfile

- [x] 3.1 Regenerate `.config/mise.lock` so the `ttyd` and `github:charmbracelet/vhs` entries are removed (run the documented lock flow), and verify the rust/other entries are otherwise unchanged. Done by moving `.config/mise.local.toml` aside (CI's view) and deleting the now-orphaned `vhs`/`ttyd` tables — pure 58-line deletion, zero additions, rust + 4 other tools intact (mise does not auto-prune stale lock tables).
- [x] 3.2 Confirm `git status --porcelain .config/mise.lock` is empty after a fresh `mise install` in a checkout where `.config/mise.local.toml` is absent. Verified: CI-style `mise install` against the slimmed lock produced zero unstaged churn.

## 4. Remove per-workflow guards

- [x] 4.1 Remove the `env: MISE_DISABLE_TOOLS: ttyd,github:charmbracelet/vhs` block from `.github/workflows/build.yml`
- [x] 4.2 Remove the `env: MISE_DISABLE_TOOLS: ttyd,github:charmbracelet/vhs` block from `.github/workflows/release.yml`
- [x] 4.3 Confirm `.github/workflows/release-plz.yml` has no `MISE_DISABLE_TOOLS` env (it must not need one). Confirmed: `grep MISE_DISABLE` across `.github/workflows/` now returns nothing.

## 5. Document & verify

- [x] 5.1 Document the expected gitignored `.config/mise.local.toml` (purpose + copy-paste snippet) in AGENTS.md / contributor docs so a fresh clone knows demo tools live there. Added a "Generating the demo" section to `docs/CONTRIBUTING.md` (the canonical setup doc) with the rationale and a copy-paste snippet.
- [x] 5.2 Verify locally: with `.config/mise.local.toml` present, `mise ls` shows `ttyd`/`vhs` and `mise run build` (and the demo task) succeed; with it absent, neither tool resolves. Verified via `mise ls --current --json`: ttyd+vhs present in the active set only when the local file exists; absent (CI's view) when it does not. (The `demo` task runs in Docker with a GitHub token, so it is not executed here; tool resolution — what this task gates — is confirmed.)
- [ ] 5.3 After merge, watch the `Release-plz` run on `main` (cold cache) to confirm `release-plz release-pr` no longer aborts on a dirty `.config/mise.lock`. *(Post-merge observation — cannot be exercised pre-merge.)*
