# Design: Lazy Pipeline Architecture

## Architecture Overview

```
    BEFORE: Load All → Mutate All → Serialize All → Write All

    ┌──────────┐     ┌──────────┐     ┌────────────┐     ┌───────────┐
    │  Config  │────▶│ Command  │────▶│ Serialize  │────▶│ fs::write │
    │ parse ALL│     │ mutate   │     │ ALL entries │     │  (full)   │
    └──────────┘     └──────────┘     └────────────┘     └───────────┘

    AFTER: Observe → Plan → Apply (only what changed)

    ┌──────────┐     ┌──────────┐     ┌────────────┐     ┌───────────┐
    │ Sources  │────▶│ Command  │────▶│   Plan     │────▶│ toml_edit │
    │ (lazy)   │     │ borrow,  │     │ (diff only)│     │ (surgical)│
    └──────────┘     │ read-only│     └────────────┘     └───────────┘
                     └──────────┘
```

## Layer Changes

### Domain Layer — Iterator Accessors

Domain types return iterators instead of `Vec`:

```rust
// Before
impl Manifest {
    pub fn specs(&self) -> Vec<&ActionSpec> {
        self.actions.values().collect()
    }
}

impl WorkflowActionSet {
    pub fn action_ids(&self) -> Vec<ActionId> {
        self.versions.keys().cloned().collect()
    }
}

// After
impl Manifest {
    pub fn specs(&self) -> impl Iterator<Item = &ActionSpec> {
        self.actions.values()
    }
}

impl WorkflowActionSet {
    pub fn action_ids(&self) -> impl Iterator<Item = &ActionId> {
        self.versions.keys()
    }
}
```

Consumers that need a `HashSet` collect directly:
```rust
// Before: Vec → into_iter → collect::<HashSet>
let ids: HashSet<ActionId> = action_set.action_ids().into_iter().collect();

// After: iter → collect::<HashSet> (no intermediate Vec)
let ids: HashSet<&ActionId> = action_set.action_ids().collect();
```

### Domain Layer — Scanner Trait

```rust
// Before
pub trait WorkflowScanner {
    fn scan_all_located(&self) -> Result<Vec<LocatedAction>, WorkflowError>;
    fn find_workflow_paths(&self) -> Result<Vec<PathBuf>, WorkflowError>;
}

// After
pub trait WorkflowScanner {
    fn scan(&self) -> Box<dyn Iterator<Item = Result<LocatedAction, WorkflowError>> + '_>;
    fn scan_paths(&self) -> Box<dyn Iterator<Item = Result<PathBuf, WorkflowError>> + '_>;
}
```

Notes on the scanner:
- Returns `Box<dyn Iterator>` because the trait is object-safe and used behind `&dyn`
- Each item is `Result` — errors are per-file, not all-or-nothing
- The caller decides whether to collect, filter, or stop early
- `scan_all_located()` remains as a convenience that collects (used by init)

### Domain Layer — Plan Types

New types representing what commands want to change:

```rust
/// Changes to the manifest file.
pub struct ManifestDiff {
    pub added: Vec<(ActionId, Version)>,
    pub removed: Vec<ActionId>,
    pub overrides_added: Vec<(ActionId, ActionOverride)>,
    pub overrides_removed: Vec<(ActionId, Vec<ActionOverride>)>,
}

/// Changes to the lock file.
pub struct LockDiff {
    pub added: Vec<(LockKey, LockEntry)>,
    pub removed: Vec<LockKey>,
    pub updated: Vec<(LockKey, LockEntryPatch)>,
}

/// What to update per lock entry (only changed fields).
pub struct LockEntryPatch {
    pub version: Option<Option<String>>,
    pub specifier: Option<Option<String>>,
}

/// Changes to workflow files.
pub struct WorkflowPatch {
    pub path: PathBuf,
    pub pins: Vec<(ActionId, String)>,
}

/// Complete plan for tidy.
pub struct TidyPlan {
    pub manifest: ManifestDiff,
    pub lock: LockDiff,
    pub workflows: Vec<WorkflowPatch>,
    pub corrections: Vec<VersionCorrection>,
}

impl TidyPlan {
    pub fn is_empty(&self) -> bool {
        self.manifest.is_empty()
            && self.lock.is_empty()
            && self.workflows.is_empty()
    }
}

/// Complete plan for upgrade.
pub struct UpgradePlan {
    pub manifest: ManifestDiff,
    pub lock: LockDiff,
    pub workflows: Vec<WorkflowPatch>,
    pub upgrades: Vec<UpgradeCandidate>,
}
```

### Command Layer — Plan Functions

Commands become pure functions that produce plans:

```rust
// Before: takes ownership, mutates, returns full state
pub fn run<R, P, W>(
    mut manifest: Manifest,
    mut lock: Lock,
    registry: R,
    parser: &P,
    writer: &W,
) -> Result<(Manifest, Lock), TidyError>

// After: borrows read-only, returns plan
pub fn plan<R, P>(
    manifest: &Manifest,
    lock: &Lock,
    registry: &R,
    scanner: &P,
) -> Result<TidyPlan, TidyError>
```

The plan function:
1. Scans workflows via iterator, collects into `WorkflowActionSet`
2. Diffs action set against manifest to find added/removed actions
3. Diffs overrides to find stale/new overrides
4. Resolves only new/incomplete lock entries (network calls)
5. Computes workflow pins for affected files
6. Returns `TidyPlan` — no mutations anywhere

### Infrastructure Layer — Apply via toml_edit

New apply functions that patch files surgically:

```rust
pub fn apply_manifest_diff(path: &Path, diff: &ManifestDiff) -> Result<(), ManifestError> {
    if diff.is_empty() {
        return Ok(());
    }

    let content = fs::read_to_string(path)?;
    let mut doc = content.parse::<toml_edit::DocumentMut>()?;
    let actions = doc["actions"].as_table_mut()?;

    for (id, version) in &diff.added {
        actions.insert(id.as_str(), toml_edit::value(version.as_str()));
    }
    for id in &diff.removed {
        actions.remove(id.as_str());
    }
    // Handle override additions/removals similarly on doc["actions"]["overrides"]

    fs::write(path, doc.to_string())?;
    Ok(())
}

pub fn apply_lock_diff(path: &Path, diff: &LockDiff) -> Result<(), LockFileError> {
    if diff.is_empty() {
        return Ok(());
    }

    let content = fs::read_to_string(path)?;
    let mut doc = content.parse::<toml_edit::DocumentMut>()?;
    let actions = doc["actions"].as_table_mut()?;

    for (key, entry) in &diff.added {
        actions.insert(&key.to_string(), format_lock_inline_table(entry));
    }
    for key in &diff.removed {
        actions.remove(&key.to_string());
    }
    for (key, patch) in &diff.updated {
        // Patch individual fields in the existing inline table
    }

    fs::write(path, doc.to_string())?;
    Ok(())
}
```

### Dispatcher Layer — Wire Plan to Apply

```rust
// Before
pub fn tidy(repo_root: &Path, config: Config) -> Result<(), AppError> {
    let (manifest, lock) = tidy::run(config.manifest, config.lock, registry, &scanner, &updater)?;
    if has_manifest {
        FileManifest::new(&config.manifest_path).save(&manifest)?;
        FileLock::new(&config.lock_path).save(&lock)?;
    }
}

// After
pub fn tidy(repo_root: &Path, config: Config) -> Result<(), AppError> {
    let plan = tidy::plan(&config.manifest, &config.lock, &registry, &scanner)?;
    if !plan.is_empty() && has_manifest {
        apply_manifest_diff(&config.manifest_path, &plan.manifest)?;
        apply_lock_diff(&config.lock_path, &plan.lock)?;
        apply_workflow_patches(&plan.workflows, &updater)?;
    }
}
```

## Lint Becomes File-by-File

With the iterator scanner, lint rules that are file-local run as files are scanned:

```rust
pub fn run(
    manifest: &Manifest,
    lock: &Lock,
    scanner: &dyn WorkflowScanner,
    lint_config: &LintConfig,
) -> Result<Vec<Diagnostic>, LintError> {
    let mut diagnostics = Vec::new();
    let mut action_set = WorkflowActionSet::new();

    // File-local rules run per file
    for result in scanner.scan() {
        let action = result?;
        check_sha_mismatch(&action, lock, &mut diagnostics);
        check_unpinned(&action, &mut diagnostics);
        check_stale_comment(&action, &mut diagnostics);
        action_set.insert(&action);
    }

    // Global rules run after all files scanned
    check_unsynced_manifest(&action_set, manifest, &mut diagnostics);

    filter_ignored(&mut diagnostics, lint_config);
    Ok(diagnostics)
}
```

## Init: Full Creation Path

`init` creates files from scratch (no existing file to patch). Two options:

**Option A**: Use `format_manifest_toml()` / `serialize_lock()` for init only.
These functions still exist but are only called when creating new files.

**Option B**: Create an empty `toml_edit::DocumentMut`, apply the full plan as inserts.
Same code path as tidy, just with everything in `diff.added`.

Recommendation: **Option B** — single code path, `format_manifest_toml` can be removed entirely.

## Dependency

Add `toml_edit` to `Cargo.toml`:

```toml
toml_edit = "0.22"
```

From the same toml-rs project as the existing `toml = "0.9"` dependency. Used by cargo itself for `Cargo.toml` editing.

## What Gets Removed

After the migration is complete:
- `format_manifest_toml()` — replaced by toml_edit patching
- `serialize_lock()` — replaced by toml_edit patching
- `manifest_to_data()` — intermediate type no longer needed
- `Manifest` mutation methods (`set`, `remove`, `add_override`, `replace_overrides`) — commands no longer mutate
- `Lock` mutation methods (`set`, `set_version`, `set_specifier`, `retain`) — commands no longer mutate
- Ownership transfer of `Manifest`/`Lock` in command signatures — replaced by borrows

## Key Invariants

1. **Plans are the only description of changes.** No mutations happen outside of plan application.
2. **Apply is the only writer.** Commands never call `fs::write`.
3. **Empty plan = zero I/O.** No file reads, writes, or network calls for no-op runs (beyond the initial scan).
4. **Iterator consumers choose their collection.** Domain types never force `Vec` on callers.
5. **File-local lint rules stream.** Only global rules (unsynced manifest) need the full action set.
