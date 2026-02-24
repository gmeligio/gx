# tidy command

The `tidy` command ensures that `gx.toml` matches the source code in your repository. It adds any missing action requirements, removes unused ones, updates the lock file with resolved commit SHAs, and updates all workflows to use the pinned versions.

## Usage

```bash
gx tidy
```

## What it does

1. **Scans** all workflow files in `.github/workflows/` (with per-step location context)
2. **Adds** missing actions to `.github/gx.toml` that are used in workflows
3. **Removes** unused actions from `.github/gx.toml` that aren't in any workflow
4. **Prunes** stale exception entries that reference removed workflows/jobs/steps
5. **Updates** `.github/gx.lock` with resolved commit SHAs for all action versions (both global defaults and exception versions)
6. **Removes** unused entries from `.github/gx.lock`
7. **Updates** each workflow file using per-step exception resolution

This is similar to how `go mod tidy` works for Go modules.

## Lock file

The `gx.lock` file stores the resolved commit SHA for each action@version combination. This ensures reproducible builds by pinning exact commits.

Example `.github/gx.lock`:
```toml
[actions]
"actions/checkout@v4" = "11bd71901bbe5b1630ceea73d27597364c9af683"
"actions/setup-node@v3" = "1a4442cacd436585916779262731d5b162bc6ec7"
```

The lock file is automatically generated and updated by `gx tidy`. You should commit it to version control.

## Version extraction from workflows

`gx tidy` extracts version information from your workflows in two ways:

### 1. Tag/branch reference
When using a tag or branch name directly:
```yaml
- uses: actions/checkout@v4
```
Version extracted: `v4`

### 2. Commit SHA with comment tag
When using a commit SHA with a version comment:
```yaml
- uses: actions/checkout@abc123def456 # v4
- uses: actions/setup-node@xyz789 #v3
```
Version extracted: `v4` and `v3` respectively

The version comment can be:
- `# v4` (with space and 'v' prefix)
- `#v4` (no space, with 'v' prefix)
- `# 4` (with space, no 'v' prefix - will be normalized to `v4`)
- `#4` (no space, no 'v' prefix - will be normalized to `v4`)

This allows you to use pinned commit SHAs in your workflows while maintaining semantic version tags in `gx.toml`.

## Example: Initial setup

Given this workflow (`.github/workflows/ci.yml`) with no manifest:

```yaml
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
```

Running `gx tidy` will create `.github/gx.toml`:

```toml
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v3"
```

## Example: Adding new actions

If you add a new action to a workflow:

```yaml
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
      - uses: docker/build-push-action@v5  # newly added
```

Running `gx tidy` will add it to the manifest:

```toml
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v3"
"docker/build-push-action" = "v5"
```

## Example: Removing unused actions

If your manifest has an action that's no longer used:

```toml
[actions]
"actions/checkout" = "v4"
"actions/old-action" = "v1"  # not used in any workflow
```

Running `gx tidy` will remove it:

```toml
[actions]
"actions/checkout" = "v4"
```

## Example: Updating workflows

If your manifest specifies `v4` but a workflow uses `v3`:

Manifest:
```toml
[actions]
"actions/checkout" = "v4"
```

Workflow before:
```yaml
- uses: actions/checkout@v3
```

After `gx tidy`, the workflow is updated:
```yaml
- uses: actions/checkout@v4
```

## Example: Multiple versions across workflows

Given two workflows with different versions:

`.github/workflows/ci.yml`:
```yaml
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
```

`.github/workflows/deploy.yml`:
```yaml
jobs:
  deploy:
    steps:
      - uses: actions/checkout@v3
```

Running `gx tidy` creates a hierarchical manifest:

```toml
[actions]
"actions/checkout" = "v4"

[workflows."deploy.yml".actions]
"actions/checkout" = "v3"
```

The highest semver version (v4) becomes the global default, and deploy.yml gets a workflow-level override.

## Example: Multiple versions within a workflow

Given a workflow with different versions in different jobs:

`.github/workflows/ci.yml`:
```yaml
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  test:
    steps:
      - uses: actions/checkout@v3
```

Running `gx tidy` creates:

```toml
[actions]
"actions/checkout" = "v4"

[workflows."ci.yml".jobs.test.actions]
"actions/checkout" = "v3"
```

## Exception-aware resolution

When the manifest contains `[actions.exceptions]` entries, `gx tidy` applies them per workflow file. Each step is resolved using the exception hierarchy (step > job > workflow > global), and workflow files are updated file-by-file with the correct version for each step.

Example: given this manifest:

```toml
[actions]
"actions/checkout" = "v4"

[actions.exceptions]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
```

After `gx tidy`:
- `.github/workflows/ci.yml` steps use `v4` SHA
- `.github/workflows/deploy.yml` steps use `v3` SHA

### Stale exception cleanup

`gx tidy` automatically removes exception entries that reference workflows, jobs, or step indices that no longer exist in the scanned workflows.

## Behavior

### Version resolution for new actions

When adding a new action that appears with different versions:
- The **most-used** version becomes the global default (highest occurrence count across all steps)
- Tiebreak: highest semver wins

For non-semver versions (branches, SHAs), the most-used version is selected; highest semver is the tiebreak.

### Existing manifest versions

When adding missing actions:
- Existing entries in `[actions]` are preserved (not overwritten)
- Only new actions are added with versions from workflows

### Idempotency

Running `gx tidy` multiple times produces the same result. After the first run, subsequent runs should report no changes if the codebase hasn't changed.

### Skipped actions

The following are ignored:
- Local actions: `./path/to/action`
- Docker actions: `docker://image:tag`

## Output

```
Scanning workflows...
  .github/workflows/ci.yml
  .github/workflows/deploy.yml

Removing unused actions from manifest:
  - actions/old-action

Adding missing actions to manifest:
  + actions/checkout@v4
  + actions/setup-node@v3

Manifest updated: .github/gx.toml
  Resolving actions/checkout@v4 ...
  Resolving actions/setup-node@v3 ...
Lock file updated: .github/gx.lock

Updated workflows:
  .github/workflows/ci.yml
    - actions/checkout@v4

1 workflow(s) updated.
```

When everything is in sync:

```
Scanning workflows...
  .github/workflows/ci.yml

Manifest is already in sync with workflows.
Lock file updated: .github/gx.lock

Workflows are already up to date.
```
