[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/gmeligio/gx)
[![Ask NotebookLM](https://img.shields.io/badge/NotebookLM-000000.svg?style=for-the-badge&logo=NotebookLM&logoColor=white)](https://notebooklm.google.com/notebook/0e1bc78e-7f6b-4781-b2b1-17e5afc1dd19)
[![crates.io](https://img.shields.io/crates/v/gx.svg)](https://crates.io/crates/gx)
[![crates.io](https://img.shields.io/crates/d/gx)](https://crates.io/crates/gx)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

# gx

Package manager for GitHub Actions.

![gx tidy demo](docs/demo.gif)

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

The tag `v4` can point to different code tomorrow. The commit SHA cannot. gx rewrites your workflows to use SHAs and keeps a comment with the version for readability.

## Why pin to commits?

When your workflow says `actions/checkout@v4`, that tag can be moved to point to different code at any time. Pinning to a commit SHA guarantees you always run the exact code you reviewed. [GitHub recommends this practice](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions#using-third-party-actions), but doing it by hand is tedious. gx automates it.

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
gx tidy      # Pin actions to commit SHAs and sync manifest if present
gx upgrade   # Upgrade pinned actions to newer versions
gx lint      # Check for unpinned or mismatched actions
gx init      # Create a manifest and lock file from your current workflows
```

## Already using another tool?

gx works alongside your existing setup.

| If you use… | gx adds… |
|---|---|
| No tool | SHA pinning, version upgrades, lint, and a manifest to keep your team in sync |
| Renovate | A local CLI (no bot/PR required), lint checks, and a manifest/lock system for auditing |
| Dependabot | Initial SHA pinning ([not yet supported](https://github.com/dependabot/dependabot-core/issues/7913) by Dependabot), lint, and a manifest/lock system |
| ratchet | A manifest/lock system for team reproducibility, and standard version comments (no `# ratchet:` prefix) |
| pinact | A manifest/lock system for team reproducibility |

## Configuration

gx works with no configuration. Run `gx tidy` and your workflows are pinned.

For teams that want reproducibility, `gx init` creates a manifest (`.github/gx.toml`) and lock file (`.github/gx.lock`) that track every pinned action. See the [documentation](https://deepwiki.com/gmeligio/gx) for details on the manifest format and overrides.

## FAQ

<details>
<summary>Do I need a GITHUB_TOKEN?</summary>

No, but without one you're limited to 60 GitHub API requests per hour. For most projects that's enough. Set `GITHUB_TOKEN` for CI or large repos.

</details>

<details>
<summary>How do I use gx in CI?</summary>

Add `gx lint` to your workflow to enforce pinning on every PR:

```yaml
- name: Check action pins
  run: gx lint
```

</details>

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for setup instructions and guidelines. Questions? [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/gmeligio/gx) [![Ask NotebookLM](https://img.shields.io/badge/NotebookLM-000000.svg?style=for-the-badge&logo=NotebookLM&logoColor=white)](https://notebooklm.google.com/notebook/0e1bc78e-7f6b-4781-b2b1-17e5afc1dd19)

## License

[MIT](LICENSE.md)
