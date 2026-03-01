## Context

After running `gx upgrade --latest`, the manifest changes from floating ranges like `"v1"` to exact tags like `"v1.1.2"`. This destroys the user's semver intent. The lock file also has incomplete entries — 5 of 9 entries are missing the `version` and `specifier` fields required by the lock-format v1.3 spec.

Root causes:
1. `find_upgrade_candidate` returns a raw tag, and the upgrade flow blindly writes it to the manifest regardless of whether it's in-range or cross-range.
2. `resolve_and_store` in upgrade.rs doesn't call `populate_resolved_fields`, so lock entries from upgrade lack `version`/`specifier`.
3. `tags_for_sha` failures are silently swallowed with no fallback for `resolved_version`.
4. The serializer conditionally omits `version`/`specifier` when `version` is None.
5. Pre-release candidates are included in bulk upgrades.

## Goals / Non-Goals

**Goals:**
- Manifest precision is preserved during upgrade (major stays major, minor stays minor)
- All lock entries always have all 6 fields per spec
- Pre-releases excluded from bulk upgrade candidates
- Upgrade populates lock metadata the same way tidy does

**Non-Goals:**
- Changing the `VersionRegistry` trait
- Adding pagination to `tags_for_sha` (separate concern)
- Adding pagination to `get_tags_for_sha` (separate concern)

## Decisions

### D1: `find_upgrade_candidate` returns `UpgradeAction` enum

Instead of returning `Option<Version>` (a raw tag), return a richer type that distinguishes in-range from cross-range:

```rust
pub enum UpgradeAction {
    /// Candidate is within the manifest's current range.
    /// Only the lock needs re-resolving; manifest stays unchanged.
    InRange { candidate: Version },
    /// Candidate is outside the manifest's range.
    /// Manifest must change. `new_manifest_version` preserves the original precision.
    CrossRange { candidate: Version, new_manifest_version: Version },
}
```

`find_upgrade_candidate` returns `Option<UpgradeAction>`.

**In-range detection:** Compare the candidate's major (and minor for patch precision) against the manifest version. If they match the precision-level constraint, it's in-range.

**Precision-preserving manifest version for cross-range:** Extract components from the candidate tag at the manifest's precision level:
- Major (`v1`) + candidate `v3.0.0` → `v3`
- Minor (`v0.5`) + candidate `v1.0.0` → `v1.0`
- Patch (`v1.15.2`) + candidate `v1.15.3` → `v1.15.3` (exact tag)

**Alternatives considered:**
- Returning a boolean `is_in_range` alongside the version — less type-safe, easy to ignore.
- Doing the in-range check in the upgrade command instead — scatters the logic.

### D2: Upgrade command uses `UpgradeAction` to decide manifest changes

```
for each UpgradeAction:
    match action:
        InRange { candidate }:
            // Don't touch manifest
            // Re-resolve the EXISTING manifest version to update lock
            resolve_and_store(manifest_spec, lock)
        CrossRange { candidate, new_manifest_version }:
            // Update manifest with precision-preserved version
            manifest.set(id, new_manifest_version)
            // Resolve the new manifest version
            resolve_and_store(new_manifest_spec, lock)
```

### D3: Share `populate_resolved_fields` between tidy and upgrade

Currently `populate_resolved_fields` is a private function in `tidy.rs`. Move it to a shared location (e.g., `domain/resolution.rs` or a new `domain/enrichment.rs`) so both tidy and upgrade can use it.

The function:
1. Calls `tags_for_sha` to find the most specific version tag for the resolved SHA
2. Falls back to the manifest version if no more-specific tag exists (NEW)
3. Computes `specifier` from the manifest version's precision

### D4: Fallback for `resolved_version`

When `tags_for_sha` fails or returns no matching tags, `resolved_version` falls back to the manifest version instead of staying None:

```
BEFORE:
    if let Ok(tags) = registry.tags_for_sha(id, sha) {
        action.resolved_version = Version::highest(&tags);
    }
    // resolved_version stays None on error or empty tags

AFTER:
    let resolved = if let Ok(tags) = registry.tags_for_sha(id, sha) {
        Version::highest(&tags)
    } else {
        None
    };
    action.resolved_version = Some(resolved.unwrap_or_else(|| action.version.clone()));
```

This satisfies the lock-format spec scenario: "No more specific tag exists → version = manifest version".

### D5: Unconditional serialization of all 6 fields

The serializer always outputs `version` and `specifier`:

```
BEFORE:
    if let Some(version) = &entry.version {
        // include version + specifier
    } else {
        // only 4 fields
    }

AFTER:
    let version = entry.version.as_deref()
        .unwrap_or(key.version.as_str());  // fallback to lock key version
    let specifier = entry.specifier.as_deref().unwrap_or("");
    // always write all 6 fields
```

### D6: Fix `Version::precision()` for pre-releases

`precision()` currently returns `None` for pre-release versions because it splits on `.` and gets 4+ parts (e.g., `"3.0.0-beta.2"` → `["3", "0", "0-beta", "2"]`). This makes pre-release manifests un-upgradeable and prevents `specifier()` from working.

**Fix:** Strip the pre-release suffix (everything after the first `-`) before counting components:

```rust
pub fn precision(&self) -> Option<VersionPrecision> {
    let stripped = self.0.strip_prefix('v')
        .or_else(|| self.0.strip_prefix('V'))
        .unwrap_or(&self.0);
    // Strip pre-release suffix before counting components
    let base = stripped.split('-').next().unwrap_or(stripped);
    let parts: Vec<&str> = base.split('.').collect();
    // ... rest unchanged
}
```

Results:
- `v3.0.0-beta.2` → base `3.0.0` → Patch → specifier `~3.0.0-beta.2`
- `v3.0-rc.1` → base `3.0` → Minor → specifier `^3.0-rc.1`
- `v3-alpha` → base `3` → Major → specifier `^3-alpha`

Note: `specifier()` continues to use the full stripped string (including pre-release suffix) so the specifier accurately reflects the manifest version.

### D7: Renovate-like pre-release handling in `find_upgrade_candidate`

Pre-release handling follows the Renovate pattern: stable candidates are always preferred, pre-releases are only considered as fallback when the manifest itself is a pre-release.

**Rules:**
- **Stable manifest** → exclude all pre-release candidates
- **Pre-release manifest** → include both stable and pre-release candidates, but prefer stable

**Implementation:** Single-pass with a stability-preferring comparator. No new parameters needed.

```rust
let manifest_is_prerelease = !manifest_semver.pre.is_empty();

// In the filter: stable manifest excludes pre-releases entirely
.filter_map(|c| {
    let parsed = parse_semver(c.as_str())?;
    if !manifest_is_prerelease && !parsed.pre.is_empty() {
        return None;
    }
    // ... existing floor/range checks ...
    Some((c.clone(), parsed))
})
// Prefer stable over pre-release, then highest version
.max_by(|(_, a), (_, b)| {
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, false) => Ordering::Greater,  // stable always wins
        (false, true) => Ordering::Less,
        _ => a.cmp(b),                       // same stability: highest wins
    }
})
```

**Examples:**
- Stable `v4` + candidates `[v5, v5.1.0-beta]` → `v5` (pre-release filtered out)
- Pre-release `v3.0.0-beta.2` + candidates `[v3.0.0, v3.1.0-dev.1]` → `v3.0.0` (stable preferred)
- Pre-release `v3.1.0-dev.1` + candidates `[v3.1.0-dev.2]` → `v3.1.0-dev.2` (no stable exists)
- Pre-release `v3.0.1-insiders.1` + candidates `[v3.0.1, v3.0.2-insiders.1]` → `v3.0.1` (stable preferred)

Pinned mode (`gx upgrade action@v3.0.0-beta.2`) bypasses `find_upgrade_candidate` entirely, so pre-release pins are always supported.

**Alternatives considered:**
- Unconditional pre-release filter (Option C) — simpler but prevents upgrading pre-release manifests to newer pre-releases when no stable exists.
- `skip_prereleases` parameter — unnecessary since the behavior is fully determined by the manifest version.

## Risks / Trade-offs

- **In-range upgrades still need lock re-resolution** — For an in-range upgrade, the lock must be re-resolved to get the latest SHA. The manifest version (e.g., `v1`) might point to a different commit than the candidate tag (e.g., `v1.1.2`). This is fine: `resolve()` resolves the manifest version's ref, and `populate_resolved_fields` finds the most specific tag for that SHA.
- **Precision extraction from candidate tags** — Constructing `v3` from candidate `v3.0.0` requires string manipulation. Edge cases: candidate `v3` already has major precision (no extraction needed), candidate `v3.0.0-beta.2` has pre-release suffix. Handle by stripping to the required number of components.
