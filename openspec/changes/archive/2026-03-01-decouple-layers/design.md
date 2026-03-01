# Design: Decouple Layers

## Target Dependency Graph

```
main.rs
   │
   ├──▶ config.rs (ConfigError — no upward dependency)
   │
   ▼
commands/ ─────▶ domain/ ◀───── infrastructure/
   │                                │
   └──▶ infrastructure/             │
        (only from app.rs           │
         dispatcher for saves)      │
```

Rules enforced after this change:
- `config.rs` imports only from `domain/` and `infrastructure/`
- `commands/tidy.rs`, `commands/upgrade.rs`, `commands/lint/` import only from `domain/` and `config`
- `commands/app.rs` is the only command file that imports from `infrastructure/` (it's the dispatcher)
- `domain/` never imports from `commands/` or `infrastructure/`

## Error Architecture

### Before

```
GxError
  ├── App(AppError)
  │     ├── Manifest(ManifestError)        ← infra
  │     ├── Lock(LockFileError)            ← infra
  │     ├── Workflow(WorkflowError)        ← domain (but I/O-flavored)
  │     ├── Github(GithubError)            ← infra
  │     ├── Tidy(TidyError)
  │     │     ├── Manifest(ManifestError)  ← DEAD
  │     │     ├── Lock(LockFileError)      ← DEAD
  │     │     └── Workflow(WorkflowError)
  │     ├── Upgrade(UpgradeError)
  │     │     ├── Manifest(ManifestError)  ← DEAD
  │     │     ├── Lock(LockFileError)      ← DEAD
  │     │     └── Workflow(WorkflowError)
  │     └── Lint(LintError)
  │           └── Workflow(WorkflowError)
  └── Repo(RepoError)

config::Config::load() → AppError         ← CYCLE
```

### After

```
GxError
  ├── Config(ConfigError)                  ← NEW, breaks cycle
  ├── App(AppError)
  │     ├── Manifest(ManifestError)        ← infra (saves only)
  │     ├── Lock(LockFileError)            ← infra (saves only)
  │     ├── Workflow(WorkflowError)        ← domain (semantic)
  │     ├── Github(GithubError)            ← infra
  │     ├── Tidy(TidyError)
  │     │     └── ResolutionFailed         ← only reachable variant
  │     ├── Upgrade(UpgradeError)
  │     │     ├── PinnedRequiresSingleScope
  │     │     ├── ActionNotInManifest
  │     │     ├── TagNotFound
  │     │     └── TagFetchFailed
  │     └── Lint(LintError)
  │           └── ViolationsFound
  └── Repo(RepoError)

config::Config::load() → ConfigError      ← NO CYCLE
```

## WorkflowError Redesign (Option 3: Domain-Semantic)

### Domain layer (semantic errors)

```rust
// domain/workflow.rs
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("failed to scan workflows: {reason}")]
    ScanFailed { reason: String },

    #[error("failed to parse workflow {path}: {reason}")]
    ParseFailed { path: String, reason: String },

    #[error("failed to update workflow {path}: {reason}")]
    UpdateFailed { path: String, reason: String },
}
```

### Infrastructure layer (I/O errors, internal only)

```rust
// infrastructure/workflow.rs — not pub, internal to this module
#[derive(Debug, Error)]
enum IoWorkflowError {
    #[error("glob pattern error")]
    Glob(#[from] glob::PatternError),
    #[error("read error: {}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("YAML parse error: {}", path.display())]
    Parse { path: PathBuf, source: Box<serde_saphyr::Error> },
    #[error("write error: {}", path.display())]
    Write { path: PathBuf, source: std::io::Error },
    #[error("regex error")]
    Regex(#[from] regex::Error),
}
```

Infrastructure methods use `IoWorkflowError` internally and convert to `WorkflowError` at the trait boundary via `From` impl.

## action.rs Split

```
domain/action/
  mod.rs          ← pub use re-exports (preserves all current public API paths)
  identity.rs     ← ActionId, Version, CommitSha, VersionPrecision
  uses_ref.rs     ← UsesRef, InterpretedRef, RefType
  spec.rs         ← ActionSpec, LockKey
  resolved.rs     ← ResolvedAction, VersionCorrection
  upgrade.rs      ← UpgradeAction, UpgradeCandidate, find_upgrade_candidate
```

`domain/mod.rs` re-exports remain unchanged — all external `use crate::domain::ActionId` paths continue to work.

## tidy::run() Decomposition

```rust
pub fn run<R, P, W>(...) -> Result<(Manifest, Lock), TidyError> {
    // Phase 1: Scan
    let located = scanner.scan_all_located()?;
    if located.is_empty() { return Ok((manifest, lock)); }
    let action_set = WorkflowActionSet::from_located(&located);

    // Phase 2: Sync manifest
    sync_manifest_actions(&mut manifest, &action_set, &located, &registry);
    upgrade_sha_versions_to_tags(&mut manifest, &registry);

    // Phase 3: Sync overrides
    sync_overrides(&mut manifest, &located, &action_set);
    prune_stale_overrides(&mut manifest, &located);

    // Phase 4: Resolve lock
    let corrections = update_lock(&mut lock, &mut manifest, &registry)?;
    lock.retain(&build_keys_to_retain(&manifest));

    // Phase 5: Update workflows
    let results = update_workflow_files(&located, &manifest, &lock, scanner, updater)?;
    print_update_results(&results);
    print_corrections(&corrections);

    Ok((manifest, lock))
}
```

New extracted functions:
- `sync_manifest_actions()` — remove unused, add missing (currently inline ~60 lines)
- `upgrade_sha_versions_to_tags()` — upgrade bare SHA manifest entries (currently inline ~40 lines)
- `update_workflow_files()` — build per-file maps and call updater (currently inline ~30 lines)
- `print_corrections()` — log version corrections (currently inline ~6 lines)

Existing extracted functions that remain as-is:
- `sync_overrides()`, `prune_stale_overrides()`, `update_lock()`, `populate_lock_entry()`, `build_keys_to_retain()`, `build_file_update_map()`
