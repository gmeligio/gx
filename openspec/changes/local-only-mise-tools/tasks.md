## 1. Verify resolution (decision gate)

- [ ] 1.1 Create a throwaway `.config/mise.local.toml` with `jq` and run `mise config` / `mise ls` to confirm mise discovers a `.local.toml` nested under `.config/`; if it does not resolve, fall back to a root-level `mise.local.toml` and adjust the paths in the tasks below
- [ ] 1.2 Confirm mise writes the local lock to `.config/mise.local.lock` (not `.config/mise.lock`) when the local config is present, then delete the throwaway file

## 2. Split tool config

- [ ] 2.1 Create `.config/mise.local.toml` with `[tools]` `github:charmbracelet/vhs = "0.10.0"` and `ttyd = "1.7.3"`
- [ ] 2.2 Remove `github:charmbracelet/vhs` and `ttyd` from `[tools]` in `.config/mise.toml`
- [ ] 2.3 Add `.config/mise.local.toml` and `.config/mise.local.lock` to `.gitignore`

## 3. Regenerate committed lockfile

- [ ] 3.1 Regenerate `.config/mise.lock` so the `ttyd` and `github:charmbracelet/vhs` entries are removed (run the documented lock flow), and verify the rust/other entries are otherwise unchanged
- [ ] 3.2 Confirm `git status --porcelain .config/mise.lock` is empty after a fresh `mise install` in a checkout where `.config/mise.local.toml` is absent

## 4. Remove per-workflow guards

- [ ] 4.1 Remove the `env: MISE_DISABLE_TOOLS: ttyd,github:charmbracelet/vhs` block from `.github/workflows/build.yml`
- [ ] 4.2 Remove the `env: MISE_DISABLE_TOOLS: ttyd,github:charmbracelet/vhs` block from `.github/workflows/release.yml`
- [ ] 4.3 Confirm `.github/workflows/release-plz.yml` has no `MISE_DISABLE_TOOLS` env (it must not need one)

## 5. Document & verify

- [ ] 5.1 Document the expected gitignored `.config/mise.local.toml` (purpose + copy-paste snippet) in AGENTS.md / contributor docs so a fresh clone knows demo tools live there
- [ ] 5.2 Verify locally: with `.config/mise.local.toml` present, `mise ls` shows `ttyd`/`vhs` and `mise run build` (and the demo task) succeed; with it absent, neither tool resolves
- [ ] 5.3 After merge, watch the `Release-plz` run on `main` (cold cache) to confirm `release-plz release-pr` no longer aborts on a dirty `.config/mise.lock`
