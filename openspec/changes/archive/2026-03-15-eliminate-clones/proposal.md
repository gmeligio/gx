# Eliminate Clones via Domain Restructuring

## Problem

The codebase has ~170 `.clone()` calls. Many exist not because cloning is inherently needed, but because the domain model has:

1. **Duplicate types** — `Spec` and `LockKey` are structurally identical
2. **Flat structs with no composition** — `Resolved` and `Entry` duplicate 4 fields; `InterpretedRef` and `Located` duplicate 3 fields
3. **Functions borrowing when they should consume** — `&T` parameters that immediately clone all fields
4. **Reports converting domain types to strings** — cloning inner strings at the report boundary
5. **No lint catching redundant clones** — `clippy::redundant_clone` is in `nursery`, not covered by `perf = "deny"`

## Scope

Restructure domain types, change function signatures to prefer ownership, and add a lint rule. Do NOT introduce `Rc<str>` or `Arc<str>` — the domain fixes alone should eliminate 80-90 of the ~170 clones.

## Changes

### 1. Delete `LockKey`, use `Spec` everywhere

`LockKey` and `Spec` are identical: `{ id: ActionId, version: Specifier }`. Delete `LockKey` entirely and use `Spec` in all 16 files that reference it. The `From<&Spec> for LockKey` impl (which clones both fields) disappears.

**Files affected:** 16 files importing `LockKey`.

### 2. Extract `ResolvedCommit` from `Resolved` and `Entry`

Four fields are duplicated between `Resolved` and `Entry`:

```rust
pub struct ResolvedCommit {
    pub sha: CommitSha,
    pub repository: String,
    pub ref_type: Option<RefType>,
    pub date: String,
}
```

`Resolved` becomes `{ id, version, commit: ResolvedCommit }`.
`Entry` becomes `{ commit: ResolvedCommit, version: Option<String>, comment: String }`.

This enables `Resolved::with_sha(self, sha)` to use struct update syntax with zero clones instead of cloning 5 fields.

### 3. Compose `Located` from `InterpretedRef`

`Located` currently duplicates all 3 fields of `InterpretedRef` plus adds `location`. Change to:

```rust
pub struct Located {
    pub action: InterpretedRef,
    pub location: Location,
}
```

`ActionSet` collapses `add()` and `add_located()` into a single method taking `&InterpretedRef`. `Located` callers pass `&located.action`.

### 4. Prefer ownership transfer over borrow+clone (Strategy 1)

Change function signatures from `&T` to `T` where the function immediately clones all fields. Key targets:

- `ActionSet::add(&mut self, interpreted: &InterpretedRef)` → takes `InterpretedRef`
- `ActionSet::from_located(actions: &[Located])` → takes `Vec<Located>`
- `Resolved::with_sha(&self, sha)` → takes `self` (consuming)
- Report builders that clone domain types into output structs

### 5. Reports hold domain types directly

Change report structs from string tuples to domain types:

```rust
// Before
pub removed: Vec<String>,
pub added: Vec<(String, String)>,

// After
pub removed: Vec<ActionId>,
pub added: Vec<(ActionId, Specifier)>,
```

Display/formatting moves to the renderer, not the report builder.

### 6. Add `redundant_clone = "deny"` to Cargo.toml

`clippy::redundant_clone` is in the `nursery` group, NOT covered by `perf = "deny"`. Adding it explicitly catches 1 existing bug (`workflow_update.rs:71`) and prevents future regressions.

## Trade-offs

| Change | Benefit | Cost |
|--------|---------|------|
| Delete `LockKey` | ~10 clones eliminated, simpler model | Lose type-level distinction between manifest spec and lock lookup key |
| `ResolvedCommit` | ~10 clones eliminated, struct update syntax | `resolved.sha` becomes `resolved.commit.sha` (more nesting) |
| Compose `Located` | ~8 clones eliminated, one fewer duplicate method | `action.id` becomes `action.action.id` unless accessors are added |
| Ownership transfer | ~40-50 clones eliminated | Breaking API changes across ~30 function signatures |
| Domain-typed reports | ~15 clones eliminated | Formatting moves to renderers |
| `redundant_clone` lint | Catches regressions | Nursery lint, may have rare false positives |

## Out of Scope

- `Rc<str>` / `Arc<str>` for identity types (not needed after domain fixes)
- Renaming `UsesRef` or its fields (separate change)
- Renaming `InterpretedRef` (decided to keep as-is)

## Estimated Impact

~80-90 of ~170 clones eliminated through structural changes. Remaining ~80 clones are either in test code (~30), genuinely necessary (~25), or addressable through further ownership transfer (~25).
