## 1. Make both error enums extensible first

- [x] 1.1 Add `#[non_exhaustive]` to `resolution::Error` in `src/domain/resolution.rs`
- [x] 1.2 Add `#[non_exhaustive]` to `github::Error` in `src/infra/github/registry.rs`, leaving the four in-crate `map_err` arms exhaustive (Decision 5)

## 2. Carry the forge as data

- [x] 2.1 Add `Forge` enum (`#[non_exhaustive]`, `Copy`, one `GitHub` variant) to `src/domain/resolution.rs` with a `Display` impl and a `token_env()` accessor returning the per-forge credential variable name
- [x] 2.2 Change `Error::RateLimited` and `Error::AuthRequired` to struct variants carrying `forge: Forge`, keeping named fields so #137 can add `retry_after` without a breaking redesign (Decision 4)
- [x] 2.3 Rewrite both variants' `#[error(...)]` text to interpolate `{forge}` and lead with the remedy, per Decision 3

## 3. Split the classification predicate

- [x] 3.1 Rename `is_recoverable()` to `is_skippable()`; keep `RateLimited` and `AuthRequired` returning `true` so a tokenless run still warns and skips rather than hard-failing
- [x] 3.2 Add `is_retryable()` returning `true` only for `RateLimited`; document on it that auth is excluded because repeating the request cannot change the outcome
- [x] 3.3 Update the sole caller `src/tidy/lock_sync.rs:52` to `is_skippable()`

## 4. Update construction sites

- [x] 4.1 Update the four `map_err` blocks in `src/infra/github/registry.rs` to construct `RateLimited { forge: Forge::GitHub }` / `AuthRequired { forge: Forge::GitHub }`
- [x] 4.2 Update test registries that construct these variants: `src/domain/resolution_testutil.rs`, `src/tidy/command_tests.rs`, `src/tidy/lock_sync_tests.rs`, `tests/common/registries.rs`; also check `src/tidy/manifest_sync.rs`, which consumes `AuthRequiredRegistry` without constructing the variant directly

## 5. Tests

- [x] 5.1 Replace the `is_recoverable_*` tests with `is_skippable` tests covering all four variants
- [x] 5.2 Add `is_retryable` tests covering every variant, including the assertion that `AuthRequired` is NOT retryable
- [x] 5.3 Add `Display` tests asserting each forge-carrying variant's message contains the forge name and its remedy env var (substring assertions, not full-string, per the test strategy)
- [x] 5.4 Update `src/domain/resolution.rs`'s existing `resolve_from_sha_describe_error_propagates` test for the new variant shape

## 6. Verify

- [x] 6.1 `mise run test` passes, including `tests/integ_tidy.rs:674` (tidy succeeds under `AuthRequiredRegistry`) unmodified — the executable form of the skippable promise
- [x] 6.2 `mise run integ` passes (no test asserts on the two reworded strings; they appear only at their definitions)
- [x] 6.3 Confirm no budget in `tests/code_health.rs` was raised and no new file was added to `src/domain/`
