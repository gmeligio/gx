## Why

The `x86_64-apple-darwin` (Intel Mac) release target is the slowest build job on the release critical path (369s vs. ~240s for the next-slowest) running on the most expensive runner tier (macOS bills ~10.3× Linux), while being the least-used artifact — 4 lifetime downloads, zero since v0.6.0 (~3 months). Dropping it cuts release-build wall-clock by ~35% and removes the priciest minutes from every release, at no meaningful cost to users.

## What Changes

- **BREAKING**: Remove `x86_64-apple-darwin` from the `targets` array in `dist-workspace.toml`. Future releases will no longer ship an Intel-Mac binary tarball, and the Homebrew formula will no longer serve an Intel-Mac install (Apple-Silicon macOS, Linux, and Windows are unaffected).
- macOS distribution becomes Apple-Silicon-only (`aarch64-apple-darwin`). Intel-Mac users can build from source via `cargo install`.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `homebrew-release`: Add a requirement that macOS distribution targets Apple Silicon only. This is the one user-traceable effect of an otherwise packaging-level change — it makes the supported-macOS-architecture set explicit so an Intel-Mac user understands `brew install` will not offer a binary and must build from source.

## Impact

- `dist-workspace.toml` — one line: remove `"x86_64-apple-darwin"` from `targets`.
- `.github/workflows/release.yml` — **no change**. The build matrix is computed at runtime by `dist` from `targets`; the workflow file hardcodes no target triples (verified: `dist generate` produces a byte-identical `release.yml` after the edit).
- Homebrew tap `gmeligio/homebrew-tap` — the next release's generated `gx.rb` will drop the `Hardware::CPU.intel?` macOS branch automatically. No manual tap edit.
- Existing releases retain their Intel-Mac assets; only future releases are affected.
