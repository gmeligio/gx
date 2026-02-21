[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/gmeligio/gx)
[![Ask NotebookLM](https://img.shields.io/badge/NotebookLM-000000.svg?style=for-the-badge&logo=NotebookLM&logoColor=white)](https://notebooklm.google.com/notebook/0e1bc78e-7f6b-4781-b2b1-17e5afc1dd19)

# gx

Package manager for Github Actions.

## Usage

### tidy

This is the command that you'll run most of the time. It doesn't require configuration.

Update pinned SHAs when action versions change in your workflows. If the `gx.toml` file exists, ensure that it matches the Github workflows code. It adds any missing action requirements and it removes actions not required anymore. Also add any missing entries to gx.lock and removes unnecessary entries.

```bash
gx tidy
```

### init

You should run `gx init` when you want to have reproducible runs of gx commands. That reproducibility is very helpful when multiple people are working on the same project.

Pin all Github Actions in your workflows to their current SHA. Create `gx.toml` and `gx.lock` files if they don't exist.

```bash
gx init
```

### upgrade

Check for newer versions of actions in your manifest and upgrade them. It resolves the upgraded versions to their commit SHAs. It updates the workflows based on the upgraded versions. It updates the `gx.toml` and `gx.lock` files if they exist.

Skips actions that are not semver-versioned (e.g., pinned to a commit SHA).

```bash
gx upgrade
```

## Options

- `-v, --verbose` - Enable verbose output
