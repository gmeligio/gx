# tidy command - Implementation

This document describes how the `tidy` command works and the assumptions made during implementation.

**Diagrams:** [System overview](overview-architecture.excalidraw) · [tidy command flow](tidy-command.excalidraw.json)

## Overview

The `tidy` command ensures that `gx.toml` matches the source code in the repository. Similar to `go mod tidy`, it:

1. **Adds** any missing action requirements to `gx.toml` that are used in workflows
2. **Removes** action requirements from `gx.toml` that aren't used in any workflow
3. **Prunes** stale override entries that reference removed workflows/jobs/steps
4. **Resolves** action versions to commit SHAs via the Github API (global defaults and override versions)
5. **Validates** that existing SHAs match their version comments, correcting mismatches
6. **Updates** each workflow file using per-step override resolution

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
src/domain/manifest.rs            # Manifest domain entity with ActionOverride and resolve_version()
src/domain/resolution.rs          # ActionResolver, ResolutionResult, VersionRegistry trait
src/domain/workflow_actions.rs    # WorkflowActionSet, WorkflowLocation, LocatedAction
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

`WorkflowActionSet` deduplicates versions across workflows and tracks occurrence counts:

```rust
pub struct WorkflowActionSet {
    versions: HashMap<ActionId, HashSet<Version>>,
    shas: HashMap<ActionId, CommitSha>,                 // First SHA wins
    counts: HashMap<ActionId, HashMap<Version, usize>>, // Occurrence counts per version
}
```

Methods:
- `versions_for(id)` - All unique versions found for an action
- `action_ids()` - All action IDs discovered
- `sha_for(id)` - SHA if present in workflow (first one wins)
- `dominant_version(id)` - Most-used version; tiebreak: highest semver

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

1. Call `action_set.dominant_version(id)` — most-used version across all steps; tiebreak: highest semver
2. Add to manifest via `manifest.set(id, version)`

### 6. Update existing actions

For actions in both manifest and workflows, check if the manifest version should be replaced. This handles the case where a workflow was upgraded from SHA to semver tag:

```rust
if manifest_version.should_be_replaced_by(workflow_version) {
    manifest.set(action_id, workflow_version);
}
```

`should_be_replaced_by` returns true when the current version is a SHA and the other is a semver tag.

### 7. Prune stale overrides

`prune_stale_overrides()` scans `manifest.all_overrides()` and removes any override entry whose referenced `workflow`, `job`, or `step` no longer exists in the `LocatedAction` list:

```rust
fn prune_stale_overrides(manifest: &mut Manifest, located: &[LocatedAction]) {
    // Build live set of workflows/jobs/steps from scan
    // For each override: check workflow exists, then job, then step
    // Replace override list with pruned version
}
```

### 8. Update lock file

`update_lock()` processes all action/version pairs — global defaults and override versions:

- **Global specs with a workflow SHA**: `ActionResolver::validate_and_correct(spec, sha)` checks that the version comment matches the SHA. Returns `Resolved`, `Corrected`, or `Unresolved`.
- **Global specs without a SHA**: `ActionResolver::resolve(spec)` looks up the SHA via Github API.
- **Override versions**: Always resolved via `ActionResolver::resolve()` (no SHA correction).

```rust
pub enum ResolutionResult {
    Resolved(ResolvedAction),
    Corrected { original: ActionSpec, corrected: ResolvedAction },
    Unresolved { spec: ActionSpec, reason: String },
}
```

Version corrections update both the manifest and lock.

### 9. Clean up lock file

Retain only entries that correspond to a current (action, version) pair — including override versions:

```rust
fn build_keys_to_retain(manifest: &Manifest) -> Vec<LockKey> {
    // Global specs → LockKey
    // Override versions → LockKey (if not already present)
}
lock.retain(&keys_to_retain);
```

### 10. Update workflows (per-file, override-aware)

Workflow files are updated one at a time. For each file, `build_file_update_map()` resolves each step's version through the override hierarchy (step > job > workflow > global), then looks up the corresponding SHA from the lock:

```rust
fn build_file_update_map(manifest: &Manifest, lock: &Lock, steps: &[&LocatedAction])
    -> HashMap<ActionId, String>
{
    // For each step: manifest.resolve_version(id, location) → version
    // lock.get(LockKey::new(id, version)) → sha
    // Result: "sha # version"
}
```

`WorkflowUpdater::update_file(path, map)` applies the map to the single file using regex replacement. The workflow path is matched against stored relative paths using suffix matching (`abs_str.ends_with(rel_path)`).

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

The `Manifest` domain entity (in `src/domain/manifest.rs`) owns two maps:

```rust
pub struct Manifest {
    actions: HashMap<ActionId, ActionSpec>,               // global defaults
    overrides: HashMap<ActionId, Vec<ActionOverride>>,  // location-specific overrides
}

pub struct ActionOverride {
    pub workflow: String,       // relative path, e.g. ".github/workflows/deploy.yml"
    pub job: Option<String>,    // job id
    pub step: Option<usize>,    // 0-based step index (requires job)
    pub version: Version,
}
```

Key methods:
- `resolve_version(id, location)` — resolves through step > job > workflow > global hierarchy
- `add_override(id, exc)`, `overrides_for(id)`, `all_overrides()`, `replace_overrides(id, list)`
- `remove(id)` — removes both the global entry and all its overrides

The `ManifestStore` trait has two implementations:
- `FileManifest` — reads/writes `.github/gx.toml`
- `MemoryManifest` — in-memory only, `save()` is a no-op; `from_workflows()` uses `dominant_version()`

Internal TOML wire types (in `src/infrastructure/manifest.rs`):

```rust
struct TomlActions {
    #[serde(flatten)]
    versions: BTreeMap<String, String>,                    // "owner/repo" -> "version"
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    overrides: BTreeMap<String, Vec<TomlOverride>>,
}

struct TomlOverride {
    workflow: String,
    job: Option<String>,
    step: Option<usize>,
    version: String,
}
```

Validation in `manifest_from_data()`:
- An override requires a global default for the same action
- `step` requires `job` to be set
- Duplicate scope entries (`workflow` + `job` + `step`) are rejected

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

### Dominant version for global default

When adding a new action with multiple versions observed in workflows, `WorkflowActionSet::dominant_version()` selects the global default:
1. Most-used version (highest occurrence count across all steps) wins
2. Tiebreak: highest semver

Non-semver versions (branches, SHAs) only become global if no semver versions exist.

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
- Override-aware per-file workflow updates (`test_gx_tidy_respects_override_for_specific_workflow`)
- Job-level override resolution (`test_gx_tidy_override_job_level`)
- Stale override pruning (`test_gx_tidy_removes_stale_override`)
- Dominant version selection (most-used wins, tiebreak: highest semver)
