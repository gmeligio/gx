# gx

Package manager for GitHub Actions.

## Usage

### tidy

This is the command that you'll run most of the time. It doesn't require configuration.

Update pinned SHAs when action versions change in your workflows. If the `gx.toml` file exists, ensure that it matches the Github workflows code. It adds any missing action requirements and it removes actions not required anymore. Also add any missing entries to gx.lock and removes unnecessary entries.

```bash
gx tidy
```

### init

You should run `gx init` when you want to have reproducible runs of gx commands. That reproducibility is very helpful when multiple people are working on the same project.

Pin all GitHub Actions in your workflows to their current SHA. Create `gx.toml` and `gx.lock` files if they don't exist.

```bash
gx init
```

## Options

- `-v, --verbose` - Enable verbose output
