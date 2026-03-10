## Why

Two code-health tests fail: `domain_does_not_import_upward` and `no_duplicate_private_fns_across_command_modules`. The root causes are:

1. **`AppError` is a god error in the wrong layer.** It lives in `domain/error.rs` but wraps errors from `infra`, `lint`, `tidy`, and `upgrade` — violating domain purity. Every variant is reachable from every command, even though `Lint::run()` can never produce `UpgradeError`. Nobody matches on `AppError` variants — it's just a pass-through to `GxError` in `main.rs`.

2. **`Command`/`CommandReport` traits are orchestration, not domain.** They live in `domain/command.rs` but `Command` depends on `AppError`, coupling domain to every layer.

3. **The duplicate-fn test is too broad** — it flags trait implementation methods (`fn render`, `fn run`) as duplicates even though they're intentionally distinct implementations of `Command`/`CommandReport` traits. It also flags `VersionRegistry` trait methods on inline test mocks.

4. **Test mock registries are duplicated** across `tidy/lock_sync.rs`, `tidy/manifest_sync.rs`, `upgrade/plan.rs`, and `domain/resolution.rs` — 8 inline mocks implementing the same trait.

## What Changes

- **Delete `AppError`**. Give `Command` an associated `type Error` instead. Each command defines a precise error type for its `run()` method. `GxError` in `main.rs` wraps per-command errors directly.
- **Move `Command`/`CommandReport` traits** out of `domain/` into `src/command.rs`. Domain stays pure.
- **Make the duplicate-fn test trait-aware** — skip functions inside `impl Trait for Type` blocks.
- **Add shared mock registries** to `domain/resolution.rs` behind `#[cfg(test)] pub(crate) mod testutil`, replacing inline duplicates.

## Capabilities

### New Capabilities

_(None — purely structural.)_

### Modified Capabilities

_(No existing capabilities are changing — these are internal structural improvements.)_

## Impact

- **Domain layer** (`src/domain/`): Loses `error.rs` and `command.rs`. Becomes a pure domain layer with no upward or sideways dependencies.
- **New `src/command.rs`**: Contains `Command` and `CommandReport` traits with associated error type. No dependency on any specific error enum.
- **Command modules** (`init`, `tidy`, `upgrade`, `lint`): Each gains a run-level error type wrapping its domain error + infra errors. Import `Command` from `crate::command` instead of `crate::domain`.
- **`main.rs`**: `GxError` wraps per-command errors directly. `AppError` is gone.
- **`domain/resolution.rs`**: Gains `#[cfg(test)] pub(crate) mod testutil` with shared `FakeRegistry` and `AuthRequiredRegistry`.
- **`tests/code_health.rs`**: Duplicate-fn detection becomes trait-aware.
- **No user-facing changes**.

## Dependencies

- Independent of other changes. The domain file splits are already complete.
