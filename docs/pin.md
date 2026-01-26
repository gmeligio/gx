# pin command

The `pin` command updates all GitHub Actions in your workflows to match the versions specified in your manifest file.

## Usage

```bash
gx pin
```

## What it does

1. Reads action versions from `.github/gx.toml`
2. Scans all workflow files in `.github/workflows/`
3. Updates action references to the pinned versions
4. Reports which workflows were updated and what changed

## Example

Given this manifest (`.github/gx.toml`):

```toml
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
```

And this workflow (`.github/workflows/ci.yml`):

```yaml
steps:
  - uses: actions/checkout@v3
  - uses: actions/setup-node@v3
```

Running `gx pin` will update the workflow to:

```yaml
steps:
  - uses: actions/checkout@v4
  - uses: actions/setup-node@v4
```

## Output

The command shows:
- Number of actions loaded from the manifest
- Each workflow file that was updated
- Specific actions that were changed in each file
- Total count of updated workflows

If no actions are defined in the manifest, the command exits without making changes.
