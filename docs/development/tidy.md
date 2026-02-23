# tidy command - Implementation

This document describes how the `tidy` command works and the assumptions made during implementation.

**Diagrams:** [System overview](overview-architecture.excalidraw) · [tidy command flow](tidy-command.excalidraw.json)

## Overview

The `tidy` command ensures that `gx.toml` matches the source code in the repository. Similar to `go mod tidy`, it:

1. **Adds** any missing action requirements to `gx.toml` that are used in workflows
2. **Removes** action requirements from `gx.toml` that aren't used in any workflow
3. **Resolves** action versions to commit SHAs via the Github API
4. **Validates** that existing SHAs match their version comments, correcting mismatches
5. **Updates** all workflows to match the pinned versions in `gx.toml`

## Architecture

**Entry point:** `src/commands/app.rs::tidy()` (dispatcher, handles store selection)
↓ Calls ↓

```
src/commands/tidy.rs              # Command entry point and orchestration
src/infrastructure/workflow.rs    # YAML parsing, action extraction, and workflow updates
src/infrastructure/manifest.rs    # ManifestStore trait, FileManifest, MemoryManifest
src/infrastructure/lock.rs        # LockStore trait, FileLock, MemoryLock
src/infrastructure/github.rs      # GithubRegistry (implements VersionRegistry)
src/domain/action.rs              # Domain types: ActionId, Version, CommitSha, UsesRef, InterpretedRef, etc.
src/domain/resolution.rs          # ActionResolver, ResolutionResult, VersionRegistry trait
src/domain/workflow_actions.rs    # WorkflowActionSet (aggregates actions across workflows)
```

## Algorithm

### 1. Scan workflows

`WorkflowParser::scan_all()` finds all `.yml` and `.yaml` files in `.github/workflows/` and extracts action references:

```rust
// Raw data from workflow YAML (no interpretation)
pub struct UsesRef {
    pub action_name: String,  // e.g., "actions/checkout"
    pub uses_ref: String,     // e.g., "v4" or "abc123def..."
    pub comment: Option<String>, // e.g., "v4" from "# v4"
}
```

Each `UsesRef` is then interpreted into domain types via `uses_ref.interpret()`:

```rust
pub struct InterpretedRef {
    pub id: ActionId,
    pub version: Version,       // Normalized version (e.g., "v4")
    pub sha: Option<CommitSha>, // Present only if uses_ref is a 40-char hex SHA
}
```

### 2. Aggregate into WorkflowActionSet

`WorkflowActionSet` deduplicates versions across workflows:

```rust
pub struct WorkflowActionSet {
    versions: HashMap<ActionId, HashSet<Version>>,
    shas: HashMap<ActionId, CommitSha>, // First SHA wins
}
```

Methods:
- `versions_for(id)` - All unique versions found for an action
- `action_ids()` - All action IDs discovered
- `sha_for(id)` - SHA if present in workflow (first one wins)

### 3. Compare with manifest

```rust
let workflow_actions: HashSet<ActionId> = action_set.action_ids().into_iter().collect();
let manifest_actions: HashSet<ActionId> = manifest.specs().iter().map(|s| s.id.clone()).collect();

let missing = workflow_actions.difference(&manifest_actions);  // In workflows, not in manifest
let unused = manifest_actions.difference(&workflow_actions);   // In manifest, not in workflows
```

### 4. Remove unused actions

For each unused action, `manifest.remove(id)` is called and the manifest is marked dirty.

### 5. Add missing actions

For each missing action:

1. Get all versions from `WorkflowActionSet`
2. Select highest semver version via `Version::highest()`
3. Add to manifest via `manifest.set(id, version)`

### 6. Update existing actions

For actions in both manifest and workflows, check if the manifest version should be replaced. This handles the case where a workflow was upgraded from SHA to semver tag:

```rust
if manifest_version.should_be_replaced_by(workflow_version) {
    manifest.set(action_id, workflow_version);
}
```

`should_be_replaced_by` returns true when the current version is a SHA and the other is a semver tag.

### 7. Update lock file

The `update_lock_file` function processes each action in the manifest:

- **If workflow has a SHA**: `ActionResolver::validate_and_correct(spec, sha)` checks that the version comment matches the SHA. Returns `Resolved`, `Corrected`, or `Unresolved`.
- **If no workflow SHA**: `ActionResolver::resolve(spec)` looks up the SHA via Github API.

```rust
pub enum ResolutionResult {
    Resolved(ResolvedAction),
    Corrected { original: ActionSpec, corrected: ResolvedAction },
    Unresolved { spec: ActionSpec, reason: String },
}
```

Version corrections update both the manifest and lock.

### 8. Clean up lock file

Remove entries from lock file that are no longer in the manifest:

```rust
let keys_to_retain: Vec<LockKey> = manifest.specs().iter().map(LockKey::from).collect();
lock.retain(&keys_to_retain);
```

### 9. Update workflows

Apply manifest versions to all workflows using regex replacement. The `lock.build_update_map()` builds a map of `ActionId` → `"SHA # version"` format, which `WorkflowWriter::update_all()` applies.

## Version logic

Version comparison and selection are methods on `Version` in `src/domain/action.rs`:

```rust
impl Version {
    pub fn highest(versions: &[Version]) -> Option<Version>;
    pub fn is_sha(&self) -> bool;
    pub fn is_semver_like(&self) -> bool;
    pub fn should_be_replaced_by(&self, other: &Version) -> bool;
    pub fn precision(&self) -> Option<VersionPrecision>;
    pub fn find_upgrade(&self, candidates: &[Version]) -> Option<Version>;
}
```

Handles formats: `v4`, `v4.1`, `v4.1.2`, `4.1.2`. Non-semver versions (branches, SHAs) are treated as lower priority.

## Manifest structure

The manifest uses the `ManifestStore` trait with two implementations:

```rust
pub trait ManifestStore {
    fn get(&self, id: &ActionId) -> Option<&Version>;
    fn set(&mut self, id: ActionId, version: Version);
    fn has(&self, id: &ActionId) -> bool;
    fn save(&mut self) -> Result<(), ManifestError>;
    fn specs(&self) -> Vec<ActionSpec>;
    fn remove(&mut self, id: &ActionId);
    fn path(&self) -> Result<&Path, ManifestError>;
    fn is_empty(&self) -> bool;
}
```

- `FileManifest` — persists to `.github/gx.toml`, tracks `dirty: bool`
- `MemoryManifest` — in-memory only, `save()` is a no-op

Internal TOML structure:

```rust
struct ManifestData {
    actions: HashMap<String, String>,  // "owner/repo" -> "version"
}
```

## Lock file structure

The lock uses the `LockStore` trait with two implementations:

```rust
pub trait LockStore {
    fn get(&self, key: &LockKey) -> Option<&CommitSha>;
    fn set(&mut self, resolved: &ResolvedAction);
    fn has(&self, key: &LockKey) -> bool;
    fn retain(&mut self, keys: &[LockKey]);
    fn build_update_map(&self, keys: &[LockKey]) -> HashMap<ActionId, String>;
    fn save(&mut self) -> Result<(), LockFileError>;
}
```

- `FileLock` — persists to `.github/gx.lock`, tracks `dirty: bool`
- `MemoryLock` — in-memory only, `save()` is a no-op

Internal TOML structure:

```rust
struct LockData {
    version: String,                    // "1.0"
    actions: HashMap<String, String>,   // "action@version" -> "commit_sha"
}
```

## YAML parsing and version extraction

Uses `serde-saphyr` for YAML parsing with minimal structs:

```rust
struct Workflow { jobs: HashMap<String, Job> }
struct Job { steps: Vec<Step> }
struct Step { uses: Option<String> }
```

### Version extraction strategy

The extraction handles two cases (two-phase approach because YAML parsers strip comments):

1. **Raw content scan**: `uses:\s*([^#\n]+)#\s*(\S+)` builds a map of `uses` → comment
2. **YAML parsing**: `serde-saphyr` extracts structured `uses:` values
3. **Merge**: Each `uses` value is matched against the comment map to create `UsesRef`
4. **Interpretation**: `UsesRef::interpret()` normalizes versions and identifies SHAs

## Assumptions

### Semver for global version

When multiple versions exist, `Version::highest()` selects the highest semver version as the global default. Non-semver versions (branches, SHAs) only become global if no semver versions exist.

### Skip local and docker actions

Ignored patterns:
- Local: `./path/to/action`
- Docker: `docker://image:tag`

### Existing manifest versions preserved

When adding missing actions, existing entries in the manifest are never overwritten. Only new actions are added with versions from workflows.

### Idempotency

Running `gx tidy` multiple times produces the same result. After the first run, subsequent runs should report no changes.

## Github API integration

The `GithubRegistry` struct implements the `VersionRegistry` trait:

```rust
pub trait VersionRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<CommitSha, ResolutionError>;
    fn tags_for_sha(&self, id: &ActionId, sha: &CommitSha) -> Result<Vec<Version>, ResolutionError>;
    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError>;
}
```

Resolution strategy for `lookup_sha`:
1. If already a full SHA (40 hex chars), return as-is
2. Try as a tag: `/repos/{owner/repo}/git/ref/tags/{ref}`
3. Try as a branch: `/repos/{owner/repo}/git/ref/heads/{ref}`
4. Try as a commit: `/repos/{owner/repo}/commits/{ref}`

Uses `reqwest` blocking client with 30-second timeout.

## Testing

### Unit tests

- Version selection (`select_version`)
- `Version::highest()`, `Version::precision()`, `Version::find_upgrade()`
- `UsesRef::interpret()` with various formats
- `WorkflowActionSet` aggregation and deduplication
- `ActionResolver` with mock `VersionRegistry`
- Lock file retain and build_update_map

### Integration tests

- Workflow scanning and action extraction
- SHA with comment extraction
- Short ref handling
- Workflow writing with SHA replacement
- No duplicate comments after update
