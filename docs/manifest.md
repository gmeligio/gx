# Manifest file

The manifest file defines which versions of GitHub Actions to use across all your workflows, with support for hierarchical overrides.

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

## Hierarchical overrides

When different versions are needed in specific contexts, use hierarchical overrides:

### Workflow-level override

```toml
[actions]
"actions/checkout" = "v4"

[workflows."deploy.yml".actions]
"actions/checkout" = "v3"
```

### Job-level override

```toml
[actions]
"actions/checkout" = "v4"

[workflows."ci.yml".jobs."legacy-build".actions]
"actions/checkout" = "v3"
```

### Step-level override

```toml
[actions]
"actions/checkout" = "v4"

[workflows."ci.yml".jobs."build".steps."0".actions]
"actions/checkout" = "v3"
```

## Override precedence

Versions are resolved from most specific to least specific:

1. Step-level (`workflows."x".jobs."y".steps."0".actions`)
2. Job-level (`workflows."x".jobs."y".actions`)
3. Workflow-level (`workflows."x".actions`)
4. Global (`actions`)

## Action names

Action names must match the format used in workflow files:
- `owner/repo` for GitHub actions (e.g., `actions/checkout`)
- Full format with owner and repository name

## Versions

Versions can be:
- Tags: `"v4"`, `"v3.2.1"`
- Branch names: `"main"`, `"develop"`
- Commit SHAs: `"a1b2c3d"`

The version format must match what GitHub Actions accepts in the `uses:` field.

## Empty manifest

An empty `[actions]` section is valid:

```toml
[actions]
```

Running `gx pin` with an empty manifest will not modify any workflows.
