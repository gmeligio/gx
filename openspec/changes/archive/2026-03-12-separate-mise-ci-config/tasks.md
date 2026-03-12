## 1. Extract file tasks

- [x] 1.1 Create `.config/mise/tasks/` directory
- [x] 1.2 Extract each `[tasks.*]` entry from `.config/mise.toml` into an executable script in `.config/mise/tasks/` with `#MISE` comment headers for metadata (description, depends, shell)
- [x] 1.3 Handle the namespaced `lint:size` task using a `lint/` subdirectory (`.config/mise/tasks/lint/size`)
- [x] 1.4 Remove all `[tasks.*]` sections from `.config/mise.toml`
- [x] 1.5 Verify `mise tasks` still lists all tasks locally

## 2. Create CI config

- [x] 2.1 Create `.config/mise.ci.toml` with CI-only tools (cargo-dist, cargo-semver-checks, cargo-deny) and shared settings (github_attestations, slsa)
- [x] 2.2 Verify `.config/mise.ci.toml` does NOT include vhs or ttyd

## 3. Update CI workflows

- [x] 3.1 Add `MISE_CONFIG_FILE: .config/mise.ci.toml` to the top-level `env` block in `.github/workflows/build.yml`
- [x] 3.2 Add `MISE_CONFIG_FILE: .config/mise.ci.toml` to the top-level `env` block in `.github/workflows/release.yml`

## 4. Verification

- [x] 4.1 Run `mise run build` locally to confirm file tasks work
- [x] 4.2 Run `mise tasks` with `MISE_CONFIG_FILE=.config/mise.ci.toml` to confirm CI config discovers file tasks
