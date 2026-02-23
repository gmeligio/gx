# upgrade command - Implementation

**Diagram:** [System overview](overview-architecture.excalidraw)

## Overview

The `upgrade` command finds newer versions of actions via the Github API, applies upgrades to the manifest, resolves new SHAs, and updates workflows.

## Code path

`src/commands/upgrade.rs`

## Signature

```rust
pub fn run<M: ManifestStore, L: LockStore, R: VersionRegistry, W: WorkflowUpdater>(
    repo_root: &Path,
    mut manifest: Manifest,
    manifest_store: M,
    mut lock: Lock,
    lock_store: L,
    registry: R,
    writer: &W,
    mode: &UpgradeMode,
) -> Result<()>
```

## Algorithm

`run` delegates to two private helpers and applies results:

### `determine_upgrades` — find what to change

Returns `Ok(None)` (nothing to do) or `Ok(Some((upgrades, repins)))`.

**For `Safe`/`Latest` modes:**

If the manifest is empty, returns `Ok(None)` immediately. Otherwise iterates specs:

- Bare SHAs: skipped with a log message.
- Non-semver refs (branches, text tags): collected into `repins` for re-pinning.
- Semver specs: fetches all tags via `registry.all_tags(&spec.id)`, then calls `find_upgrade` (Safe) or `find_latest_upgrade` (Latest) to find a candidate. Returns `Ok(None)` if both `upgrades` and `repins` are empty.

`Version::find_upgrade()` respects precision:
- **Major** (`v4`): upgrades to any higher major (`v5`, `v6`)
- **Minor** (`v4.1`): upgrades within same major (`v4.2`, `v4.3`)
- **Patch** (`v4.1.0`): upgrades within same major.minor (`v4.1.1`, `v4.1.3`)

**For `Targeted` mode:**

Verifies the action is in the manifest and the requested version exists in the registry, then returns a single-element `upgrades` list with an empty `repins`.

### `resolve_and_store` — resolve a spec to SHA and write to lock

```rust
fn resolve_and_store<R: VersionRegistry>(
    service: &ActionResolver<R>,
    spec: &ActionSpec,
    lock: &mut Lock,
    unresolved_msg: &str,
)
```

Calls `service.resolve(spec)` and matches on `ResolutionResult`:
- `Resolved` / `Corrected` → writes the SHA to the lock via `lock.set(&resolved)`.
- `Unresolved` → logs a warning with the provided `unresolved_msg` prefix.

### `run` — orchestrate

1. Calls `determine_upgrades`; returns early if `None`.
2. Applies each upgrade to the manifest (`manifest.set`).
3. Calls `resolve_and_store` for each upgraded spec ("Could not resolve").
4. Calls `resolve_and_store` for each re-pinned spec ("Could not re-pin").
5. Saves manifest and lock; retains only manifest entries in lock.
6. Builds update map from upgraded + re-pinned keys and calls `writer.update_all()`.

## Key types

### UpgradeMode

```rust
pub enum UpgradeMode {
    Safe,                        // default: upgrade within current major
    Latest,                      // upgrade to absolute latest, crossing major versions
    Targeted(ActionId, Version), // upgrade one specific action to a specific version
}
```

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

In `commands/app.rs::upgrade()`, the command branches based on manifest existence:

- **File-backed**: `FileManifest::new(&manifest_path).load()` + `FileLock::new(&lock_path).load()` when `gx.toml` exists
- **Memory-only**: `MemoryManifest::from_workflows(&action_set).load()` + `Lock::default()` when no manifest exists

Entry point is called from `main.rs` line ~57:
```rust
Commands::Upgrade { action, latest } => {
    let mode = resolve_upgrade_mode(action, latest)?;
    commands::app::upgrade(&repo_root, &manifest_path, &lock_path, &mode)
}
```

## Testing

- `print_update_results` with empty and non-empty results
- Integration tests in `tests/upgrade_test.rs`
