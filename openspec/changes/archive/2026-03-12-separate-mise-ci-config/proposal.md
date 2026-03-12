## Why

The CI pipeline fails on macOS runners because `mise-action` installs all tools from `.config/mise.toml`, including `ttyd` which doesn't support `darwin/amd64`. Tools like `ttyd` and `vhs` are only needed locally for demo generation, not in CI.

## What Changes

- Add `.config/mise.ci.toml` containing only CI-relevant tools (cargo-dist, cargo-semver-checks, cargo-deny) and shared settings
- Set `MISE_CONFIG_FILE: .config/mise.ci.toml` at the workflow level in `build.yml` and `release.yml`
- Extract all inline `[tasks]` from `.config/mise.toml` into file tasks under `.config/mise/tasks/` so both configs share them without duplication
- Keep `.config/mise.toml` unchanged except for removing inline tasks (tools and settings stay)

## Capabilities

### New Capabilities

- `ci-mise-config`: Separate mise configuration for CI that excludes local-only tools (vhs, ttyd), using file tasks to avoid task duplication between local and CI configs

### Modified Capabilities

_(none)_

## Impact

- **CI workflows**: `build.yml` and `release.yml` gain a top-level `env.MISE_CONFIG_FILE` pointing to `.config/mise.ci.toml`
- **Local development**: No behavioral change — `.config/mise.toml` still used by default, tasks auto-discovered from `.config/mise/tasks/`
- **New files**: `.config/mise.ci.toml` and individual task scripts in `.config/mise/tasks/`
