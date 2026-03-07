## Context

cargo-dist generates `.sha256` sidecar files and a unified `sha256.sum` for every release artifact. Since June 2025, GitHub natively exposes immutable SHA256 digests for all uploaded release assets via UI, REST API, GraphQL API, and `gh` CLI. The Homebrew formula has SHA256 baked in at build time by cargo-dist, independent of sidecar files. The sidecar files are redundant.

## Goals / Non-Goals

**Goals:**
- Remove redundant `.sha256` sidecar files and `sha256.sum` from future releases.
- Reduce release artifact clutter.

**Non-Goals:**
- Changing how cargo-dist bakes SHA256 into the Homebrew formula (that stays as-is).
- Adding CI steps to query GitHub's SHA256 API (not needed — the formula already works without sidecar files).
- Removing checksums from past releases.

## Decisions

**Set `checksum = "false"` in `dist-workspace.toml`.**
This is cargo-dist's built-in config to disable sidecar checksum generation. The `false.sum` bug (cargo-dist #1963) has been fixed in recent versions.

No CI workflow changes are needed — the `build-global-artifacts` job that generates checksums will simply produce fewer files.

## Risks / Trade-offs

- [Users relying on sidecar `.sha256` files] → Unlikely for this project's scale. GitHub's native checksums are a better alternative. Past releases retain their sidecar files.
