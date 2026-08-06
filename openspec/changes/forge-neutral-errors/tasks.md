## 1. Make both error enums extensible first

- [ ] 1.1 Add `#[non_exhaustive]` to `resolution::Error` in `src/domain/resolution.rs`
- [ ] 1.2 Add `#[non_exhaustive]` to `github::Error` in `src/infra/github/registry.rs`, leaving the four in-crate `map_err` arms exhaustive (Decision 5)

## 2. Carry the forge as data

- [ ] 2.1 Add `Forge` enum (`#[non_exhaustive]`, `Copy`, one `GitHub` variant) to `src/domain/resolution.rs` with a `Display` impl and a `token_env()` accessor returning the per-forge credential variable name
- [ ] 2.2 Change `Error::RateLimited` and `Error::AuthRequired` to struct variants carrying `forge: Forge`, keeping named fields so #137 can add `retry_after` without a breaking redesign (Decision 4)
- [ ] 2.3 Rewrite both variants' `#[error(...)]` text to interpolate `{forge}` and lead with the remedy, per Decision 3

## 3. Split the classification predicate

- [ ] 3.1 Rename `is_recoverable()` to `is_skippable()`; keep `RateLimited` and `AuthRequired` returning `true` so a tokenless run still warns and skips rather than hard-failing
- [ ] 3.2 Add `is_retryable()` returning `true` only for `RateLimited`; document on it that auth is excluded because repeating the request cannot change the outcome
- [ ] 3.3 Update the sole caller `src/tidy/lock_sync.rs:52` to `is_skippable()`

## 4. Update construction sites

- [ ] 4.1 Update the four `map_err` blocks in `src/infra/github/registry.rs` to construct `RateLimited { forge: Forge::GitHub }` / `AuthRequired { forge: Forge::GitHub }`
- [ ] 4.2 Update test registries that construct these variants: `src/domain/resolution_testutil.rs`, `src/tidy/command_tests.rs`, `src/tidy/lock_sync_tests.rs`, `tests/common/registries.rs`

## 5. Tests

- [ ] 5.1 Replace the `is_recoverable_*` tests with `is_skippable` tests covering all four variants
- [ ] 5.2 Add `is_retryable` tests covering all four variants, including the assertion that `AuthRequired` is NOT retryable
- [ ] 5.3 Add `Display` tests asserting each forge-carrying variant's message contains the forge name and its remedy env var (substring assertions, not full-string, per the test strategy)
- [ ] 5.4 Update `src/domain/resolution.rs`'s existing `resolve_from_sha_describe_error_propagates` test for the new variant shape

## 6. Verify

- [ ] 6.1 `mise run test` passes, including `tests/integ_tidy.rs:674` (tidy succeeds under `AuthRequiredRegistry`) unmodified — the executable form of the skippable promise
- [ ] 6.2 `mise run integ` passes; confirm no integration assertion depended on the two reworded strings
- [ ] 6.3 Confirm no budget in `tests/code_health.rs` was raised and no new file was added to `src/domain/`
