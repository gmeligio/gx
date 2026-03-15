# Design: Two-Tier Lock File

## Overview

Split the lock from a single `HashMap<Spec, Entry>` into two maps: resolutions (specifier → version) and actions (version → metadata). This enables specifier changes to reuse existing resolutions and deduplicates action metadata across overlapping specifiers.

## Key Decisions

### D1: New `Resolution` domain type

**Decision**: Add a `Resolution` struct to the lock domain:

```rust
pub struct Resolution {
    pub version: Version,
    pub comment: String,
}
```

**Justification**: This is the value type for the resolutions tier. It holds the resolved version (using the existing `Version` newtype) and the specifier-dependent comment. The comment belongs here rather than in the action entry because the same version can have different comments depending on specifier precision.

**Location**: `src/domain/lock/resolution.rs`, alongside `entry.rs`.

### D2: New `ActionKey` for tier 2

**Decision**: The tier 2 key is `(ActionId, Version)`. Rather than using a raw tuple in the `HashMap`, define a key struct:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionKey {
    pub id: ActionId,
    pub version: Version,
}
```

**Justification**: A named struct is more readable than `(ActionId, Version)` in type signatures and error messages. `Version` needs `Hash` and `Eq` derives (it already has them).

**Location**: `src/domain/lock/mod.rs` or a new `src/domain/lock/action_key.rs`.

### D3: Restructured `Lock`

**Decision**: The `Lock` struct becomes:

```rust
pub struct Lock {
    pub resolutions: HashMap<Spec, Resolution>,
    pub actions: HashMap<ActionKey, Commit>,
}
```

**Justification**: The existing `Entry` struct had `commit: Commit`, `version: Option<String>`, and `comment: String`. In the two-tier model:
- `comment` moves to `Resolution` (depends on specifier precision, not version).
- `version` is redundant with `ActionKey.version` (the key carries the resolved version).
- What remains is just `Commit` (sha, repository, ref_type, date).

So the action map value reuses the existing `Commit` struct directly. No new `ActionEntry` type is needed. The `Entry` struct can be removed.

### D4: Lock API

**`get(spec)` → two-step lookup**:
```rust
pub fn get(&self, spec: &Spec) -> Option<(&Resolution, &Commit)> {
    let resolution = self.resolutions.get(spec)?;
    let key = ActionKey { id: spec.id.clone(), version: resolution.version.clone() };
    let entry = self.actions.get(&key)?;
    Some((resolution, entry))
}
```

Returns both the resolution and the commit metadata. Callers that only need the SHA can destructure.

**`set(spec, version, commit, comment)` → populates both tiers**:
```rust
pub fn set(&mut self, spec: &Spec, version: Version, commit: Commit, comment: String) {
    self.resolutions.insert(spec.clone(), Resolution { version: version.clone(), comment });
    let action_key = ActionKey { id: spec.id.clone(), version };
    self.actions.insert(action_key, commit);
}
```

The old `set(resolved)` + `set_version()` + `set_comment()` pattern is replaced by a single call that takes all values upfront. Callers (in `lock_sync.rs` and `upgrade/plan.rs`) already have all these values available — they just need to pass them differently.

**`is_complete(spec)` → checks both tiers**:
```rust
pub fn is_complete(&self, spec: &Spec) -> bool {
    let Some(resolution) = self.resolutions.get(spec) else { return false };
    if resolution.version.as_str().is_empty() { return false; }
    if resolution.comment != spec.version.to_comment() { return false; }
    let key = ActionKey { id: spec.id.clone(), version: resolution.version.clone() };
    let Some(commit) = self.actions.get(&key) else { return false };
    !commit.sha.as_str().is_empty()
        && !commit.repository.is_empty()
        && commit.ref_type.is_some()
        && !commit.date.is_empty()
}
```

**`retain(specs)` → prune resolutions only**:
```rust
pub fn retain(&mut self, specs: &[Spec]) {
    let keep: HashSet<&Spec> = specs.iter().collect();
    self.resolutions.retain(|k, _| keep.contains(k));
    // Do NOT prune actions here — orphan cleanup happens in tidy
}
```

**`cleanup_orphans()` → prune unreferenced action entries**:
```rust
pub fn cleanup_orphans(&mut self) {
    let referenced: HashSet<ActionKey> = self.resolutions.iter()
        .map(|(spec, res)| ActionKey { id: spec.id.clone(), version: res.version.clone() })
        .collect();
    self.actions.retain(|k, _| referenced.contains(k));
}
```

### D5: `build_update_map` adaptation

**Current**: Iterates lock entries, formats `"SHA # comment"` per action ID.

**New**: Iterates resolutions, looks up the action entry for the SHA:

```rust
pub fn build_update_map(&self, specs: &[Spec]) -> HashMap<ActionId, String> {
    specs.iter()
        .filter_map(|spec| {
            let res = self.resolutions.get(spec)?;
            let key = ActionKey { id: spec.id.clone(), version: res.version.clone() };
            let entry = self.actions.get(&key)?;
            Some((spec.id.clone(), format!("{} # {}", entry.sha, res.comment)))
        })
        .collect()
}
```

The comment now comes from the resolution, not the action entry.

### D6: Serialization format

**Resolutions section** — sorted by action ID, then by specifier string:
```toml
[resolutions."actions/checkout"."^4"]
version = "v4.2.1"
comment = "v4"
```

**Actions section** — sorted by action ID, then by version string:
```toml
[actions."actions/checkout"."v4.2.1"]
sha = "abc123..."
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"
```

Fields in actions: `sha`, `repository`, `ref_type`, `date` (4 fields, down from 6 — `version` and `comment` moved to resolutions).

### D7: Deserialization and migration

**Current format detection**: If the TOML has `[resolutions]`, it's two-tier. If `[actions]` keys contain `@`, it's flat format. If values are plain strings, it's v1.0. If entries have a `specifier` field, it's v1.3.

**Migration chain**: v1.0 → flat → two-tier, v1.3 → flat → two-tier, v1.4 (inline tables) → flat → two-tier, flat → two-tier.

The flat-to-two-tier migration splits each `(action@specifier, entry)` into:
- Resolution: `(action, specifier)` → `{ version: entry.version, comment: entry.comment }`
- Action: `(action, entry.version)` → `{ sha, repository, ref_type, date }`

### D8: `diff()` adaptation

**Decision**: Lock diff operates on the **resolutions** tier only, using the action entries to get SHAs for comparison. The diff answers: "which specs changed their resolved version or SHA?"

```rust
pub fn diff(&self, other: &Lock) -> LockDiff {
    // Compare resolution keys and their resolved versions/SHAs
    // Added: spec in `other` but not `self`
    // Removed: spec in `self` but not `other`
    // Changed: same spec, different resolved version or different SHA
}
```

**Justification**: Consumers of `LockDiff` (tidy report, upgrade report) care about "what changed for my specs?" — not about orphaned action entries. Orphan cleanup is a separate concern handled by `cleanup_orphans()`.

For the common case (upgrade `^4` from `v4.2.1` to `v4.3.0`):
- `LockDiff.added` contains `(Spec(checkout, ^4), Resolution(v4.3.0))`
- `LockDiff.removed` contains `Spec(checkout, ^4)` (the old resolution)
- This appears in reports as "upgraded actions/checkout: v4.2.1 → v4.3.0"
- Orphaned action entry `v4.2.1` is cleaned up separately by tidy

## Module Impact

| Module | Changes |
|--------|---------|
| `domain/lock/mod.rs` | New `Lock` structure with two maps, `ActionKey`, revised API |
| `domain/lock/entry.rs` | Remove `comment` field, potentially replace with `Commit` reuse |
| `domain/lock/resolution.rs` | New `Resolution` struct |
| `infra/lock/convert.rs` | Two-tier serialization/deserialization, flat-to-two-tier migration |
| `infra/lock/migration.rs` | Existing migrations output flat format (unchanged), flat→two-tier added in convert |
| `tidy/lock_sync.rs` | Adjusted `populate_lock_entry` to pass version + commit separately |
| `tidy/mod.rs` | Call `cleanup_orphans()` after retain |
| `upgrade/plan.rs` | Adjusted lock set calls |
| `lint/sha_mismatch.rs` | Adjusted lock entry access |
| `lint/stale_comment.rs` | Comment now in resolution, not entry |
| `lint/unpinned.rs` | Adjusted lock entry access |
| `domain/plan.rs` | `LockDiff` may need adjustment for two-tier |
| `infra/lock/tests.rs` | Updated for new format |

## What This Does NOT Change

- Manifest format (`gx.toml`)
- CLI output or workflow update behavior
- Resolution logic (`ActionResolver`, `VersionRegistry`)
- The `Resolved` struct itself (carries resolution results, not lock structure)
- External behavior of any command
