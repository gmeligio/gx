## Context

After `describe-sha-trait-method`, `VersionRegistry::describe_sha` returns tags, repository, and date for a commit SHA in one efficient call. However, tidy phases still query the same SHAs redundantly:

1. `sync_manifest_actions` → `correct_version` → calls `tags_for_sha` (or `describe_sha`)
2. `upgrade_sha_versions_to_tags` → calls `tags_for_sha` (same SHAs, skipped if already corrected)
3. `update_lock` → `resolve_from_sha` → calls `describe_sha` (same SHAs again)

Each phase creates its own `ActionResolver`, discarding accumulated knowledge. The domain needs a structure that persists across phases.

## Goals / Non-Goals

**Goals:**
- Introduce `ShaIndex` as a domain entity that accumulates `ShaDescription` results during a plan run
- Eliminate all duplicate registry calls for the same `(ActionId, CommitSha)` pair across phases
- Keep `ActionResolver` stateless — pure transformation logic
- Keep the registry dumb — no caching or intelligence at the infra boundary

**Non-Goals:**
- Parallelizing registry calls across multiple actions (future, orthogonal)
- Caching across separate `plan()` invocations (each plan run starts fresh)
- Changing `lookup_sha` or `all_tags` call patterns (these are forward-path operations, not SHA-first)

## Decisions

### 1. ShaIndex is a standalone domain entity, not part of ActionResolver

**Decision**: `ShaIndex` is a separate struct passed alongside `ActionResolver`, not embedded in it.

**Rationale**: `ActionResolver` is a stateless service that combines registry calls and derives results. `ShaIndex` is stateful knowledge that accumulates during a plan run. Mixing them violates single responsibility. Keeping them separate means:
- `ActionResolver` stays easy to test (no state to manage)
- `ShaIndex` has a clear lifecycle: created at plan start, discarded at plan end
- Methods explicitly show their dependencies: `(resolver, sha_index)` vs hidden state

### 2. get_or_describe is the single entry point for SHA knowledge

**Decision**: `ShaIndex::get_or_describe(&mut self, registry, id, sha) -> Result<&ShaDescription>` is the only method that calls the registry. `ActionResolver` SHA methods and phase functions that need tag knowledge delegate to it.

**Rationale**: One place to decide "do I already know this?" avoids scattered cache-check logic. The method name makes the contract clear: get if known, describe if not.

### 3. resolve_from_sha and correct_version gain a &mut ShaIndex parameter; refine_version is deleted

**Decision**: `resolve_from_sha` and `correct_version` gain a `&mut ShaIndex` parameter. `refine_version` is deleted — it has no callers in production code. `resolve` (forward path, tag → SHA) is unchanged.

**Rationale**: Only SHA-first methods need the index. The forward path (`resolve`) takes a version, not a SHA — it doesn't benefit from SHA knowledge. `refine_version` was dead code; its logic (find best tag for a SHA) is handled by `upgrade_sha_versions_to_tags` which now uses `ShaIndex` directly.

### 3a. upgrade_sha_versions_to_tags uses ShaIndex directly

**Decision**: `upgrade_sha_versions_to_tags` calls `sha_index.get_or_describe(resolver.registry(), id, sha)` to get tags, then picks the best tag with `select_most_specific_tag`. It does not go through an `ActionResolver` method.

**Rationale**: This phase function's logic is "get tags for a SHA, pick the best one, update the manifest." It doesn't need the resolver's transformation logic — just tag knowledge from ShaIndex. Going through `get_or_describe` directly is simpler and ensures the description is cached for later phases (e.g., `update_lock` → `resolve_from_sha`).

### 4. tidy::plan() creates one resolver, threads it with sha_index

**Decision**: `plan()` creates `ActionResolver::new(registry)` once and passes `&resolver` + `&mut sha_index` to all phase functions. Phase functions no longer take `&R` directly or create their own resolvers.

**Rationale**: This makes the data flow explicit. The resolver is borrowed immutably (it's stateless), the sha_index is borrowed mutably (it accumulates). The ownership is clear.

### 5. ShaIndex uses &mut self, not RefCell

**Decision**: `get_or_describe` takes `&mut self` since there is no concurrent access.

**Rationale**: Single-threaded CLI. `RefCell` would add runtime borrow-checking overhead for no benefit. `&mut self` communicates the mutation honestly at the type level.

## Risks / Trade-offs

- **[Signature changes]** `resolve_from_sha` and `correct_version` gain a `&mut ShaIndex` parameter. All tidy phase functions gain `&ActionResolver<R>` + `&mut ShaIndex`. → Mechanical change, no logic changes. Tests update straightforwardly.
- **[refine_version deletion]** Removing dead code. No callers exist — `upgrade_sha_versions_to_tags` was duplicating its logic inline. → Clean removal, no behavior change.
- **[ShaIndex owns registry interaction for SHAs]** The resolver and phase functions no longer call `describe_sha` or `tags_for_sha` directly — they go through `ShaIndex`. This means the index mediates between all SHA-first consumers and the registry. → This is intentional: it's the single point of deduplication. The resolver still calls `lookup_sha` directly for the forward path.
