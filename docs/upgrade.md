# upgrade command

The `upgrade` command checks for newer versions of actions in your manifest and upgrades them.

## Usage

```bash
gx upgrade
```

## What it does

1. **Reads** all action specs from the manifest (or workflows in memory-only mode)
2. **Checks** each semver-versioned action for newer tags via the Github API
3. **Upgrades** the manifest with the highest compatible version
4. **Resolves** the new versions to commit SHAs
5. **Updates** the lock file and workflows

## Version precision

Upgrades respect the precision of your current version pin:

| Current | Upgrade scope | Example |
|---|---|---|
| `v4` (major) | Any newer major | `v4` → `v5`, `v6` |
| `v4.1` (minor) | Same major, newer minor | `v4.1` → `v4.2`, `v4.3` |
| `v4.1.0` (patch) | Same major.minor, newer patch | `v4.1.0` → `v4.1.1`, `v4.1.3` |

## Re-pinning dynamic refs

Actions that use non-semver references (branches like `main`, text tags like `stable` or `next`) are automatically re-pinned to their current commit SHA on every `gx upgrade` run. This ensures the lock file stays current with the branch HEAD.

The manifest is unchanged (the version stays `main`); only the lock file and workflows are updated with the fresh SHA.

| Reference type | Upgrade behaviour |
|---|---|
| Semver (`v4`, `v4.1`, `v4.1.0`) | Upgraded to newer version within precision constraints |
| Branch/tag (`main`, `stable`) | Re-pinned to current commit SHA |
| Bare SHA (40 hex chars) | Skipped — nothing to resolve |

## Skipped actions

The following are not upgraded or re-pinned:

- Bare commit SHAs without a version comment (no ref to re-resolve)
- Actions where `GITHUB_TOKEN` is missing or the API call fails (logged as warnings)

## Two modes

Like other gx commands, `upgrade` works in two modes:

- **File-backed** (when `gx.toml` exists): Updates `gx.toml`, `gx.lock`, and workflows
- **Memory-only** (no `gx.toml`): Updates workflows only

## Example

Given this manifest:

```toml
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
```

If `actions/checkout` has a `v5` tag available, running `gx upgrade` outputs:

```
Checking for upgrades...
Upgrading actions:
+ actions/checkout v4 -> v5
Updated workflows:
  .github/workflows/ci.yml
  ~ actions/checkout@<new-sha> # v5
1 workflow(s) updated.
```

And updates the manifest:

```toml
[actions]
"actions/checkout" = "v5"
"actions/setup-node" = "v4"
```

When all actions are already at their latest:

```
Checking for upgrades...
All actions are up to date.
```

## Environment

- `GITHUB_TOKEN`: Required for fetching available tags from the Github API.
