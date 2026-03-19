[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/gmeligio/gx)
[![Ask NotebookLM](https://img.shields.io/badge/NotebookLM-000000.svg?style=for-the-badge&logo=NotebookLM&logoColor=white)](https://notebooklm.google.com/notebook/0e1bc78e-7f6b-4781-b2b1-17e5afc1dd19)
[![crates.io](https://img.shields.io/crates/v/gx.svg)](https://crates.io/crates/gx)
[![crates.io](https://img.shields.io/crates/d/gx)](https://crates.io/crates/gx)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

# gx

Package manager for GitHub Actions. Like `npm` for your workflows.

![gx tidy demo](docs/demo.gif)

## Why gx?

GitHub recommends [pinning actions to commit SHAs](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions#using-third-party-actions) to prevent supply chain attacks, but maintaining SHAs by hand is tedious and error-prone. gx automates it.

- **Security**: Resolves version tags to commit SHAs automatically.
- **Automation**: Updates your workflows and keeps everything in sync.
- **Flexibility**: Works with zero configuration or with a manifest for team reproducibility.
- **Lint**: Catches unpinned actions, SHA mismatches, and stale version comments before they reach CI.

See the [FAQ](#faq) for how gx compares to Renovate and Dependabot.

## Quick Start

Before:
```yaml
- uses: actions/checkout@v4
- uses: actions/setup-node@v4
```

After running `gx tidy`:
```yaml
- uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4
- uses: actions/setup-node@39370e3970a6d050c480ffad4ff0ed4d3fdee5af # v4
```

```bash
# Pin all actions in your workflows to commit SHAs
gx tidy

# Or initialize a manifest for reproducible team builds
gx init
```

## Installation

```bash
brew install gmeligio/tap/gx
```

<details>
<summary>More options</summary>

### Binary download

Download a pre-built binary for your platform from [GitHub Releases](https://github.com/gmeligio/gx/releases).

### Cargo

```bash
cargo install gx
```

</details>

## Commands

```bash
gx tidy     # Resolve tags to SHAs and update workflows. Syncs manifest if gx.toml exists.
gx init     # Create gx.toml and gx.lock from current workflows.
gx upgrade  # Upgrade pinned actions to newer versions. Skips non-semver.
gx lint     # Check for unpinned actions, SHA mismatches, stale comments, unsynced manifests.
```

## Configuration

gx operates in two modes:

- **Memory-only** (no `gx.toml`): Scans workflows, resolves SHAs, and updates workflow files in place.
- **File-backed** (with `gx.toml`): Maintains a manifest (`.github/gx.toml`) and lock file (`.github/gx.lock`) for reproducible builds across your team.

Global options: `-v, --verbose` for verbose output, `--version` for version info.

For details on the manifest format, hierarchical overrides, and lock file schema, see the [DeepWiki documentation](https://deepwiki.com/gmeligio/gx).

## FAQ

<details>
<summary>Do I need a GITHUB_TOKEN?</summary>

No, but without it you are limited to 60 unauthenticated GitHub API requests per hour. For most projects `gx tidy` finishes well within that limit. Set `GITHUB_TOKEN` to avoid rate limits in CI or for large repos.

```bash
export GITHUB_TOKEN=<your-token>
gx tidy
```

[Create a token](https://github.com/settings/tokens) with no extra scopes required (public repo access is enough).

</details>

<details>
<summary>Do I need a gx.toml manifest?</summary>

No. `gx tidy` works without any configuration. The manifest is optional and exists for teams that need reproducible builds: it locks every action to a specific SHA so that everyone on the team resolves the same versions.

</details>

<details>
<summary>How do I use gx in CI?</summary>

Add `gx lint` as a step in your CI workflow to enforce pinning policies on every PR:

```yaml
- name: Check action pins
  run: gx lint
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

Run `gx tidy` locally (or in a scheduled workflow) to keep pins up to date.

</details>

<details>
<summary>How does gx compare to Renovate and Dependabot?</summary>

These tools are complementary, not competing:

| Tool | Approach | Scope |
|---|---|---|
| Renovate / Dependabot | Bot, opens PRs on a schedule | All dependency types (npm, Docker, Actions, ...) |
| gx | CLI, rewrites in place | GitHub Actions only |

Key differences between gx and the bots:

- **Initial SHA pinning**: Dependabot cannot do initial SHA pinning (open feature request since 2021). gx handles this with `gx tidy`.
- **Manifest + lock system**: gx tracks every pinned SHA in a lock file, similar to `go.sum`. This makes auditing and reproducibility straightforward.
- **Structured lint**: `gx lint` can block CI on unpinned actions or SHA mismatches, giving you a policy enforcement layer.
- **Hierarchical overrides**: `gx.toml` supports per-workflow and per-job version overrides.

</details>

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for setup instructions and guidelines. Questions? [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/gmeligio/gx) [![Ask NotebookLM](https://img.shields.io/badge/NotebookLM-000000.svg?style=for-the-badge&logo=NotebookLM&logoColor=white)](https://notebooklm.google.com/notebook/0e1bc78e-7f6b-4781-b2b1-17e5afc1dd19)

## License

[MIT](LICENSE.md)
