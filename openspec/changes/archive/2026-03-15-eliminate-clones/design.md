# Design: Eliminate Clones via Domain Restructuring

## Overview

Eliminate ~80-90 of ~170 `.clone()` calls by restructuring domain types to remove field duplication, preferring ownership transfer, and adding a lint guard.

## Key Decisions

### D1: Delete LockKey, use Spec everywhere

**Decision**: Delete `LockKey` entirely. Add `Hash` and `Eq` derives to `Spec`. Move `parse()` method to `Spec`.

**Justification**: `LockKey` and `Spec` are structurally identical (`{ id: ActionId, version: Specifier }`). The type-level distinction provides no safety — both represent "action + version specifier" and there is no invariant that one enforces and the other does not. The `From<&Spec> for LockKey` impl clones both fields unnecessarily. Every `LockKey` in the codebase is constructed from a `Spec` or parsed from the same `"action@specifier"` format that `Spec` could parse directly. Removing the conversion eliminates ~10 clones and simplifies the domain model.

**Migration**: All 16 files importing `LockKey` switch to `Spec`. The `Lock` struct changes from `HashMap<LockKey, Entry>` to `HashMap<Spec, Entry>`. `From<&Resolved> for LockKey` becomes `From<&Resolved> for Spec`.

### D2: Extract ResolvedCommit

**Decision**: Extract `{ sha, repository, ref_type, date }` into a `ResolvedCommit` struct shared by `Resolved` and `Entry`.

**Justification**: These four fields are duplicated verbatim between `Resolved` and `Entry`. When converting `Resolved` → `Entry` (in lock building), all four are cloned individually. With `ResolvedCommit`, the conversion moves the struct in one shot. The `with_sha` method on `Resolved` currently clones 5 fields; with composition + ownership it becomes a struct update with zero clones.

**Nesting trade-off**: `resolved.sha` becomes `resolved.commit.sha`. This adds one level of nesting but is idiomatic Rust composition. No `Deref` or accessor methods — direct field access through the composed struct.

**Location**: `ResolvedCommit` lives in `src/domain/action/resolved.rs` alongside `Resolved`, since both are part of the resolution domain.

### D3: Compose Located from InterpretedRef

**Decision**: Change `Located` from flat fields to `{ action: InterpretedRef, location: Location }`.

**Justification**: `Located` duplicates all 3 fields of `InterpretedRef` (`id`, `version`, `sha`). Composition eliminates the duplication and allows `ActionSet` to accept `&InterpretedRef` uniformly — callers with `Located` pass `&located.action`.

**Naming**: The field is named `action` (not `ref` or `interpreted`), so access reads as `located.action.id` which is natural. This avoids the `action.action.id` problem raised in the proposal review — only `from_located` iteration uses the pattern `for action in actions { action.action.id }`, which can use destructuring: `for Located { action, .. } in actions`.

**ActionSet consolidation**: `add()` and `add_located()` collapse into a single `add(&mut self, interpreted: &InterpretedRef)`. The `add_located` method is deleted.

### D4: Ownership transfer strategy

**Decision**: Change signatures to consume `self`/take `T` instead of `&T` only where the function clones ALL fields of the borrowed parameter. Keep `&T` where the function uses fields selectively.

**Targets**:
- `Resolved::with_sha(&self, sha)` → `with_sha(self, sha)` — clones all 5 fields today
- `ActionSet::from_located(&[Located])` → `from_located(actions: Vec<Located>)` — iterates and clones all fields
- Report builders that construct `(String, String)` tuples from domain types — take owned `ActionId`/`Specifier`

**Not changed**: `ActionSet::add(&mut self, &InterpretedRef)` stays as borrow — the caller often still needs the `InterpretedRef` after adding.

### D5: Domain-typed reports

**Decision**: Report `removed`/`added`/`upgraded`/`skipped` fields use `ActionId` instead of `String` for the action name. Version from/to remain `String` since they are display values (the resolved tag name, not a domain type).

**Justification**: Report builders currently call `.to_string()` or `.as_str().to_owned()` on `ActionId` to store strings. Using `ActionId` directly eliminates the clone at the build boundary. The `render()` method calls `action.as_str()` when constructing `OutputLine` — zero-copy since `OutputLine` already holds `String` and we can call `.to_string()` once at render time.

**OutputLine stays String-based**: `OutputLine` is a rendering type, not a domain type. It correctly holds `String` values. The change is limited to the report structs between domain and rendering.

### D6: redundant_clone lint

**Decision**: Add `redundant_clone = "deny"` to `[lints.clippy]` in `Cargo.toml`.

**Justification**: This is a nursery lint, not covered by `perf = "deny"`. It catches at least one existing unnecessary clone (`workflow_update.rs`). False positives in nursery lints are rare for `redundant_clone` specifically — it has been stable in practice since Rust 1.70+. Using `deny` (not `warn`) ensures regressions are caught at compile time. Any genuine false positive can be suppressed with `#[expect(clippy::redundant_clone, reason = "...")]` per existing code-quality spec.

## Module Impact

| Module | Changes |
|--------|---------|
| `domain/action/spec.rs` | Delete `LockKey`, add `Hash`/`Eq` to `Spec`, add `parse()` |
| `domain/action/resolved.rs` | Add `ResolvedCommit`, restructure `Resolved`, consume `self` in `with_sha` |
| `domain/lock/entry.rs` | Restructure `Entry` to use `ResolvedCommit` |
| `domain/workflow_actions.rs` | Compose `Located`, delete `add_located`, change `from_located` signature |
| `tidy/report.rs` | Use `ActionId`/`Specifier` in fields |
| `upgrade/report.rs` | Use `ActionId` in fields |
| `Cargo.toml` | Add `redundant_clone = "deny"` |
| 16 files importing `LockKey` | Switch to `Spec` |
| ~10 files calling `with_sha` / building entries | Adjust for `ResolvedCommit` composition |
| ~5 files using `Located` fields | Adjust for `located.action.id` pattern |

## What This Does NOT Change

- `OutputLine` enum stays `String`-based (rendering layer)
- `InterpretedRef` keeps its name (decided in proposal)
- `UsesRef` keeps its name and fields (out of scope)
- No `Rc<str>` or `Arc<str>` introduced
- No changes to external behavior, CLI output, or file formats
