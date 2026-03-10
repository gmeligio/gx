## Context

The domain layer contains pure types and business logic. Four files exceed the 500-line budget, and the directory has 10 direct `.rs` files (target: 8). The prior `code-organization` change enriched the domain with methods like `Manifest::diff()`, `Lock::diff()`, `sync_overrides()`, etc. — which grew these files. Now the structural split is needed to stay within budget.

The existing `domain/action/` subdirectory provides precedent for grouping related types.

## Goals / Non-Goals

**Goals:**
- Every domain `.rs` file under 500 lines
- `src/domain/` direct file count ≤ 8
- Each file has a single clear responsibility
- Public API surface unchanged via re-exports

**Non-Goals:**
- Moving logic between layers
- Adding new domain methods or types
- Changing test behavior

## Decisions

### 1. Extract `Specifier` from `identity.rs`

**Decision**: Move `Specifier` enum + all its impls (~180 lines) + its tests to `domain/action/specifier.rs`.

**Rationale**: `Specifier` is a substantial type with its own parsing logic, version matching, and precision detection. It's conceptually separate from `ActionId`, `Version`, and `CommitSha`. After extraction, `identity.rs` drops to ~450 lines (types + tests) and `specifier.rs` is ~310 lines.

### 2. `manifest.rs` → `manifest/` directory

**Decision**: Split into:
- `manifest/mod.rs`: `Manifest` struct, basic CRUD (get/set/has/remove/is_empty), `diff()`, `lock_keys()`, re-exports
- `manifest/overrides.rs`: `ActionOverride`, `resolve_version()`, `sync_overrides()`, `prune_stale_overrides()`, `replace_overrides()`

**Rationale**: Override logic is a distinct concern from basic manifest operations. `resolve_version()` alone has multi-level fallback (step → job → workflow → global) that deserves isolation. This also reduces the direct file count in `domain/`.

### 3. `lock.rs` → `lock/` directory

**Decision**: Split into:
- `lock/mod.rs`: `Lock` struct, CRUD, `diff()`, `build_update_map()`, `retain()`
- `lock/entry.rs`: `LockEntry` struct, `is_complete()` validation, setters

**Rationale**: `LockEntry` has substantial validation logic (`is_complete` with multiple field checks). The split is clean because `Lock` uses `LockEntry` but the entry's internal logic is self-contained.

### 4. `resolution.rs` — extract helpers

**Decision**: Extract `ShaIndex` and `select_most_specific_tag`/`parse_version_components` to `domain/action/tag_selection.rs` (they're about version/tag logic, fitting the action subdirectory). Keep `ResolutionError`, `ResolvedRef`, `VersionRegistry`, `ActionResolver` in `resolution.rs`.

**Rationale**: `ShaIndex` is a caching utility. Tag selection is a pure function about version ordering. Both are used by `ActionResolver` but are independently testable. Moving them under `action/` makes sense since they operate on version tags.

### 5. Re-exports preserve API stability

**Decision**: `domain/mod.rs` re-exports everything at the same paths. `domain/manifest/mod.rs` re-exports from `overrides.rs`. Callers never need to know about the internal split.

## Target Structure

```
src/domain/
  action/
    mod.rs              — unchanged
    identity.rs    ~450 — ActionId, Version, CommitSha, VersionPrecision
    specifier.rs   ~310 — Specifier enum + parsing + matching
    tag_selection.rs ~120 — select_most_specific_tag, parse_version_components
  manifest/
    mod.rs         ~400 — Manifest struct, CRUD, diff(), lock_keys()
    overrides.rs   ~330 — ActionOverride, resolve_version, sync/prune overrides
  lock/
    mod.rs         ~350 — Lock struct, CRUD, diff(), build_update_map, retain
    entry.rs       ~230 — LockEntry, is_complete, setters
  resolution.rs    ~350 — ResolutionError, ResolvedRef, VersionRegistry,
                          ActionResolver (without ShaIndex and tag helpers)
  command.rs              — unchanged
  error.rs                — unchanged
  event.rs                — unchanged
  mod.rs                  — updated re-exports
  plan.rs                 — unchanged
  workflow.rs             — unchanged
  workflow_actions.rs     — unchanged
```

Direct `.rs` files in `domain/`: action/(dir), manifest/(dir), lock/(dir), resolution.rs, command.rs, error.rs, event.rs, mod.rs, plan.rs, workflow.rs, workflow_actions.rs = **8 files** (directories don't count).

## Risks / Trade-offs

- **`manifest/mod.rs` at ~400 lines**: Close to budget but has headroom. If `diff()` grows, it could be extracted to `manifest/diff.rs`.
- **`ShaIndex` in `action/`**: `ShaIndex` caches `ShaDescription` which is defined in `resolution.rs`. The cross-reference is fine via `use crate::domain::resolution::ShaDescription`. If it feels wrong, `ShaIndex` can stay in `resolution.rs` and we accept it being slightly over or extract to `resolution/sha_index.rs`.
