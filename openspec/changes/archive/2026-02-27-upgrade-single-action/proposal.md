## Why

`gx upgrade` currently operates on all actions at once — there is no way to target a single action. Users who want to upgrade only one action (e.g., responding to a security advisory, or testing a new major version in isolation) have no supported path.

## What Changes

- The `action` CLI argument for `gx upgrade` now accepts either `ACTION@VERSION` (pin to exact version) or bare `ACTION` (upgrade this action using the specified mode)
- `--latest` no longer conflicts with the `action` argument — `gx upgrade --latest actions/checkout` becomes valid
- `--latest actions/checkout@version` is explicitly rejected (combining latest mode with an exact pin is incoherent)
- **BREAKING**: `UpgradeMode` enum is replaced by `UpgradeMode` + `UpgradeScope` + `UpgradeRequest` types, separating mode (how to upgrade) from scope (which actions)
- `UpgradeMode::Targeted(ActionId, Version)` is removed; replaced by `UpgradeMode::Pinned(Version)` with `UpgradeScope::Single(ActionId)`
- Safe+Single and Latest+Single now both supported

## Capabilities

### New Capabilities

- `upgrade-scope`: Scoped upgrade — ability to target a single action by name for any upgrade mode (Safe or Latest), leaving all other actions untouched

### Modified Capabilities

<!-- none -->

## Impact

- `src/main.rs`: CLI argument parsing and `resolve_upgrade_mode` function
- `src/commands/upgrade.rs`: `UpgradeMode` enum, `determine_upgrades` function, `run` function signature
- `tests/upgrade_test.rs`: Tests using `UpgradeMode::Safe`, `UpgradeMode::Latest`, `UpgradeMode::Targeted`
- No changes to manifest, lock, or workflow update logic
