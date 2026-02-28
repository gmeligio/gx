## 1. Restructure UpgradeMode types

- [x] 1.1 Add `UpgradeScope` enum (`All`, `Single(ActionId)`) to `src/commands/upgrade.rs`
- [x] 1.2 Replace `UpgradeMode::Targeted(ActionId, Version)` with `UpgradeMode::Pinned(Version)`
- [x] 1.3 Add `UpgradeRequest` struct with `mode: UpgradeMode` and `scope: UpgradeScope` fields
- [x] 1.4 Implement `UpgradeRequest::new` with validation rejecting `Pinned + All`

## 2. Update determine_upgrades

- [x] 2.1 Change `determine_upgrades` signature to accept `&UpgradeRequest` instead of `&UpgradeMode`
- [x] 2.2 For `Single(id)` scope, filter `manifest.specs()` to only the matching action before processing
- [x] 2.3 Update the `Pinned` arm (was `Targeted`) to use `request.scope` for the `ActionId` and `request.mode` for the `Version`

## 3. Update CLI parsing

- [x] 3.1 Remove `conflicts_with = "action"` from the `--latest` flag in `src/main.rs`
- [x] 3.2 Update `resolve_upgrade_mode` to detect bare `ACTION` (no `@`) vs `ACTION@VERSION`
- [x] 3.3 Add explicit rejection of `--latest` combined with `ACTION@VERSION` with a clear error message
- [x] 3.4 Wire all four valid CLI combinations to the correct `UpgradeRequest` values

## 4. Update call sites and tests

- [x] 4.1 Update `src/commands/app.rs` to pass `UpgradeRequest` instead of `UpgradeMode`
- [x] 4.2 Update `tests/upgrade_test.rs` — replace `UpgradeMode::Safe`, `UpgradeMode::Latest`, `UpgradeMode::Targeted` usages with `UpgradeRequest`
- [x] 4.3 Add tests for `Safe + Single`: only the targeted action is upgraded
- [x] 4.4 Add tests for `Latest + Single`: only the targeted action is upgraded to latest
- [x] 4.5 Add test for `--latest` + `ACTION@VERSION` rejection
- [x] 4.6 Add test for `Pinned + All` rejection in `UpgradeRequest::new`
