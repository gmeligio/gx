# Manifest file

The manifest file defines which versions of GitHub Actions to use across all your workflows.

## Location

The manifest must be located at `.github/gx.toml` in your repository root.

## Format

The manifest uses TOML format with an `[actions]` section:

```toml
[actions]
"owner/action-name" = "version"
```

## Example

```toml
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
"actions/setup-python" = "v5"
"docker/build-push-action" = "v5"
```

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
