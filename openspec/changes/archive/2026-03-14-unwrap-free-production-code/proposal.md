## Why

The prior change `clippy-restriction-lints` refactored code to comply with 35 restriction lints but never added them to `Cargo.toml` — only `module_name_repetitions` and `pub_use` were committed. The gate is missing. Meanwhile, production code still contains `expect()` calls that panic instead of propagating errors, and the type system allows invalid states that force runtime validation.

This change closes both gaps: enable all 35 restriction lints and eliminate every `expect`/`unwrap` from production code through type system redesigns — not suppression.

## What Changes

### 1. Enable 35 restriction lints in Cargo.toml

Add the individually selected restriction lints from the `clippy-restriction-config` spec to `[lints.clippy]`. The codebase already complies with most of them from the prior refactor — the missing piece is turning on the gate.

### 2. Eliminate production expect/unwrap through type redesigns

Six production `expect()` calls exist. Each is fixed by making the type system encode the invariant:

- **`upgrade/cli.rs` (5 expects)**: `Request::new(Mode, Scope)` returns `Result` because `Pinned + All` is invalid. Fix: move `Pinned` into `Scope` so the invalid state is unrepresentable. `Request::new` becomes infallible.

- **`infra/github/mod.rs` (1 expect)**: `impl Default for Registry` calls `Self::new(None).expect(...)`. Fix: delete the impl — zero callers exist.

### 3. Introduce StepIndex(u16) domain type

`step: Option<usize>` appears in 4 structs. The `usize -> i64` conversion for TOML serialization uses `expect("step index overflow")`. Fix: `StepIndex(u16)` makes `From<StepIndex> for i64` infallible. Step indices are always < 1000.

### 4. Replace remaining unsafe patterns

- **`infra/manifest/patch.rs`**: `.as_array_mut().expect(...)` becomes `.ok_or(Error)?`.
- **`domain/workflow_actions.rs`**: `candidates[0]` indexing + dead `unwrap_or_else` simplified to `Version::highest(&candidates)` — the fallback was unreachable.

### 5. Add #[expect] annotations to test modules

All `#[cfg(test)]` modules in `src/` and integration test files in `tests/` get `#[expect(clippy::unwrap_used, ...)]` annotations with reasons, as designed in the `code-quality` spec.

## Capabilities

### Modified Capabilities

- `type-safe-upgrade-request`: Implements the spec — `Pinned` becomes a `Scope` variant carrying `ActionId` + `Version`, making `Request::new` infallible and removing `PinnedRequiresSingleScope`.
- `upgrade-scope`: Construction API changes from fallible to infallible.
- `code-quality`: Enables the 35 restriction lints that were designed but never gated. Adds `StepIndex` newtype requirement.

## Impact

- **`Cargo.toml`**: 33 new `[lints.clippy]` entries (35 total minus 2 already present)
- **`src/upgrade/types.rs`**: `Mode::Pinned(Version)` removed, `Scope::Pinned(ActionId, Version)` added, `Request::new` returns `Self`, `Error::PinnedRequiresSingleScope` removed
- **`src/upgrade/cli.rs`**: All `.expect()` calls removed — construction is infallible
- **`src/upgrade/plan.rs`**: Pattern matching updated for new `Scope::Pinned` variant
- **`src/infra/github/mod.rs`**: `impl Default for Registry` deleted
- **`src/domain/workflow_actions.rs`**: `Location.step` and `ActionOverride.step` change from `Option<usize>` to `Option<StepIndex>`
- **`src/infra/manifest/patch.rs`** and **`src/infra/manifest/convert.rs`**: `StepIndex` + fallible array access
- **All test modules**: `#[expect]` annotations for safety lints
- **No user-facing changes**: CLI behavior is identical
