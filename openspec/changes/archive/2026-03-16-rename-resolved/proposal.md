# Proposal: Rename Resolved to RegistryResolution

## Why

This is a naming refactor that makes the domain model honest. It does not warrant a spec — no user-facing behavior changes.

## Problem

After the `serialization-boundary` proposal introduces `ResolvedAction` as the workflow output type, the existing `Resolved` struct in `domain/action/resolved.rs` becomes confusing:

1. **`Resolved` is not a resolved action** — it holds a `Specifier` (the input), not the resolved `Version` (the output). It's the registry's return value, not the final resolution.
2. **The codebase already knows this** — every import site aliases it as `Resolved as ResolvedAction`, which now collides with the real `ResolvedAction`.
3. **The `version` field is a `Specifier`** — calling a specifier "version" is misleading. The field should be named `specifier`.

## Solution

1. **Rename `Resolved` → `RegistryResolution`** — honest name for "what the registry returned."
2. **Rename field `version: Specifier` → `specifier: Specifier`** — honest field name.
3. **Update all import sites** — remove the `as ResolvedAction` aliases (no longer needed since `Resolved` → `RegistryResolution` doesn't collide with `ResolvedAction`).
4. **Rename `Lock::set_resolved()` → `Lock::set_from_registry()`** — matches the new type name.

## Non-goals

- Changing the struct's fields or behavior beyond naming.
- Restructuring the resolver's return type.
- Renaming the `resolved.rs` module file. After all three proposals, the file contains `RegistryResolution`, `ResolvedAction`, and `Commit` — all "things that result from resolution." The `resolved` module name remains a valid grouping. Renaming to `registry_resolution.rs` would be misleading since the file also hosts `ResolvedAction` and `Commit`.
- Deleting `to_workflow_ref()` — already handled by `resolved-version-comment` which applies first.

## Outcome

- `Resolved` → `RegistryResolution` with field `specifier: Specifier`.
- All `Resolved as ResolvedAction` aliases removed.
- `Lock::set_resolved()` → `Lock::set_from_registry()`.
- Clear naming distinction: `RegistryResolution` (resolver output) vs `ResolvedAction` (workflow output).

## Delta specs

- **domain-composition**: Update `Resolved` references to `RegistryResolution`, rename `version` field to `specifier` in scenarios, update `From<&Resolved>` to `From<&RegistryResolution>`.
- **resolution**: Clarify that `ResolvedAction` in the main spec is the registry result type now named `RegistryResolution`. Add `Lock::set_from_registry()` rename requirement.

## Order

Apply after `serialization-boundary` (which introduces the `ResolvedAction` name).
