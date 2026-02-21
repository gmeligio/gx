# Architecture

## Diagrams

| Diagram | Description |
|---------|-------------|
| [Domain module](domain-architecture.excalidraw.json) | All domain types in `action.rs`, `resolution.rs`, `workflow_actions.rs` and how they relate |
| [Manifest system](manifest-architecture.excalidraw.json) | `ManifestStore` trait, `FileManifest`, `MemoryManifest`, and `gx.toml` format |
| [Lock system](lock-architecture.excalidraw.json) | `LockStore` trait, `FileLock`, `MemoryLock`, and `gx.lock` format |

## Layer diagram

```
main.rs (Composition Root)
    │ Checks if gx.toml exists
    │ Injects FileManifest/FileLock or MemoryManifest/MemoryLock
    ▼
commands/ (Application Layer)
    │ tidy.rs, upgrade.rs
    │ Orchestrates domain + infrastructure via trait abstractions
    ▼
domain/ (Business Types + Resolution Logic)
    │ action.rs, resolution.rs, workflow_actions.rs
    │ Pure domain types and algorithms, no I/O
    ▼
infrastructure/ (File I/O + Github API)
    │ manifest.rs, lock.rs, github.rs, workflow.rs, repo.rs
    │ Implements traits defined in domain
```

## Dependency injection

`main.rs` is the composition root. It checks whether `.github/gx.toml` exists and injects the appropriate implementations:

```rust
if manifest_path.exists() {
    // File-backed mode: changes persist to disk
    let manifest = FileManifest::load_or_default(&manifest_path)?;
    let lock = FileLock::load_or_default(&lock_path)?;
    commands::tidy::run(&repo_root, manifest, lock, registry)
} else {
    // Memory-only mode: only workflows are updated
    let manifest = MemoryManifest::from_workflows(&action_set);
    let lock = MemoryLock::default();
    commands::tidy::run(&repo_root, manifest, lock, registry)
}
```

Commands accept trait abstractions, not concrete types:

```rust
pub fn run<M: ManifestStore, L: LockStore, R: VersionRegistry>(
    repo_root: &Path, mut manifest: M, mut lock: L, registry: R,
) -> Result<()>
```

## Trait abstractions

### ManifestStore (`infrastructure/manifest.rs`)

Maps `ActionId` → `Version`. Implementations:
- `FileManifest` — reads/writes `.github/gx.toml`, tracks `dirty: bool`
- `MemoryManifest` — in-memory only, `save()` is a no-op

### LockStore (`infrastructure/lock.rs`)

Maps `LockKey` (action@version) → `CommitSha`. Implementations:
- `FileLock` — reads/writes `.github/gx.lock`, tracks `dirty: bool`
- `MemoryLock` — in-memory only, `save()` is a no-op

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
    ▼ (manifest sync)
ActionSpec { id: ActionId, version: Version }
    ▼ (resolved via VersionRegistry)
ResolvedAction { id: ActionId, version: Version, sha: CommitSha }
    ▼ (stored in lock)
LockKey { id: ActionId, version: Version } → CommitSha
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

1. Create `src/commands/newcmd.rs` with a `pub fn run<M: ManifestStore, L: LockStore>(...) -> Result<()>`
2. Add a variant to the `Commands` enum in `src/main.rs`
3. Add `pub mod newcmd;` to `src/commands.rs`
4. Add the match arm in `main()` with appropriate DI (file-backed vs memory-only)
5. Add user docs in `docs/newcmd.md`
6. Add implementation docs in `docs/development/newcmd.md`
7. Update `CLAUDE.md` file tree and commands section
