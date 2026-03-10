## Why

`GithubRegistry::fetch_ref` returns the raw `object.sha` from the GitHub refs API without checking `object.type`. For annotated tags, this is a **tag object SHA** — a git internal reference that is not a commit. This violates the `VersionRegistry::lookup_sha` contract which promises a `CommitSha` (a commit). The result: invalid workflow pins and lock entries that GitHub rejects with HTTP 422 when used in `uses: owner/repo@sha`.

## What Changes

- **Fix `fetch_ref`**: Check `object.type` after fetching a ref; when it is `"tag"` (annotated), dereference via `GET /git/tags/{sha}` to obtain the underlying commit SHA. Rename to `fetch_ref_commit` to make the contract explicit.
- **Delete `resolve_version_for_sha`**: Dead code (no production callers) with the same annotated tag bug. Remove it and its tests.
- **Remove internal types from public re-exports**: `GitObject`, `GitRef`, `GitRefEntry` are only used within `infra::github`; stop re-exporting them.

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `sha-first-resolution`: The registry contract for `lookup_sha` implicitly requires returning a real commit SHA. This change enforces that invariant at the infra boundary for annotated tags. No spec-level behavior change — the spec already assumes SHAs are commits.

## Impact

- `src/infra/github/resolve.rs` — main fix site (rename + dereference + delete dead method)
- `src/infra/github/mod.rs` — remove public re-exports
- `src/infra/github/tests.rs` — remove dead code test
- `tests/e2e_github.rs` — remove dead code e2e test
- Existing failing e2e tests will pass after the fix
