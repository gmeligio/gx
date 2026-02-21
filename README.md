[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/gmeligio/gx)
[![Ask NotebookLM](https://img.shields.io/badge/NotebookLM-000000.svg?style=for-the-badge&logo=NotebookLM&logoColor=white)](https://notebooklm.google.com/notebook/0e1bc78e-7f6b-4781-b2b1-17e5afc1dd19)

# gx

Package manager for Github Actions. Like `go mod tidy` for your workflows.

## Why gx?

- **Security**: Github recommends [pinning actions to commit SHAs](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions#using-third-party-actions) to prevent supply chain attacks. Maintaining SHAs by hand is tedious and error-prone.
- **Automation**: gx resolves version tags to commit SHAs, updates your workflows, and keeps everything in sync.
- **Flexibility**: Run with zero configuration (memory-only mode) or create a manifest for team reproducibility.
- **Upgrades**: Check for newer versions and upgrade with a single command, respecting your version precision.

## Quick start

```bash
# Pin all actions in your workflows to commit SHAs
gx tidy

# Or initialize a manifest for reproducible builds
gx init
```

Before:
```yaml
- uses: actions/checkout@v4
```

After:
```yaml
- uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4
```

## Installation

### From crates.io

```bash
cargo install gx
```

### From source

```bash
git clone https://github.com/gmeligio/gx.git
cd gx
cargo install --path .
```

## Commands

### tidy

The command you'll run most often. It doesn't require configuration.

Update pinned SHAs when action versions change in your workflows. If `gx.toml` exists, ensure it matches the workflows — add missing actions, remove unused ones, and update the lock file.

```bash
gx tidy
```

[Learn more](docs/tidy.md)

### init

Set up gx with a manifest and lock file for team reproducibility. Creates `gx.toml` and `gx.lock` from your current workflows.

```bash
gx init
```

[Learn more](docs/init.md)

### upgrade

Check for newer versions of actions and upgrade them. Resolves new SHAs and updates workflows. Skips non-semver versions.

```bash
gx upgrade
```

[Learn more](docs/upgrade.md)

## How it works

gx operates in two modes:

- **Memory-only** (no `gx.toml`): Scans workflows, resolves SHAs, and updates workflow files in place. No manifest or lock files are created.
- **File-backed** (with `gx.toml`): Maintains a manifest (`.github/gx.toml`) and lock file (`.github/gx.lock`) for reproducible builds across your team.

gx uses a two-phase approach to extract version information from workflows. Since YAML parsers strip comments, it first scans raw content for version comments (`uses: action@SHA # v4`), then parses the YAML structure and merges the results.

## Configuration

- [Manifest file](docs/manifest.md) (`.github/gx.toml`) — defines which versions to use, with support for hierarchical overrides
- [Lock file](docs/lock.md) (`.github/gx.lock`) — stores resolved commit SHAs for reproducible builds

## Options

- `-v, --verbose` — Enable verbose output
- `--version` — Print version

## Environment

- `GITHUB_TOKEN` — Optional, needed for resolving commit SHAs via the Github API. [Create a token](https://github.com/settings/tokens).

## Documentation

| Document | Description |
|---|---|
| [tidy](docs/tidy.md) | Tidy command usage and examples |
| [init](docs/init.md) | Init command usage and examples |
| [upgrade](docs/upgrade.md) | Upgrade command usage and examples |
| [Manifest](docs/manifest.md) | Manifest file format and hierarchical overrides |
| [Lock file](docs/lock.md) | Lock file format and why commit SHAs |
| [Contributing](docs/CONTRIBUTING.md) | How to contribute |

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for setup instructions and guidelines.

## License

[MIT](LICENSE)
