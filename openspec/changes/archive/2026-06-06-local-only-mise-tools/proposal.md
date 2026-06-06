## Why

The `Release-plz` job fails with *"the working directory of this project has uncommitted changes … [".config/mise.lock"]"*. On a cold runner, `jdx/mise-action` installs every tool in `.config/mise.toml` — including the local-only demo tools `ttyd` and `vhs` — and writes their freshly-resolved checksums back into the committed `.config/mise.lock`, dirtying the tree so release-plz refuses to compute versions.

`build.yml` and `release.yml` suppress this with a per-workflow `MISE_DISABLE_TOOLS: ttyd,github:charmbracelet/vhs` env, but #86 added `mise-action` to `release-plz.yml` without that guard. The guard is invisible, unenforced, and easy to forget — this is the second regression of this class. The fix is to make CI exclusion structural rather than a per-workflow opt-in.

## What Changes

- Move the local-only demo tools `ttyd` and `github:charmbracelet/vhs` out of `.config/mise.toml` into a new gitignored `.config/mise.local.toml`.
- Add `.config/mise.local.toml` and `.config/mise.local.lock` to `.gitignore`. mise writes a separate lockfile per config file (`mise.local.toml` → `mise.local.lock`), so local-tool checksum churn can never touch the committed `.config/mise.lock`.
- Remove the `MISE_DISABLE_TOOLS` env from `build.yml` and `release.yml`, and do not add it to `release-plz.yml`. Because the local config is gitignored, it is absent in any CI checkout, so CI never sees `ttyd`/`vhs` — no per-workflow guard required.

## Capabilities

### New Capabilities

_(none)_

This change falls under the relevance gate's **"Skip spec — CI/tooling, dependency updates, packaging chores"** clause. It alters how CI provisions developer tooling and has no user-facing behavior: the `gx` binary, its CLI surface, and all existing specs (`action-resolution`, `lockfile-integrity`, `manifest-and-lock`, etc.) are untouched. No new domain concept is introduced and nothing users can do changes.

### Modified Capabilities

_(none)_

## Impact

- **`.config/mise.toml`**: removes `ttyd` and `github:charmbracelet/vhs` from `[tools]`.
- **`.config/mise.local.toml`** (new, gitignored): holds `ttyd` and `vhs` for local demo generation.
- **`.config/mise.lock`**: `ttyd`/`vhs` entries removed (they move to the gitignored `mise.local.lock`).
- **`.gitignore`**: adds `.config/mise.local.toml` and `.config/mise.local.lock`.
- **`.github/workflows/build.yml`, `release.yml`**: drop the `MISE_DISABLE_TOOLS` env block.
- **`.github/workflows/release-plz.yml`**: no longer churns `.config/mise.lock`; the failing job recovers with no per-workflow guard added.
- **Local development**: contributors who generate demos must have `.config/mise.local.toml` present; document this so a fresh clone knows the file is expected and gitignored.
