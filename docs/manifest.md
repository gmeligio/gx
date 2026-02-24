# Manifest file

The manifest file defines which versions of Github Actions to use across all your workflows, with support for location-specific overrides.

## Location

The manifest must be located at `.github/gx.toml` in your repository root.

The lock file (`.github/gx.lock`) stores the resolved commit SHAs and is automatically managed by `gx tidy`.

## Format

The manifest uses TOML format with an `[actions]` section for global defaults:

```toml
[actions]
"owner/action-name" = "version"
```

## Simple example

```toml
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
"actions/setup-python" = "v5"
"docker/build-push-action" = "v5"
```

## Action overrides

When a specific workflow, job, or step must use a different version than the global default, add an `[actions.overrides]` sub-table.

Each override entry is a list of objects under the action name:

```toml
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
```

### Workflow-level override

Applies to all steps in the named workflow:

```toml
[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
```

### Job-level override

Applies to all steps in the named job within the named workflow:

```toml
[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/ci.yml", job = "legacy-build", version = "v3" },
]
```

### Step-level override

Applies to a single step (0-based index) within a job (requires `job`):

```toml
[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/ci.yml", job = "build", step = 0, version = "v3" },
]
```

### Multiple overrides

Multiple entries can be combined:

```toml
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
  { workflow = ".github/workflows/ci.yml", job = "legacy-build", version = "v2" },
]
```

## Override resolution order

Versions are resolved from most specific to least specific:

1. **Step-level** (`workflow` + `job` + `step`)
2. **Job-level** (`workflow` + `job`, no `step`)
3. **Workflow-level** (`workflow` only, no `job` or `step`)
4. **Global** (`[actions]` default)

## Override field reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `workflow` | string | yes | Relative path from repo root, e.g. `.github/workflows/ci.yml` |
| `job` | string | no | Job id as defined in the workflow file |
| `step` | integer | no | 0-based step index within the job (requires `job`) |
| `version` | string | yes | Version to use at this location |

## Validation rules

- An override entry requires a global default for the same action (`gx tidy` enforces this)
- `step` requires `job` to be set
- Duplicate scope entries (same `workflow`+`job`+`step` combination) are rejected

## Stale overrides

`gx tidy` automatically removes override entries whose referenced workflow, job, or step no longer exists.

## Action names

Action names must match the format used in workflow files:
- `owner/repo` for Github actions (e.g., `actions/checkout`)
- Full format with owner and repository name

## Versions

Versions can be:
- Tags: `"v4"`, `"v3.2.1"`
- Branch names: `"main"`, `"develop"`
- Commit SHAs: `"a1b2c3d"`

The version format must match what Github Actions accepts in the `uses:` field.

## Empty manifest

An empty `[actions]` section is valid:

```toml
[actions]
```

Running `gx tidy` with an empty manifest will not modify any workflows.

## Implementation

`gx.toml` is managed through the `Manifest` domain entity (`src/domain/manifest.rs`), which owns all CRUD operations and override resolution logic. Persistence is handled by `ManifestStore` (`src/infrastructure/manifest.rs`): `FileManifest` reads/writes disk, `MemoryManifest` is used when no `gx.toml` exists.
