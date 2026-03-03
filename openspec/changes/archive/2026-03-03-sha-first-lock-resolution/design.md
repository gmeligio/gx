## Context

The lock file's source of truth is the SHA. All other lock fields (version, ref_type, date) should be derived from it. Currently, `populate_lock_entry` inverts this: it uses the manifest version as a lookup key to find a SHA via the GitHub API. This causes incorrect results when floating tags (like `v3`) have been moved by maintainers to point at newer releases.

The workflow files already contain the correct SHA (e.g., `@6d1e696... # v3`). This SHA is parsed into `LocatedAction.sha` during workflow scanning but is never passed to the lock resolution phase.

## Goals / Non-Goals

**Goals:**
- Lock resolution uses the workflow SHA when available, deriving version and metadata from it.
- `select_best_tag` is fixed to prefer the most specific tag (most components), matching the lock-format spec's definition of the `version` field.
- The `refine_version` post-step in `populate_lock_entry` is removed — version is set during resolution.
- Init remains a variant of tidy (same `plan()` code path), no special-casing.

**Non-Goals:**
- Changing how the manifest version is determined (comment extraction, dominant version selection).
- Changing the `correct_version` guard that keeps the original version when it's already in the tag list.
- Changing the upgrade command's tag selection logic (it has its own candidate filtering).

## Decisions

### D1: Fix `select_best_tag` to prefer most specific (no split needed)

During exploration we considered splitting into `select_broadest_tag` and `select_most_specific_tag`. On closer analysis, no caller actually needs the broadest tag:

- **`correct_version`** (manifest): Has its own `tags.contains(original_version)` guard that keeps the original when valid. The `select_best_tag` fallback only fires when the original is wrong (e.g., comment says `v4` but SHA points to `v3.x` tags). In that case, most specific is correct.
- **`upgrade_sha_versions_to_tags`** (manifest): Upgrades bare SHAs. The existing spec scenario says `[v4, v4.2.1]` → `v4.2.1` (most specific).
- **`refine_version`** (lock): Most specific by definition.

**Decision**: Fix `select_best_tag` in place to prefer more components. Rename to `select_most_specific_tag` for clarity. No second function needed.

**Sort order change:**
```
Before: fewer components first (ascending len), then highest version
After:  more components first (descending len), then highest version
```

### D2: Add `resolve_from_sha` to `ActionResolver`

New method on `ActionResolver` that builds a `ResolvedAction` from a known SHA:

```rust
pub fn resolve_from_sha(
    &self,
    id: &ActionId,
    sha: &CommitSha,
) -> Result<ResolvedAction, ResolutionError> {
    // 1. Get metadata (repo, date) via existing lookup_sha
    //    (resolve_ref returns immediately for valid SHAs, then fetches commit date)
    let meta = self.registry.lookup_sha(id, &Version::from(sha.as_str()))?;

    // 2. Get tags → most specific version
    let tags = self.registry.tags_for_sha(id, sha).unwrap_or_default();
    let version = select_most_specific_tag(&tags)
        .unwrap_or_else(|| Version::from(sha.as_str()));

    // 3. Determine ref_type from tags
    let ref_type = if tags.is_empty() { RefType::Commit } else { RefType::Tag };

    Ok(ResolvedAction::new(id.clone(), version, sha.clone(), meta.repository, ref_type, meta.date))
}
```

No new `VersionRegistry` trait methods needed. Uses existing `lookup_sha` (which handles SHA inputs via `resolve_ref`) and `tags_for_sha`.

### D3: Build workflow SHA map and pass to lock resolution

In `plan()`, build a `HashMap<LockKey, CommitSha>` from the `located` actions:

```rust
let workflow_shas: HashMap<LockKey, CommitSha> = located.iter()
    .filter_map(|loc| {
        let sha = loc.sha.as_ref()?;
        let manifest_version = planned_manifest.get(&loc.id)?;
        let key = LockKey::new(loc.id.clone(), manifest_version.clone());
        Some((key, sha.clone()))
    })
    .collect();
```

The key uses the **manifest version** (not the located version) because that's what the lock key is based on. For init, these are the same. For tidy with overrides, the override version produces its own lock key.

Pass `&workflow_shas` into `update_lock` → `populate_lock_entry`.

### D4: Rewrite `populate_lock_entry` as SHA-first

```rust
fn populate_lock_entry<R>(lock, resolver, spec, workflow_shas, unresolved) {
    let key = LockKey::from(spec);

    // Skip if already complete
    if lock.get(&key).is_some_and(|e| e.is_complete(&spec.version)) {
        return;
    }

    // Resolve: SHA-first when available, version-fallback otherwise
    if !lock.has(&key) {
        let result = if let Some(sha) = workflow_shas.get(&key) {
            resolver.resolve_from_sha(&spec.id, sha)
        } else {
            resolver.resolve(spec)  // existing version → SHA path
        };

        match result {
            Ok(action) => lock.set(&action),
            Err(e) => { unresolved.push(...); return; }
        }
    }

    // Specifier always derived from manifest version
    if let Some(_entry) = lock.get(&key) {
        lock.set_specifier(&key, spec.version.specifier());
    }
}
```

The `refine_version` post-step is removed. For the SHA-first path, `resolve_from_sha` already sets the most specific version. For the version-fallback path, `resolve` sets version from the spec, and we rely on the lock entry being self-consistent.

### D5: Simplify `resolve` return type for consistency

Currently `resolve` returns `ResolutionResult` (an enum with Resolved/Corrected/Unresolved). The `Corrected` variant is never constructed by `resolve` — it was designed for `correct_version`. For the new `resolve_from_sha`, returning `Result<ResolvedAction, ResolutionError>` is cleaner and sufficient.

`resolve` can also be changed to return `Result<ResolvedAction, ResolutionError>` since the `Corrected` variant is unused in that path.

## Risks / Trade-offs

**[Risk] `resolve_from_sha` calls `lookup_sha` with SHA as version** → `resolve_ref` returns `RefType::Commit` for valid SHAs, but the actual ref may be a tag. Mitigated by overriding `ref_type` based on `tags_for_sha` result. The date will be commit date rather than release date, which is acceptable.

**[Risk] Override actions may not have workflow SHAs in the map** → The SHA map is keyed by `LockKey(id, manifest_version)`. Override versions produce different lock keys. If an override version appears in the workflow with a SHA, the located action carries it. If the override version doesn't appear in workflows (edge case), it falls back to registry resolution. This is correct behavior.
