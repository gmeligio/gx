# upgrade command - Implementation

## Overview

The `upgrade` command finds newer versions of actions via the Github API, applies upgrades to the manifest, resolves new SHAs, and updates workflows.

## Code path

`src/commands/upgrade.rs`

## Algorithm

### 1. Get specs from manifest

```rust
let specs = manifest.specs();
```

If no specs exist (empty manifest), return immediately.

### 2. Find available upgrades

For each spec, check if the version has semver precision:

```rust
if spec.version.precision().is_none() {
    continue; // Skip non-semver (branches, bare SHAs)
}
```

Fetch all tags for the action's repository via `registry.all_tags(&spec.id)`, then find the highest compatible upgrade:

```rust
if let Some(upgraded) = spec.version.find_upgrade(&tags) {
    upgrades.push(UpgradeCandidate { id, current, upgraded });
}
```

`Version::find_upgrade()` respects precision:
- **Major** (`v4`): upgrades to any higher major (`v5`, `v6`)
- **Minor** (`v4.1`): upgrades within same major (`v4.2`, `v4.3`)
- **Patch** (`v4.1.0`): upgrades within same major.minor (`v4.1.1`, `v4.1.3`)

### 3. Apply upgrades to manifest

```rust
for upgrade in &upgrades {
    manifest.set(upgrade.id.clone(), upgrade.upgraded.clone());
}
```

### 4. Resolve new SHAs

For each upgrade, create an `ActionSpec` with the new version and resolve via `ActionResolver::resolve()`:

```rust
let spec = ActionSpec::new(upgrade.id.clone(), upgrade.upgraded.clone());
let result = service.resolve(&spec);
match result {
    ResolutionResult::Resolved(resolved) => lock.set(&resolved),
    ResolutionResult::Corrected { corrected, .. } => lock.set(&corrected),
    ResolutionResult::Unresolved { spec, reason } => warn!("..."),
}
```

### 5. Save and update workflows

```rust
manifest.save()?;
lock.retain(&keys_to_retain);
lock.save()?;
```

Only upgraded actions are included in the workflow update map (not all specs):

```rust
let upgraded_keys: Vec<LockKey> = upgrades.iter()
    .map(|u| LockKey::new(u.id.clone(), u.upgraded.clone()))
    .collect();
let update_map = lock.build_update_map(&upgraded_keys);
writer.update_all(&update_map)?;
```

## Key types

### UpgradeCandidate

```rust
pub struct UpgradeCandidate {
    pub id: ActionId,
    pub current: Version,
    pub upgraded: Version,
}
```

### VersionPrecision

```rust
pub enum VersionPrecision {
    Major,  // "v4"
    Minor,  // "v4.1"
    Patch,  // "v4.1.0"
}
```

## Two modes

In `main.rs`, the upgrade command branches like tidy:

- **File-backed**: `FileManifest`/`FileLock` when `gx.toml` exists
- **Memory-only**: `MemoryManifest`/`MemoryLock` via `run_memory_only` helper when no manifest exists

## Testing

- `print_update_results` with empty and non-empty results
- `run` with empty manifest returns Ok immediately (no Github API calls)
