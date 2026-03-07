## Why

GitHub natively exposes immutable SHA256 digests for all uploaded release assets (since June 2025), making cargo-dist's `.sha256` sidecar files redundant. The Homebrew formula already has SHA256 hashes baked in at build time by cargo-dist, so the sidecar files serve no verification purpose. Removing them reduces release artifact noise.

## What Changes

- Disable cargo-dist's checksum sidecar file generation by setting `checksum = "false"` in `dist-workspace.toml`.
- Remove per-artifact `.sha256` files and the unified `sha256.sum` file from future releases.
- If the known cargo-dist bug ([#1963](https://github.com/axodotdev/cargo-dist/issues/1963)) causes a `false.sum` file to still be generated, add a CI cleanup step to remove it.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `pr-based-homebrew-publish`: The Homebrew formula generation is unaffected (SHA256 is computed from the built artifact, not from sidecar files), but the release artifacts it references will no longer have companion `.sha256` files.

## Impact

- **Config**: `dist-workspace.toml` — new `checksum` field.
- **CI**: `.github/workflows/release.yml` — possible cleanup step if `false.sum` bug is present.
- **Release artifacts**: Future releases will have fewer files (no `.sha256` sidecars, no `sha256.sum`).
- **Downstream consumers**: Users who relied on sidecar `.sha256` files for verification should use the GitHub API or UI instead.
