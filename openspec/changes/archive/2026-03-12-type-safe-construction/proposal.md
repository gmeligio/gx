## Why

`UpgradeRequest::new()` returns `Result` to enforce that `Pinned` mode requires `Single` scope. But every call site in `resolve_upgrade_mode()` uses `.expect("... is always valid")` inside a function that itself returns `Result` — triggering the `unwrap_in_result` lint. The runtime validation is provably unnecessary: only one of the six mode/scope combinations is invalid (`Pinned + All`), and the call sites never construct it. The type system should make the invalid state unrepresentable instead of validating at runtime.

## What Changes

- Restructure `UpgradeRequest` construction to eliminate `Result` from `new()` by encoding the constraint in the type system
- Remove all `.expect()` calls from `resolve_upgrade_mode()`
- Remove the `PinnedRequiresSingleScope` error variant from `UpgradeError`

## Capabilities

### New Capabilities
- `type-safe-upgrade-request`: Defines how `UpgradeRequest` construction enforces valid mode/scope combinations at compile time rather than runtime.

### Modified Capabilities
- `upgrade-scope`: The `UpgradeRequest` construction API changes from fallible (`Result`) to infallible, with the `Pinned` mode requiring an `ActionId` directly.

## Impact

- **`src/upgrade/types.rs`**: Restructured `UpgradeRequest` and `UpgradeMode`/`UpgradeScope` types
- **`src/upgrade/cli.rs`**: Simplified construction without `.expect()`
- **`src/upgrade/plan.rs`**: Updated pattern matching if type structure changes
- **Tests**: Updated to reflect new API
- **No user-facing changes**: CLI behavior is identical
