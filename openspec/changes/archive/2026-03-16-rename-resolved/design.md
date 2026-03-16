# Design: Rename Resolved to RegistryResolution

## Approach

Pure mechanical rename. No logic changes, no new types, no behavioral differences.

## Changes

### domain/action/resolved.rs
- Rename `struct Resolved` → `struct RegistryResolution`.
- Rename field `version: Specifier` → `specifier: Specifier`.
- Update `new()` parameter name: `version` → `specifier`.
- Update `Resolved::with_sha` → `RegistryResolution::with_sha`.
- Update doc comments to say "registry resolution" instead of "resolved action."
- Update all test references.

### domain/lock/mod.rs
- Rename `set_resolved()` → `set_from_registry()`.
- Update import: `Resolved` → `RegistryResolution`.
- Update internal references to `resolved.version` → `resolved.specifier`.

### domain/resolution.rs
- Remove `Resolved as ResolvedAction` alias (line 2). Import `RegistryResolution` directly.
- Update `ActionResolver::resolve()` and `resolve_from_sha()` return types and construction.
- Update test references.

### All import sites with `Resolved as ResolvedAction` alias
These files use the alias and must switch to direct `RegistryResolution` import:
- `src/tidy/tests.rs` (line 5)
- `src/upgrade/plan.rs` (line 293 in tests)
- `src/lint/stale_comment.rs` (line 67 in tests)
- `src/infra/lock/format.rs` (line 225 in tests)

### Other import sites using `Resolved` directly
- `src/domain/lock/mod.rs` (line 4)
- `src/domain/plan.rs` — if it references `Resolved`

### From impl
- `From<&Resolved> for Spec` → `From<&RegistryResolution> for Spec`.
- Update field access: `resolved.version` → `resolved.specifier`.

## No file rename

The module file stays as `resolved.rs`. After all three proposals, it contains `RegistryResolution`, `ResolvedAction`, and `Commit` — all resolution-related types. The module name `resolved` remains accurate.
