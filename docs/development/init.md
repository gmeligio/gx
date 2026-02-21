# init command - Implementation

## Overview

The `init` command is a thin wrapper that validates preconditions, then delegates to `tidy::run` with file-backed stores.

## Code path

`src/main.rs` lines 66-74:

```rust
Commands::Init => {
    if manifest_path.exists() {
        anyhow::bail!("Already initialized. Use `gx tidy` to update.");
    }
    let registry = GithubRegistry::from_env()?;
    let manifest = FileManifest::load_or_default(&manifest_path)?;
    let lock = FileLock::load_or_default(&lock_path)?;
    commands::tidy::run(&repo_root, manifest, lock, registry)
}
```

## Key difference from tidy

- `init` always uses `FileManifest`/`FileLock` (never memory-only)
- `init` bails if `gx.toml` already exists
- The actual logic is identical to `tidy` — `tidy::run` handles scanning, resolving, and writing

## Flow

1. Check that `gx.toml` does not exist → bail if it does
2. Create `GithubRegistry` from `GITHUB_TOKEN` env var
3. Create empty `FileManifest` (since file doesn't exist, `load_or_default` returns default)
4. Create empty `FileLock` (same)
5. Delegate to `tidy::run` which scans workflows, populates manifest, resolves SHAs, saves both files, and updates workflows
