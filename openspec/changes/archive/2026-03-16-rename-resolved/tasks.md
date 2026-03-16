# Tasks: Rename Resolved to RegistryResolution

Assumes `serialization-boundary` has been applied.

## 1. Rename struct and field
- [x] Rename `Resolved` → `RegistryResolution` in `domain/action/resolved.rs`
- [x] Rename field `version: Specifier` → `specifier: Specifier`
- [x] Update `new()` parameter: `version` → `specifier`
- [x] Update doc comments

## 2. Rename Lock method
- [x] Rename `Lock::set_resolved()` → `Lock::set_from_registry()` in `domain/lock/mod.rs`
- [x] Update field access: `resolved.version` → `resolved.specifier`

## 3. Update From impl
- [x] `From<&Resolved> for Spec` → `From<&RegistryResolution> for Spec`
- [x] Update field access in the impl body

## 4. Remove all `as ResolvedAction` aliases
- [x] `src/tidy/tests.rs`: `Resolved as ResolvedAction` → `RegistryResolution`
- [x] `src/upgrade/plan.rs` (tests): `Resolved as ResolvedAction` → `RegistryResolution`
- [x] `src/lint/stale_comment.rs` (tests): `Resolved as ResolvedAction` → `RegistryResolution`
- [x] `src/infra/lock/format.rs` (tests): `Resolved as ResolvedAction` → `RegistryResolution`
- [x] `src/domain/resolution.rs`: `Resolved as ResolvedAction` → `RegistryResolution`

## 5. Update remaining imports
- [x] `src/domain/lock/mod.rs`: `Resolved` → `RegistryResolution`
- [x] Any other files importing `Resolved` directly

## 6. Update ActionResolver return types
- [x] `domain/resolution.rs`: Update `resolve()` and `resolve_from_sha()` return types and construction
- [x] Update test references in `domain/resolution.rs` tests

## 7. Verify
- [x] `cargo check` passes
- [x] `cargo clippy` passes
- [x] `cargo test` passes
- [x] No remaining references to `Resolved` (the old name) except in `resolved.rs` module name
