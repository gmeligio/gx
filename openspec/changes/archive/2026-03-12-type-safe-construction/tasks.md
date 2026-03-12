## 1. Restructure Types

- [x] 1.1 Modify `UpgradeMode::Pinned` to carry both `Version` and `ActionId` (e.g., `Pinned { version: Version, target: ActionId }`)
- [x] 1.2 Change `UpgradeRequest::new()` from `Result<Self, UpgradeError>` to `Self` (infallible construction)
- [x] 1.3 Remove `UpgradeError::PinnedRequiresSingleScope` variant

## 2. Update Call Sites

- [x] 2.1 Update `resolve_upgrade_mode()` in `src/upgrade/cli.rs` to construct `UpgradeRequest` without `.expect()`
- [x] 2.2 Update `plan()` in `src/upgrade/plan.rs` to pattern-match on the new `Pinned { version, target }` variant
- [x] 2.3 Update any other references to `UpgradeRequest::new()` that handle the `Result`

## 3. Update Tests

- [x] 3.1 Update `src/upgrade/types.rs` tests — remove `unwrap_err()` test for `PinnedRequiresSingleScope` since the variant no longer exists
- [x] 3.2 Update `src/upgrade/cli.rs` tests if construction API changed
- [x] 3.3 Update `src/upgrade/plan.rs` tests for new `Pinned` variant structure

## 4. Verification

- [x] 4.1 Run `mise run clippy` and confirm zero `unwrap_in_result` errors in `src/upgrade/`
- [x] 4.2 Run `rtk cargo test` and confirm all tests pass
