# init command

The `init` command creates manifest and lock files from your current workflows, enabling reproducible builds and team collaboration.

## Usage

```bash
gx init
```

## What it does

1. **Scans** all workflow files in `.github/workflows/`
2. **Creates** `.github/gx.toml` with action versions extracted from workflows
3. **Resolves** each action version to a commit SHA via the Github API
4. **Creates** `.github/gx.lock` with the resolved commit SHAs
5. **Updates** all workflows to use `SHA # version` format

This is equivalent to running `gx tidy` for the first time with file-backed storage.

## Prerequisites

- No existing `.github/gx.toml` file. If it already exists, use `gx tidy` instead.
- `GITHUB_TOKEN` environment variable set for resolving commit SHAs.

## When to use

Use `gx init` when you want to:

- Set up gx for the first time in a repository
- Enable reproducible builds across your team
- Start tracking action versions in a manifest file

If you don't need a manifest file and just want to pin SHAs in your workflows, use `gx tidy` without initializing — it works in memory-only mode.

## Example

Given this workflow (`.github/workflows/ci.yml`):

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
```

Running `gx init` creates `.github/gx.toml`:

```toml
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v3"
```

And `.github/gx.lock`:

```toml
version = "1.0"

[actions]
"actions/checkout@v4" = "11bd71901bbe5b1630ceea73d27597364c9af683"
"actions/setup-node@v3" = "1a4442cacd436585916779262731d5b162bc6ec7"
```

And updates the workflow to use pinned SHAs:

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4
      - uses: actions/setup-node@1a4442cacd436585916779262731d5b162bc6ec7 # v3
```

## Difference from tidy

| | `gx init` | `gx tidy` (no manifest) |
|---|---|---|
| Creates `gx.toml` | Yes | No (memory-only) |
| Creates `gx.lock` | Yes | No (memory-only) |
| Updates workflows | Yes | Yes |
| Requires no existing manifest | Yes (errors if exists) | Works either way |

## Error handling

If `.github/gx.toml` already exists, `gx init` exits with an error:

```
Already initialized. Use `gx tidy` to update.
```
