## Context

The GitHub refs API (`GET /repos/{owner}/{repo}/git/ref/tags/{name}`) returns a `GitRef` with an `object` containing `sha` and `type`. For **lightweight tags**, `object.type` is `"commit"` and `object.sha` is the commit SHA directly. For **annotated tags**, `object.type` is `"tag"` and `object.sha` is a tag object SHA — a git internal reference that must be dereferenced via `GET /git/tags/{sha}` to reach the underlying commit.

`fetch_ref` currently returns `object.sha` without checking `object.type`. This is the sole entry point for tag/branch → SHA resolution in `resolve_ref`, which feeds `VersionRegistry::lookup_sha`. All downstream consumers (lock entries, workflow pins) receive the wrong SHA for annotated tags.

The codebase already handles annotated tag dereferencing correctly in `get_tags_for_sha` (reverse lookup: commit SHA → tag names). The fix reuses the same pattern.

## Goals / Non-Goals

**Goals:**
- `fetch_ref` always returns a commit SHA, regardless of tag type (lightweight or annotated)
- Rename to `fetch_ref_commit` to make the contract explicit
- Remove dead code (`resolve_version_for_sha`) that has the same bug
- Remove internal response types from public re-exports

**Non-Goals:**
- Introducing new domain types (e.g., `GitObjectSha` vs `CommitSha`) — the domain model is correct; only the infra violates the contract
- Changing the `VersionRegistry` trait or `ActionResolver`
- Changing how `get_tags_for_sha` works (it's already correct)

## Decisions

### 1. Fix at `fetch_ref` level, not `resolve_ref`

**Decision**: Add dereferencing inside `fetch_ref` (renamed `fetch_ref_commit`), not in its caller `resolve_ref`.

**Rationale**: `fetch_ref` is the single point where a ref's `object.sha` is extracted. Fixing here means all callers (tag resolution and branch resolution paths in `resolve_ref`) automatically get commit SHAs. No caller of `fetch_ref` ever wants a tag object SHA.

**Alternative considered**: Fix in `resolve_ref` after the `fetch_ref` call. Rejected because it duplicates the check across two call sites and leaves `fetch_ref` as a footgun for future callers.

### 2. Inline dereferencing rather than reusing `dereference_tag`

**Decision**: Add the dereference logic directly in `fetch_ref_commit` rather than calling the existing `dereference_tag` method.

**Rationale**: `dereference_tag` takes a `GitRefEntry` and a target `commit_sha` to match against — it's designed for the reverse lookup use case (does this tag point to a specific commit?). In `fetch_ref_commit` we don't have a `GitRefEntry` or a target commit SHA to match; we just need to follow the tag object to its commit. The logic is simple: `GET /git/tags/{sha}` → return `object.sha`.

### 3. Delete `resolve_version_for_sha`

**Decision**: Delete entirely rather than fix.

**Rationale**: Zero production callers. Only used in unit tests and one e2e test we just wrote to demonstrate the bug. The functionality it provides (find which tag points to a SHA) is already available through `get_tags_for_sha` which correctly handles annotated tags.

### 4. Remove `GitObject`, `GitRef`, `GitRefEntry` from public re-exports

**Decision**: Change `pub use` to `pub(super) use` or remove the re-export entirely.

**Rationale**: These types are GitHub API response structures. They're only used within `infra::github` (resolve.rs and tests.rs). Re-exporting them leaks infra implementation details.

## Risks / Trade-offs

- **Extra API call for annotated tags** → One additional `GET /git/tags/{sha}` per annotated tag resolution. This is unavoidable — the GitHub refs API doesn't provide the commit SHA directly for annotated tags. The same pattern is already used in `get_tags_for_sha`.
- **Behavioral change for `fetch_tag_date`** → After the fix, `lookup_sha` passes the commit SHA (not tag object SHA) to `fetch_tag_date`. But `fetch_tag_date` calls `GET /git/tags/{sha}` which expects a tag object SHA, not a commit SHA. This will return 404 and fall through to `fetch_commit_date`, which is correct behavior — the date is still obtained, just via a different path. No fix needed.
