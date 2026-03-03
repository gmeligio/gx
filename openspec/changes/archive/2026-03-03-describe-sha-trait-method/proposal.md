## Why

The `VersionRegistry` trait lacks a unified operation for "describe this commit SHA." The SHA-first resolution path (`resolve_from_sha`) currently calls `lookup_sha(id, SHA_as_version)` — which wastefully tries tag/branch/commit endpoints sequentially — then separately calls `tags_for_sha`. This models two separate questions when the domain needs one: "given a trusted SHA, tell me about it." The result is 5-15 API calls per action where 2 would suffice, making `init` appear to hang on repos with many actions.

## What Changes

- Add a `describe_sha` method to the `VersionRegistry` trait that accepts a trusted `CommitSha` and returns tags, repository, and date in one operation
- Add a `ShaDescription` domain type to carry the result
- Rewrite `ActionResolver::resolve_from_sha` to use `describe_sha` instead of `lookup_sha` + `tags_for_sha`
- Implement `describe_sha` in `GithubRegistry` going directly to `/commits/{sha}` (1 call) + tag lookup, skipping the 3-endpoint `resolve_ref` fallback

## Capabilities

### New Capabilities

- `sha-description`: Domain operation for describing a commit SHA — returns tags, repository, and date in a single trait method, replacing the compound `lookup_sha` + `tags_for_sha` pattern for SHA-first resolution.

### Modified Capabilities

- `sha-first-resolution`: `resolve_from_sha` changes its internal implementation to use `describe_sha` instead of `lookup_sha` + `tags_for_sha`. External behavior (inputs/outputs) is unchanged.

## Impact

- `src/domain/resolution.rs` — new type, new trait method, resolver rewrite
- `src/infrastructure/github.rs` — new `GithubRegistry` implementation
- All `VersionRegistry` implementors (7 total: 1 production, 6 test mocks) must add the new method
- No breaking changes to CLI behavior — same inputs produce same outputs, just faster
