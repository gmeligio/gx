## Why

`gx upgrade --latest` fabricates version tags that may not exist. When the best semver candidate is `v3.0.0-beta.2`, the code extracts the major component and constructs `"v3"` — a tag that was never in the candidate list. Resolution then fails with 422 because GitHub has no such ref.

The root cause is that `find_upgrade` and `find_latest_upgrade` reformat their output to match the input's precision (Major/Minor/Patch), constructing synthetic tag names instead of returning actual tags from the candidate list.

Additionally, the lock file has no way to record the precise resolved version behind a floating tag (`v4` → `v4.2.1`), and upgrades cannot detect when a candidate resolves to the same SHA already locked.

## What Changes

- **Replace `find_upgrade` + `find_latest_upgrade` with a single `find_upgrade_candidate` function** that returns actual tag names from the candidate list. Never fabricates tags.
- **Add `version` and `specifier` fields to lock entries** (lock v1.3). `version` stores the most specific resolved version (via SHA matching against all tags). `specifier` stores the semver range derived from the manifest version's precision (`^4`, `^4.2`, `~4.1.0`).
- **Use lock version as upgrade floor.** Candidates must be strictly greater than both the manifest version and the lock's resolved version, eliminating false-positive upgrades where the candidate resolves to the same SHA.
- **Tidy fetches full tag list** per action to populate the lock's `version` field via SHA matching.

## Capabilities

### New Capabilities

- `version-resolution`: Defines how versions are interpreted as semver ranges, how upgrade candidates are selected, and how the lock stores resolved versions.

### Modified Capabilities

- `lock-format`: Add `version` and `specifier` fields to lock entries. Bump lock version to 1.3.

## Impact

- `crates/gx-lib/src/domain/action.rs` — replace `find_upgrade`, `find_latest_upgrade`, remove `VersionPrecision` reformatting from upgrade logic
- `crates/gx-lib/src/commands/upgrade.rs` — update `determine_upgrades` to pass lock version as floor
- `crates/gx-lib/src/commands/tidy.rs` — fetch tag list to resolve precise version for lock entries
- `crates/gx-lib/src/infrastructure/github.rs` — add method to find most specific tag matching a SHA
- `crates/gx-lib/src/infrastructure/lock_file.rs` — add `version` and `specifier` fields to `ActionEntry`, bump lock version
- Test files: update assertions for new function signatures and lock format
