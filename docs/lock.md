# Lock file

The lock file stores resolved commit SHAs for Github Actions, ensuring reproducible builds across environments.

## Location

The lock file is located at `.github/gx.lock` in your repository root.

It is automatically managed by `gx tidy` and `gx init` commands.

## Format

The lock file uses TOML format with two sections:

```toml
version = "1.0"

[actions]
"owner/action-name@version" = "commit-sha"
```

### Version

The `version` field at the top indicates the lock file format version. This enables future format changes while maintaining backward compatibility.

Current version: `1.0`

### Actions

The `[actions]` section maps action references (in `owner/repo@version` format) to their resolved commit SHAs.

## Example

```toml
version = "1.0"

[actions]
"actions/checkout@v4" = "11bd71901bbe5b1630ceea73d27597364c9af683"
"actions/setup-node@v4" = "39370e3970a6d050c480ffad4ff0ed4d3fdee5af"
"docker/build-push-action@v5" = "4a13e500e55cf31b7a5d59a38ab2040ab0f42f56"
```

## How it works

When you run `gx tidy` or `gx init`:

1. Actions are extracted from workflow files
2. Version tags are resolved to commit SHAs via the Github API
3. The lock file is updated with the resolved SHAs
4. Workflow files are updated to use `SHA # version` format

This ensures that workflows always use the exact same code, even if a version tag is moved.

## Why commit SHAs?

Version tags in Github can be updated to point to different commits. Using commit SHAs:

- Guarantees reproducible builds
- Protects against supply chain attacks via tag manipulation
- Provides an audit trail of exact action versions used
