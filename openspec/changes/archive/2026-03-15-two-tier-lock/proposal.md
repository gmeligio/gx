# Two-Tier Lock File

## Problem

The lock file currently uses a flat structure keyed by `(ActionId, Specifier)` — the *requested* dependency, not the *resolved* result:

```toml
[actions."actions/checkout@^4"]
sha = "abc123..."
version = "v4.2.1"
comment = "v4"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"
```

This creates several issues:

1. **Specifier change forces re-resolution** — changing `^4` to `^4.2` orphans the old entry and requires a fresh resolution, even though the existing `v4.2.1` still satisfies both.
2. **No deduplication** — if overrides produce two specifiers (`^4` and `^4.2`) that resolve to the same version, the lock stores two full copies of identical metadata.
3. **Lock doesn't answer "what version do I have?"** — you must know the specifier to look up the entry, not the resolved version.

## Scope

Restructure the lock file into two tiers: a `[resolutions]` table mapping specifiers to resolved versions, and an `[actions]` table mapping resolved versions to their metadata. Update the domain model, serialization, and all consumers.

## Changes

### 1. Split lock into `[resolutions]` + `[actions]`

**Before (flat):**
```toml
[actions."actions/checkout@^4"]
sha = "abc123..."
version = "v4.2.1"
comment = "v4"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"
```

**After (two-tier):**
```toml
[resolutions."actions/checkout"."^4"]
version = "v4.2.1"
comment = "v4"

[resolutions."actions/checkout"."^4.2"]
version = "v4.2.1"
comment = "v4.2"

[actions."actions/checkout"."v4.2.1"]
sha = "abc123def456789012345678901234567890abcd"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"
```

- **`resolutions`** maps `(ActionId, Specifier)` to a resolved `Version` and `comment`. The comment depends on specifier precision (`^4` → `"v4"`, `^4.2` → `"v4.2"`), so it belongs here, not in the action entry.
- **`actions`** maps `(ActionId, Version)` to commit metadata. No specifier coupling. Multiple resolutions can point to the same action entry.

Both use nested TOML tables — no composite string keys, no parsing needed.

### 2. Use `Version` newtype in lock domain

The `Version` newtype (`src/domain/action/identity.rs`) already represents concrete resolved versions. Use it as the tier 2 key and the tier 1 value instead of raw `String`.

### 3. Restructure `Lock` domain model

The `Lock` struct changes from a single `HashMap<Spec, Entry>` to two maps:

- `resolutions: HashMap<Spec, Resolution>` where `Resolution { version: Version, comment: String }`
- `actions: HashMap<ActionKey, Commit>` where `ActionKey { id: ActionId, version: Version }` and `Commit` is the existing struct (sha, repository, ref_type, date) — no new type needed

### 4. Update lock consumers

- **Lookup**: `lock.get(spec)` first looks up the resolution to get the `Version`, then looks up the action entry. Two O(1) lookups.
- **`retain()`**: Prune `resolutions` by manifest specs, then prune `actions` by which versions are still referenced. Unreferenced action entries become orphans cleaned up by `tidy`.
- **`set()`**: Writing a resolution populates both tiers.
- **`diff()`**: Compare both tiers.

### 5. Lazy cleanup of orphaned action entries

When a specifier changes (e.g., `^4` → `^4.2`) and still resolves to `v4.2.1`, only the `resolutions` tier updates. The `actions` entry for `v4.2.1` is untouched. When a specifier resolves to a *new* version (`v4.3.0`), the old `v4.2.1` action entry becomes orphaned. Cleanup happens in `tidy`, not eagerly — this avoids unnecessary work during resolution.

### 6. Lock file migration

Existing flat-format lock files are migrated to the two-tier format. The migration splits each flat entry into a resolution + action entry. This is a one-way migration — no version field is written (per the versionless format spec).

## Trade-offs

| Change | Benefit | Cost |
|--------|---------|------|
| Two-tier split | Specifier changes reuse resolutions; deduplication; version-indexed lookup | Two hash lookups instead of one; more complex domain model |
| Nested TOML keys | No string parsing; grouped by action | Deeper nesting in lock file |
| `Version` newtype in lock | Type safety, no raw strings | Migration of existing `Option<String>` fields |
| Lazy orphan cleanup | Simpler resolution path; no unnecessary work | Orphaned entries linger until `tidy` |
| One-way migration | Clean break from flat format | Old tooling can't read new format |

## Out of Scope

- Renaming `Spec` or `Specifier` (this change uses existing domain types)
- Changing the manifest format
- Changing CLI output or workflow update behavior
- `Rc<str>` / `Arc<str>` for identity types

## Design Precedent

The two-tier structure follows **pnpm-lock.yaml** (`importers` + `packages`), the only major package manager with an explicit two-tier lock in a single file. Naming uses domain terms (`resolutions`, `actions`) rather than generic package manager terms.
