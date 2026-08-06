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
gx tidy         # Pin actions to commit SHAs and sync manifest if present
gx upgrade      # Upgrade pinned actions to newer versions (--json for CI/PR automation)
gx lint         # Check action pinning, security, workflow validity, and run: shell scripts (see docs/lint-rules.md)
gx init         # Create a manifest and lock file from your current workflows
gx audit        # Check locked actions against security advisories (requires GITHUB_TOKEN, --json for CI)
```

All of these cover your workflows and any composite actions in `.github/actions`.

`gx lint` and `gx audit` answer different questions. Lint judges your code against rules
you own: it is fully offline, and its verdict changes only when you edit a file — safe for
a pre-commit hook. Audit judges the world's knowledge about your dependencies, so the same
commit can be clean today and vulnerable tomorrow. It reads `.github/gx.lock` and requires
a token; without one it fails loudly rather than reporting a false "clean".

## Already using another tool?

gx works alongside your existing setup.

| If you use… | gx adds… |
|---|---|
| No tool | SHA pinning, version upgrades, lint, and a manifest to keep your team in sync |
| Renovate | A local CLI (no bot/PR required), lint checks, and a manifest/lock system for auditing |
| Dependabot | Initial SHA pinning ([not yet supported](https://github.com/dependabot/dependabot-core/issues/7913) by Dependabot), lint, and a manifest/lock system |
| ratchet | A manifest/lock system for team reproducibility, and standard version comments (no `# ratchet:` prefix) |
| pinact | A manifest/lock system for team reproducibility |

Running Renovate too? It catches majors; in-range advancement is `gx upgrade`'s job. See [docs/renovate.md](docs/renovate.md) for how the two fit together.

## Configuration

gx works with no configuration. Run `gx tidy` and your workflows are pinned.

For teams that want reproducibility, `gx init` creates a manifest (`.github/gx.toml`) and lock file (`.github/gx.lock`) that track every pinned action. See the [documentation](https://deepwiki.com/gmeligio/gx) for details on the manifest format and overrides.

## FAQ

<details>
<summary>Do I need a GITHUB_TOKEN?</summary>

For `gx tidy`, `gx upgrade`, and `gx init`: no, but without one you're limited to 60 GitHub API requests per hour. For most projects that's enough. Set `GITHUB_TOKEN` for CI or large repos.

For `gx audit`: yes, always. It queries GitHub's GraphQL API, which rejects unauthenticated requests, so without a token it exits non-zero rather than reporting a clean audit it never performed. `gx lint` never needs one — it makes no network requests at all.

</details>

<details>
<summary>How do I use gx in CI?</summary>

Add `gx lint` to your workflow to enforce pinning on every PR:

```yaml
- name: Check action pins
  run: gx lint
```

To keep actions current, copy [docs/gx-upgrade.yml](docs/gx-upgrade.yml) into `.github/workflows/`. It runs
`gx upgrade` on a schedule and opens a PR with the changes. See [docs/upgrade-workflow.md](docs/upgrade-workflow.md).

`gx audit` belongs on a schedule for the same reason, but answers a different question: its
verdict changes as advisories are published, not as you edit code, so a nightly run catches a
dependency that went bad since your last commit.

```yaml
- name: Audit locked actions
  run: gx audit
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

</details>

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for setup instructions and guidelines. Questions? [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/gmeligio/gx) [![Ask NotebookLM](https://img.shields.io/badge/NotebookLM-000000.svg?style=for-the-badge&logo=NotebookLM&logoColor=white)](https://notebooklm.google.com/notebook/0e1bc78e-7f6b-4781-b2b1-17e5afc1dd19)

## License

[MIT](LICENSE.md)
