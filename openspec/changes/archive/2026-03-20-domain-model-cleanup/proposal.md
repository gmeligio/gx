# Domain Model Cleanup

## Summary

Refactor domain types to eliminate duplication, improve composition, and align naming with semantics. No user-facing behavior changes — this is internal restructuring.

## Motivation

The current domain model has structural issues identified during analysis:

1. **Duplicate types**: `Commit` and `ResolvedRef` are structurally identical (sha, repository, ref_type, date) but have different names
2. **Poor composition**: `RegistryResolution` flattens `id + specifier` instead of composing from `Spec`; carries data the caller already has
3. **Overloaded naming**: `Spec.version` is a `Specifier` (range like `^4`), not a `Version` (concrete like `v4.2.1`)
4. **Misplaced type**: `InterpretedRef` lives in `uses_ref.rs` alongside parsing artifacts, but is a domain concept used widely
5. **Over-engineered Lock**: Two-tier `HashMap<Spec, Resolution>` + `HashMap<ActionKey, Commit>` adds indirection without proportional benefit

## Spec gate

Skipped — internal refactoring with no user-visible change.

## Changes

### 1. Merge `ResolvedRef` into `Commit`

`ResolvedRef` (in `resolution.rs`) and `Commit` (in `action/resolved.rs`) have identical fields. Keep `Commit` as the single type. The `VersionRegistry` trait methods that return `ResolvedRef` will return `Commit` instead.

**Files affected:**
- `domain/resolution.rs` — remove `ResolvedRef`, update `VersionRegistry` trait
- `domain/action/resolved.rs` — `Commit` stays as-is
- `infra/github/mod.rs` — update registry implementation

### 2. Rename `InterpretedRef` to `WorkflowAction`, move to `domain/workflow_action.rs`

`InterpretedRef` represents "an action as declared in a workflow file." The name should reflect *what* it is, not *how* it was created.

- Rename `InterpretedRef` → `WorkflowAction`
- Create `domain/workflow_action.rs` as its new home
- `uses_ref.rs` retains only `UsesRef` and `RefType` (parsing concerns)
- `Located`, `Location`, `ActionSet`, `WorkflowPath`, `JobId`, `StepIndex` stay in `workflow_actions.rs` (they co-locate with `WorkflowAction`'s consumers)

**Files affected:**
- `domain/action/uses_ref.rs` — remove `InterpretedRef`, keep `UsesRef`/`RefType`
- `domain/workflow_action.rs` — new file with `WorkflowAction`
- `domain/workflow_actions.rs` — update imports
- All consumers (tidy, lint, patches, overrides)

### 3. Rename `Spec.version` to `Spec.specifier`

The field holds a `Specifier` (desired range), not a `Version` (resolved concrete). The field name should match the type name.

**Files affected:**
- `domain/action/spec.rs` — rename field
- All call sites accessing `spec.version`

### 4. Flatten Lock to single-tier

Replace the two-tier structure with a flat `HashMap<Spec, LockEntry>`:

```rust
struct LockEntry {
    pub version: Version,
    pub commit: Commit,
}

struct Lock {
    entries: HashMap<Spec, LockEntry>,
}
```

- Lookups become single-step
- `set_version` becomes a simple field update (no key-swap)
- `cleanup_orphans` is eliminated (no orphans possible)
- Infra serialization can still write two-tier TOML for dedup — that's a serialization concern, not a domain one

**Files affected:**
- `domain/lock/mod.rs` — rewrite to flat structure
- `domain/lock/resolution.rs` — may merge into parent
- `infra/lock/format.rs` — update serialization/deserialization (can keep two-tier TOML format)

### 5. Slim `RegistryResolution` to `Resolved`

The resolver receives a `Spec` as input. The result only needs to carry what was *discovered*:

```rust
struct Resolved {
    pub version: Version,
    pub commit: Commit,
}
```

Remove `id` and `specifier` fields — callers already have the `Spec`.

**Files affected:**
- `domain/action/resolved.rs` — replace `RegistryResolution` with `Resolved`
- `domain/resolution.rs` — update `ActionResolver` return types
- `domain/lock/mod.rs` — remove `set_from_registry`, callers use `set` directly
- `tidy/lock_sync.rs`, `upgrade/plan.rs` — update call sites

### 6. Leave `ShaDescription` as-is

`ShaDescription` (tags + repository + date) is a different concept from `Commit` (sha + repository + ref_type + date). The field overlap is incidental. No change needed.

## Ordering

Changes are independent but a natural sequence minimizes churn:

1. Rename `Spec.version` → `Spec.specifier` (pure rename, minimal conflicts)
2. Merge `ResolvedRef` into `Commit` (removes a type)
3. Slim `RegistryResolution` → `Resolved` (depends on 2)
4. Rename + move `InterpretedRef` → `WorkflowAction` (independent)
5. Flatten Lock (largest change, do last)

## Risks

- **Lock flattening** is the highest-risk change — it touches serialization roundtrip logic. Existing tests should catch regressions.
- **Rename propagation** (`Spec.version` → `Spec.specifier`) touches many files but is mechanical.
- All changes are behind compilation — Rust's type system catches missed call sites.
