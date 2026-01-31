# gx

CLI to manage GitHub Actions dependencies.

## Usage

### freeze

Pin all GitHub Actions in your workflows to their current SHA:

```bash
gx freeze
```

### tidy

Update pinned SHAs when action versions change in your workflows. If the `gx.toml` file exists, it will ensure that it matches the Github workflows code. It adds any missing action requirements and it removes actions not required anymore. It also adds any missing entries to gx.lock and removes unnecessary entries.

```bash
gx tidy
```

## Options

- `-v, --verbose` - Enable verbose output
