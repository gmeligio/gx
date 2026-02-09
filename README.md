# gx

Package manager for GitHub Actions.

## Usage

### freeze

Pin all GitHub Actions in your workflows to their current SHA. Create `gx.toml` and `gx.lock` files if they don't exist.

```bash
gx freeze
```

### tidy

Update pinned SHAs when action versions change in your workflows. If the `gx.toml` file exists, ensure that it matches the Github workflows code. It adds any missing action requirements and it removes actions not required anymore. Also add any missing entries to gx.lock and removes unnecessary entries.

```bash
gx tidy
```

## Options

- `-v, --verbose` - Enable verbose output
