# tidy command - Implementation

This document describes how the `tidy` command works and the assumptions made during implementation.

## Overview

The `tidy` command ensures that `gx.toml` matches the source code in the repository. Similar to `go mod tidy`, it:

1. **Adds** any missing action requirements to `gx.toml` that are used in workflows
2. **Removes** action requirements from `gx.toml` that aren't used in any workflow
3. **Updates** all workflows to match the pinned versions in `gx.toml`

## Architecture

```
src/commands/tidy.rs      # Command entry point and orchestration
src/workflow.rs           # YAML parsing, action extraction, and workflow updates
src/manifest.rs           # Hierarchical manifest structure (gx.toml)
src/lock.rs               # Lock file structure (gx.lock)
src/github.rs             # GitHub API client for resolving refs to SHAs
src/version.rs            # Semver parsing and comparison
```

## Algorithm

### 1. Scan workflows

Find all `.yml` and `.yaml` files in `.github/workflows/` and extract action references with full location info:

```rust
pub struct ExtractedAction {
    pub name: String,
    pub version: String,
    pub file: PathBuf,
    pub location: ActionLocation,
}

pub struct ActionLocation {
    pub workflow: String,   // e.g., "ci.yml"
    pub job: String,        // e.g., "build"
    pub step_index: usize,  // e.g., 0
}
```

### 2. Build version tree

Group all extracted actions by name and track unique versions at each level:

```rust
struct ActionVersionTree {
    usages: HashMap<String, Vec<ActionUsage>>,
}
```

Methods:
- `unique_versions(action)` - All versions used across all locations
- `workflow_versions(action, workflow)` - Versions used in a specific workflow
- `job_versions(action, workflow, job)` - Versions used in a specific job

### 3. Compare with manifest

```rust
let workflow_actions: HashSet<String> = /* actions found in workflows */;
let manifest_actions: HashSet<String> = /* actions in gx.toml */;

let missing = workflow_actions - manifest_actions;  // In workflows, not in manifest
let unused = manifest_actions - workflow_actions;   // In manifest, not in workflows
```

### 4. Remove unused actions

For each unused action:
- Remove from `[actions]` section
- Remove from all workflow/job/step overrides
- Clean up empty override sections

### 5. Add missing actions

For each missing action:

1. **Single version globally**: Add to `[actions]`
2. **Multiple versions**:
   - Find highest semver → global `[actions]`
   - For each workflow with different version:
     - If all jobs same → `[workflows."x".actions]`
     - If jobs differ:
       - For each job with different version:
         - If all steps same → `[workflows."x".jobs."y".actions]`
         - If steps differ → `[workflows."x".jobs."y".steps."N".actions]`

### 6. Update lock file

For each action@version in the manifest (including all overrides):
- Skip if already in lock file
- Resolve ref to commit SHA via GitHub API
- Store in lock file as `"action@version" = "sha"`

### 7. Clean up lock file

Remove entries from lock file that are no longer in the manifest.

### 8. Update workflows

Apply manifest versions to all workflows using regex replacement:

```
uses: owner/repo@oldversion → uses: owner/repo@newversion
```

## Semver comparison

```rust
// src/version.rs
pub fn parse_semver(version: &str) -> Option<Version>;
pub fn higher_version(a: &str, b: &str) -> &str;
pub fn find_highest_version(versions: &[&str]) -> Option<&str>;
```

Handles formats: `v4`, `v4.1`, `v4.1.2`, `4.1.2`

Non-semver versions (branches, SHAs) are treated as lower priority.

## Manifest structure

```rust
pub struct Manifest {
    pub actions: HashMap<String, String>,
    pub workflows: HashMap<String, WorkflowOverride>,
}

pub struct WorkflowOverride {
    pub actions: HashMap<String, String>,
    pub jobs: HashMap<String, JobOverride>,
}

pub struct JobOverride {
    pub actions: HashMap<String, String>,
    pub steps: HashMap<String, StepOverride>,  // String keys for TOML compat
}

pub struct StepOverride {
    pub actions: HashMap<String, String>,
}
```

Empty sections are skipped during serialization via `skip_serializing_if`.

## Lock file structure

```rust
pub struct LockFile {
    pub actions: HashMap<String, String>,  // "action@version" -> "commit_sha"
}
```

Example `.github/gx.lock`:
```toml
[actions]
"actions/checkout@v4" = "11bd71901bbe5b1630ceea73d27597364c9af683"
"actions/setup-node@v3" = "1a4442cacd436585916779262731d5b162bc6ec7"
```

The lock file ensures reproducible builds by pinning exact commit SHAs.

## YAML parsing and version extraction

Uses `serde_yaml` with minimal structs:

```rust
#[derive(Deserialize)]
struct Workflow {
    jobs: HashMap<String, Job>,
}

#[derive(Deserialize)]
struct Job {
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    uses: Option<String>,
}
```

### Version extraction strategy

The extraction handles two cases:

1. **Tag/branch reference**: `uses: actions/checkout@v4`
   - Regex: `^([^@\s]+)@([^\s#]+)` extracts `actions/checkout` and `v4`
   - Version: `v4`

2. **SHA with comment tag**: `uses: actions/checkout@abc123 # v4`
   - First pass (raw content): `uses:\s*([^#\n]+)#\s*v?(\S+)` builds a map of uses → version
   - YAML parsing gets `uses: "actions/checkout@abc123"`
   - Lookup in map finds comment version `v4`
   - Version: `v4`

This two-phase approach is necessary because YAML parsers strip comments. The raw content is scanned first to extract version comments before YAML parsing.

## Assumptions

### Semver for global version

When multiple versions exist, the highest semver version becomes the global default. This assumes:
- Newer versions are preferred
- Tags like `v4` > `v3` > `v2`

Non-semver versions (branches, SHAs) only become global if no semver versions exist.

### Skip local and docker actions

Ignored patterns:
- Local: `./path/to/action`
- Docker: `docker://image:tag`

### TOML step keys are strings

Step indices are stored as string keys (e.g., `"0"`, `"1"`) because TOML doesn't support numeric keys in nested tables.

### Existing manifest versions preserved

When adding missing actions:
- Existing entries in `[actions]` are never overwritten
- Only new actions are added with versions from workflows
- Existing overrides are preserved

### Idempotency

Running `gx tidy` multiple times produces the same result. After the first run, subsequent runs should report no changes.

## Testing

### Unit tests

- Version tree grouping and filtering
- Action removal from overrides
- Empty override cleanup
- Semver parsing and comparison

### Integration tests

- Creates manifest from workflows
- Updates workflows from manifest
- Removes unused actions
- Adds missing actions
- Preserves existing versions
- Multiple workflows
- Skipping local actions
- Hierarchical version overrides
- Idempotency

## GitHub API integration

The `github` module resolves version refs to commit SHAs:

```rust
pub struct GitHubClient {
    client: reqwest::blocking::Client,
}

impl GitHubClient {
    pub fn resolve_ref(&self, owner_repo: &str, ref_name: &str) -> Result<String>;
}
```

Resolution strategy:
1. If already a full SHA (40 hex chars), return as-is
2. Try as a tag: `/repos/{owner/repo}/git/ref/tags/{ref}`
3. Try as a branch: `/repos/{owner/repo}/git/ref/heads/{ref}`
4. Try as a commit: `/repos/{owner/repo}/commits/{ref}`

Uses `reqwest` blocking client with 30-second timeout.

## Output format

```
Scanning workflows...
  .github/workflows/ci.yml
  .github/workflows/deploy.yml

Removing unused actions from manifest:
  - actions/old-action

Adding missing actions to manifest:
  + actions/checkout@v4
  + actions/setup-node@v3
  + docker/build-push-action (multiple versions):
      global: v5
      deploy.yml: v4

Manifest updated: .github/gx.toml
  Resolving actions/checkout@v4 ...
  Resolving actions/setup-node@v3 ...
  Resolving docker/build-push-action@v5 ...
  Resolving docker/build-push-action@v4 ...
Lock file updated: .github/gx.lock

Updated workflows:
  .github/workflows/ci.yml
    - actions/checkout@v4
    - actions/setup-node@v3

2 workflow(s) updated.
```

When everything is in sync:

```
Scanning workflows...
  .github/workflows/ci.yml

Manifest is already in sync with workflows.
Lock file updated: .github/gx.lock

Workflows are already up to date.
```
