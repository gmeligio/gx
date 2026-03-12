## Context

Currently `UpgradeRequest::new(mode, scope)` validates at runtime that `Pinned` mode is not combined with `All` scope. This validation exists because `Pinned(Version)` requires a specific action target. The five valid combinations are:

| Mode | Scope | Valid? |
|------|-------|--------|
| `Safe` | `All` | Yes |
| `Safe` | `Single(id)` | Yes |
| `Latest` | `All` | Yes |
| `Latest` | `Single(id)` | Yes |
| `Pinned(v)` | `Single(id)` | Yes |
| `Pinned(v)` | `All` | **No** |

The single invalid combination is caught by `UpgradeRequest::new()` returning `Err(PinnedRequiresSingleScope)`. But callers always know which combination they're constructing, so every call uses `.expect()`.

## Goals / Non-Goals

**Goals:**
- Make `Pinned + All` unrepresentable in the type system
- Remove all `.expect()` calls from `resolve_upgrade_mode()`
- Eliminate the `PinnedRequiresSingleScope` error variant

**Non-Goals:**
- Changing CLI behavior or user-facing error messages
- Restructuring the entire upgrade module

## Decisions

### Decision 1: Absorb `Pinned` target into the variant

**Choice:** Move the `ActionId` from `UpgradeScope::Single` into `UpgradeMode::Pinned`, so `Pinned` always carries its target.

```rust
// Before:
pub enum UpgradeMode {
    Safe,
    Latest,
    Pinned(Version),
}
pub enum UpgradeScope {
    All,
    Single(ActionId),
}

// After:
pub enum UpgradeMode {
    Safe,
    Latest,
    Pinned { version: Version, target: ActionId },
}
pub enum UpgradeScope {
    All,
    Single(ActionId),
}
```

With this design, `Pinned` always has a target and `UpgradeRequest` can be constructed infallibly:

```rust
impl UpgradeRequest {
    pub const fn new(mode: UpgradeMode, scope: UpgradeScope) -> Self {
        Self { mode, scope }
    }
}
```

The `scope` field still exists for `Safe`/`Latest` modes (which can target `All` or `Single`). For `Pinned`, the scope is implicitly `Single` via the embedded `ActionId`.

**Alternative considered:** Encode all valid combinations as separate enum variants in `UpgradeRequest` itself (e.g., `SafeAll`, `SafeSingle(ActionId)`, `LatestAll`, `LatestSingle(ActionId)`, `Pinned(ActionId, Version)`). Rejected — the combinatorial explosion makes pattern matching verbose, and the current `mode`/`scope` split is clean for the non-Pinned cases.

**Alternative considered:** Keep `Result` but remove `.expect()` by propagating the error. Rejected — the error can never happen at the known call sites, so propagating it adds dead error-handling code.

## Risks / Trade-offs

- **Risk: Pattern matching in `plan.rs` needs update** → Small, localized change. The `Pinned` branch already handles the `Single` case specially.
- **Trade-off: `Pinned` duplicates the `ActionId` if `scope` is also `Single`** → Acceptable. The `scope` field can be ignored for `Pinned` mode, or set to `All` as a convention (since the target is in the mode).
