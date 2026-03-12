## Context

The project uses `.config/mise.toml` for both tool installation and task definitions. CI runs `mise-action` which installs all tools, including `vhs` and `ttyd` that are only needed locally for demo generation. `ttyd` doesn't support `darwin/amd64`, causing the release workflow to fail on macOS runners.

Currently, all 13 tasks are defined inline as `[tasks.*]` in `mise.toml`, tightly coupling task definitions to the tool config.

## Goals / Non-Goals

**Goals:**
- CI uses a dedicated config that excludes local-only tools (vhs, ttyd)
- Task definitions are shared between local and CI configs without duplication
- Local development workflow is unchanged

**Non-Goals:**
- Eliminating tools/settings duplication between the two config files (accepted trade-off)
- Changing any task behavior or adding new tasks

## Decisions

### 1. Separate CI config file at `.config/mise.ci.toml`

Create a CI-specific config containing only CI-relevant tools (cargo-dist, cargo-semver-checks, cargo-deny) and shared settings.

**Alternative considered**: `os` filter on tools — only solves platform incompatibility, not "local-only" semantics. If a tool later supports all platforms but is still local-only, the problem returns.

### 2. `MISE_CONFIG_FILE` env var at workflow level

Set `MISE_CONFIG_FILE: .config/mise.ci.toml` as a top-level `env` in `build.yml` and `release.yml`. This affects both `mise install` (via mise-action) and subsequent `mise run` commands.

**Alternative considered**: `mise-action` `install_args` — only affects installation, not task execution. Would need additional env var for `mise run` steps anyway.

### 3. Extract inline tasks to file tasks in `.config/mise/tasks/`

Move all `[tasks.*]` entries from `mise.toml` into executable scripts under `.config/mise/tasks/`. Mise auto-discovers file tasks from this directory regardless of which config file is active.

Task metadata (description, depends, shell) is embedded via `#MISE` comment headers in each script file.

**Alternative considered**: Duplicate tasks in both TOML files — violates DRY, maintenance burden increases with every task change.

## Risks / Trade-offs

- **Tools/settings duplication** → Accepted. Only ~8 lines overlap, changes infrequently. Adding a new CI tool requires updating both files.
- **File tasks must be executable** → On Windows (where the maintainer works), git may not preserve execute bits. Mitigated by using `#!/usr/bin/env bash` shebangs and ensuring git tracks the executable permission.
- **Task naming** → File tasks use filename as task name. Namespaced tasks (e.g., `lint:size`) use either a subdirectory (`lint/size`) or a colon in the filename (`lint_size` mapped via config). Need to verify colon-in-filename support or use subdirectories.
