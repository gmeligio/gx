# Architecture

## Diagrams

| Diagram | Description |
|---------|-------------|
| [Domain module](domain-architecture.excalidraw.json) | All domain types and how they relate |
| [Manifest system](manifest-architecture.excalidraw.json) | `Manifest` domain entity, `ManifestStore` I/O trait, `FileManifest`, `MemoryManifest` |
| [Lock system](lock-architecture.excalidraw.json) | `Lock` domain entity, `LockStore` I/O trait, `FileLock`, `MemoryLock` |

## Layer diagram

```
main.rs (Composition Root)
    │ Checks if gx.toml exists
    │ Creates stores (FileManifest/FileLock or MemoryManifest/MemoryLock)
    │ Loads domain entities (Manifest, Lock) via stores
    ▼
commands/ (Application Layer)
    │ tidy.rs, upgrade.rs
    │ Mutates Manifest + Lock domain entities, calls store.save() when done
    ▼
domain/ (Business Types + Resolution Logic)
    │ action.rs, manifest.rs, lock.rs, resolution.rs, workflow_actions.rs
    │ Pure domain types and algorithms, no I/O
    ▼
infrastructure/ (File I/O + Github API)
    │ manifest.rs, lock.rs, github.rs, workflow.rs, repo.rs
    │ Pure I/O: load() → domain entity, save(entity)
```

## Dependency injection

`main.rs` is the composition root. It checks whether `.github/gx.toml` exists, creates the appropriate stores, loads domain entities, and injects them into commands:

```rust
// File-backed mode: changes persist to disk
let manifest_store = FileManifest::new(&manifest_path);
let manifest = manifest_store.load()?;
let lock_store = FileLock::new(&lock_path);
let lock = lock_store.load()?;
commands::tidy::run(&repo_root, manifest, manifest_store, lock, lock_store, registry, &scanner, &updater)

// Memory-only mode: only workflows are updated
let manifest_store = MemoryManifest::from_workflows(&action_set);
let manifest = manifest_store.load()?;
let lock_store = MemoryLock;
let lock = Lock::default();
commands::tidy::run(&repo_root, manifest, manifest_store, lock, lock_store, registry, &scanner, &updater)
```

Commands accept domain entities and store trait abstractions:

```rust
pub fn run<M: ManifestStore, L: LockStore, R: VersionRegistry, P: WorkflowScanner, W: WorkflowUpdater>(
    repo_root: &Path,
    mut manifest: Manifest,
    manifest_store: M,
    mut lock: Lock,
    lock_store: L,
    registry: R,
    scanner: &P,
    writer: &W,
) -> Result<()>
```

## Domain entities

### Manifest (`domain/manifest.rs`)

Owns the `ActionId → ActionSpec` map and all domain behaviour:
- `get`, `set`, `remove`, `has`, `is_empty`, `specs` — data access and mutation
- `detect_drift(action_set, filter)` — compares manifest against scanned workflow actions; returns `Vec<DriftItem>`

### Lock (`domain/lock.rs`)

Owns the `LockKey → CommitSha` map and all domain behaviour:
- `get`, `set`, `has`, `retain` — data access and mutation
- `build_update_map(keys)` — formats entries as `"SHA # version"` strings for workflow updates

### DriftItem (`domain/manifest.rs`)

Returned by `Manifest::detect_drift`:
- `MissingFromManifest { id }` — action in workflow but not in `gx.toml`
- `MissingFromWorkflow { id }` — action in `gx.toml` but not in any workflow
- `VersionMismatch { id, manifest_version, workflow_version }` — version differs between sides

## Trait abstractions

### ManifestStore (`infrastructure/manifest.rs`)

Pure I/O trait. Implementations:
- `FileManifest` — reads/writes `.github/gx.toml`
- `MemoryManifest` — no-op `save()`, `load()` returns pre-seeded or empty `Manifest`

### LockStore (`infrastructure/lock.rs`)

Pure I/O trait. Implementations:
- `FileLock` — reads/writes `.github/gx.lock`; transparently migrates old format versions on load
- `MemoryLock` — no-op `save()`, `load()` returns empty `Lock`

### VersionRegistry (`domain/resolution.rs`)

Queries available versions and commit SHAs from a remote registry:
- `lookup_sha(id, version)` — resolve a version to a SHA
- `tags_for_sha(id, sha)` — find which tags point to a SHA
- `all_tags(id)` — list all version tags for an action

Implementation: `GithubRegistry` (`infrastructure/github.rs`)

## Domain types

### Type flow

```
Workflow YAML
    ▼ (WorkflowParser extracts raw data)
UsesRef { action_name, uses_ref, comment }
    ▼ (UsesRef::interpret() normalizes)
InterpretedRef { id: ActionId, version: Version, sha: Option<CommitSha> }
    ▼ (aggregated across workflows)
WorkflowActionSet { versions, shas }
    ▼ (manifest sync / drift detection)
Manifest { actions: HashMap<ActionId, ActionSpec> }
    ▼ (resolved via VersionRegistry)
ResolvedAction { id: ActionId, version: Version, sha: CommitSha }
    ▼ (stored in lock)
Lock { actions: HashMap<LockKey, CommitSha> }
```

### Strong types

- `ActionId` — action identifier (e.g., `"actions/checkout"`)
- `Version` — version specifier with semver methods (e.g., `"v4"`, `"v4.1.0"`)
- `CommitSha` — 40-character hex commit hash
- `LockKey` — composite `action@version` key for the lock file

## Error handling

- Domain errors: `ResolutionError` (enum with `thiserror`)
- Infrastructure errors: `ManifestError`, `LockFileError`, `WorkflowError`, `GithubError` (enums with `thiserror`)
- Command level: `anyhow::Result<T>` for propagation

## Adding a new command

1. Create `src/commands/newcmd.rs` with `pub fn run<M: ManifestStore, L: LockStore>(..., manifest: Manifest, manifest_store: M, lock: Lock, lock_store: L, ...) -> Result<()>`
2. Add a variant to `Commands` in `src/main.rs`
3. Add `pub mod newcmd;` to `src/commands.rs`
4. Add the match arm in `main()` with appropriate DI (file-backed vs memory-only)
5. Add user docs in `docs/newcmd.md`
6. Add implementation docs in `docs/development/newcmd.md`
7. Update `CLAUDE.md` file tree and commands section
