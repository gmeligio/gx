## 1. Build the unified fake

- [ ] 1.1 Rewrite `src/domain/resolution_testutil.rs` as a single `FakeRegistry` with fields for: per-action tag lists (`all_tags`), per-`(action, sha)` tag lists (`describe_sha`), an optional fixed lookup SHA, an optional canned `lookup_sha` result, an optional canned `all_tags` result, a whole-registry error, a per-action error map, a `describe_sha` error, and an empty-dates flag
- [ ] 1.2 Implement `describe_sha` keyed on `(id, sha)` — an unconfigured SHA yields empty tags, never the action's full tag list (design D2)
- [ ] 1.3 Implement `lookup_sha` defaulting to the deterministic 40-hex `fake_sha(id, version)`, overridable by `with_fixed_sha` then by a canned result (design D4)
- [ ] 1.4 Add builder methods, each with a current caller: `with_all_tags`, `with_sha_tags`, `with_fixed_sha`, `with_lookup_result`, `with_tags_result`, `failing`, `failing_action`, `failing_describe`, `with_empty_dates`
- [ ] 1.5 Keep `fake_sha` a public associated function — ~30 existing call sites assert against it
- [ ] 1.6 Write private-item and field docs on every field and method (clippy strict requires them)

## 2. Export the fake to integration tests

- [ ] 2.1 Change `src/domain/resolution.rs`'s testutil include from `#[cfg(test)] pub(crate) mod` to a plain `pub mod` (no cfg, no feature — see design D1 for why the feature-gate alternative fails under `--locked`), keeping the diff confined to that include and the test-double module
- [ ] 2.2 Confirm the fake is reachable as `gx::domain::resolution::testutil::FakeRegistry` from integration tests
- [ ] 2.3 Satisfy strict clippy on the promoted module: field docs, `#[must_use]` on builders and getters, and a `Default` impl

## 3. Migrate unit-test doubles

- [ ] 3.1 Delete `MockRegistry` from `src/domain/resolution.rs`; migrate its tests to `with_lookup_result` / `with_tags_result`
- [ ] 3.2 Delete `AuthRequiredRegistry` from `src/domain/resolution_testutil.rs`; migrate `src/tidy/manifest_sync.rs` to `.failing(AuthRequired)`
- [ ] 3.3 Delete `NoopRegistry` from `src/tidy/command_tests.rs`; migrate to `.failing(AuthRequired)`
- [ ] 3.4 Delete `MixedRegistry` from `src/tidy/lock_sync_tests.rs`; migrate to `.failing_action("actions/checkout", AuthRequired)` — verify other actions still resolve and `all_tags` still errors, matching the old double
- [ ] 3.5 Migrate `src/upgrade/plan.rs` tests to the unified fake

## 4. Correct the six tests that relied on the unfaithful describe_sha

- [ ] 4.1 In `src/tidy/lock_sync_tests.rs`, re-point each `with_sha_tags` call (lines ~106, 226, 264, 302, 341, 381) from SHA `aaaa…aaaa` to the workflow SHA the test actually feeds in (`6d1e696…`), leaving every assertion unchanged
- [ ] 4.2 Confirm each of the six still passes and still exercises the path its name describes — a test that now takes the `ResolvedRef::Commit` branch instead has been silently rewritten and must be flagged, not accepted

## 5. Migrate integration-test doubles

- [ ] 5.1 Delete `tests/common/registries.rs` entirely; drop its `mod` declaration from `tests/common/mod.rs`
- [ ] 5.2 Repoint `tests/integ_pipeline.rs`, `tests/integ_tidy.rs`, `tests/integ_upgrade.rs` to `gx::domain::resolution::testutil::FakeRegistry`
- [ ] 5.3 Replace `EmptyDateRegistry` with `.with_empty_dates()` and `FailingDescribeRegistry` with `.failing_describe(...)`, preserving the exact `ResolveFailed` reason string the old double produced (`"Github API returned status 422 Unprocessable Entity"`)
- [ ] 5.3a Confirm at least one migrated test asserts on that reason string — it can reach user-facing output, so if nothing pins it the preservation is unenforced and can drift silently. If no assertion exists, report that rather than adding one (out of scope)
- [ ] 5.4 Replace `AuthRequiredRegistry` in `tests/integ_tidy.rs` with `.failing(AuthRequired)`

## 6. Mutation-check the migration

- [ ] 6.1 `sha_first_lock_uses_workflow_sha_and_most_specific_version` — break `select_most_specific_tag` to pick the least specific tag; confirm the test FAILS
- [ ] 6.2 `out_of_range_pinned_sha_is_reresolved_within_range` — make `matches_version` always return `true`; confirm the test FAILS
- [ ] 6.3 `update_lock_recoverable_errors_are_skipped` — make `is_skippable` return `false`; confirm the test FAILS
- [ ] 6.4 `update_lock_recoverable_errors_are_skipped` — mutate the FAKE this time: make `failing_action` ignore its action filter, first failing every action then failing none; confirm the test FAILS both ways. This is the `MixedRegistry` successor and the only knob modelling partial failure, so it sits on the load-bearing error-classification guardrail (design, Automated Test Strategy §4)
- [ ] 6.5 The `with_empty_dates` test in `integ_pipeline` — make the flag a no-op; confirm the test FAILS
- [ ] 6.6 The `failing_describe` test in `integ_pipeline` — make it return `Ok`; confirm the test FAILS
- [ ] 6.7 An `all_tags`-driven test in `integ_upgrade` — make `with_all_tags` drop its tags; confirm the test FAILS
- [ ] 6.8 Revert every mutation; confirm `mise run test` and `mise run integ` are green again; record which checks were run and their results for the final report

## 7. Verify the gates

- [ ] 7.1 `mise run test` passes
- [ ] 7.2 `mise run integ` passes
- [ ] 7.3 Confirm no budget number in `tests/code_health.rs` was raised, no `#[ignore]` added, no test deleted, and `src/domain/` is still within the 8-file limit
- [ ] 7.4 Confirm exactly one fake remains — use `grep -rn "VersionRegistry for" src/ tests/`, not `grep "impl VersionRegistry"`: the latter misses fully-qualified impls like `impl crate::domain::resolution::VersionRegistry for NoopRegistry` (which is how `command_tests.rs` writes it today). Expect exactly two hits: the real `Registry` in `src/infra/` and the unified fake
