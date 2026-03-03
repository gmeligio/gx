## Context

The `VersionRegistry` trait defines three operations: `lookup_sha` (forward: version → SHA + metadata), `tags_for_sha` (reverse: SHA → tags), and `all_tags` (list all tags). The SHA-first resolution path in `ActionResolver::resolve_from_sha` calls both `lookup_sha` and `tags_for_sha` to build a complete picture of a commit. However, `lookup_sha` was designed for the forward path (tag → SHA), so when called with a raw SHA as the "version", it goes through `resolve_ref` which tries tag/branch/commit endpoints sequentially — 3 HTTP calls where 1 would suffice.

The SHA is the source of truth in gx's security model. The domain needs a first-class operation that starts from a trusted SHA.

## Goals / Non-Goals

**Goals:**
- Add a `describe_sha` method to `VersionRegistry` that returns tags, repository, and date for a known SHA in one call
- Rewrite `resolve_from_sha` to use `describe_sha`, eliminating the wasteful `lookup_sha(SHA_as_version)` pattern
- Reduce GitHub API calls per SHA-first action from ~5-15 to ~2 (commit lookup + tag lookup)

**Non-Goals:**
- Parallelizing API calls across multiple actions (future optimization, orthogonal to this change)
- Caching `describe_sha` results to deduplicate between `correct_version` and `resolve_from_sha` (separate concern)
- Changing `correct_version` or `refine_version` — they only need `tags_for_sha` and work fine as-is
- GraphQL migration — different wire protocol, separate change

## Decisions

### 1. New trait method rather than refactoring `lookup_sha`

**Decision**: Add `describe_sha` as a new method alongside existing methods.

**Rationale**: `lookup_sha` serves the forward path (tag/branch → SHA) correctly. It's used by `ActionResolver::resolve` for tag-based resolution and by the upgrade flow. Changing its semantics would break those paths. A new method cleanly models the distinct domain question.

**Alternative considered**: Making `lookup_sha` detect SHA inputs and short-circuit. Rejected because it conflates two distinct operations and makes the trait less clear.

### 2. `ShaDescription` as a dedicated return type

**Decision**: New `ShaDescription { tags, repository, date }` type rather than reusing `ResolvedRef`.

**Rationale**: `ResolvedRef` carries `sha` and `ref_type`, which are meaningless for `describe_sha` — the caller already has the SHA, and `ref_type` is derived from whether tags exist. A dedicated type avoids confusion and makes the API self-documenting.

### 3. `GithubRegistry` implementation goes directly to `/commits/{sha}`

**Decision**: Skip `resolve_ref` entirely. Call `fetch_commit_date` (1 API call) + `get_tags_for_sha` (paginated, existing code).

**Rationale**: We know it's a commit SHA. There's no need to try the tag and branch ref endpoints first. The commit endpoint validates the SHA exists and returns the date in one call.

### 4. Tags error in `describe_sha` is non-fatal

**Decision**: If `get_tags_for_sha` fails, return empty tags rather than propagating the error. The commit date fetch failing IS fatal.

**Rationale**: Matches current behavior in `resolve_from_sha` where `tags_for_sha` uses `unwrap_or_default()`. The SHA is the source of truth — if we can confirm it exists (via commit lookup), the entry is valid even without tag information.

## Risks / Trade-offs

- **[Trait expansion]** Adding a method to `VersionRegistry` requires updating 7 implementations (1 production, 6 test mocks). → Low risk: all mocks are simple, mechanical additions.
- **[Date source difference]** `lookup_sha` for tags tries release date → tag date → commit date. `describe_sha` only gets commit date. → Acceptable: SHA-first entries are commits, not tags. The commit date is the correct date for the commit itself. Tag/release dates are for the tag-based forward path.
