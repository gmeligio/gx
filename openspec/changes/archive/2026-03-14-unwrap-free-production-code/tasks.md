## 1. Type System Redesigns

- [x] 1.1 Restructure `Scope`/`Mode` in `src/upgrade/types.rs`: move `Pinned(Version)` from `Mode` to `Scope::Pinned(ActionId, Version)`, make `Request::new` return `Self`, remove `Error::PinnedRequiresSingleScope`
- [x] 1.2 Update `src/upgrade/cli.rs`: remove all `.expect()` calls — construction is now infallible
- [x] 1.3 Update `src/upgrade/plan.rs`: change pattern matching from `Mode::Pinned` to `Scope::Pinned`
- [x] 1.4 Update tests in `src/upgrade/types.rs` and `src/upgrade/cli.rs` for the new API

## 2. StepIndex Newtype

- [x] 2.1 Create `StepIndex(u16)` in `src/domain/workflow_actions.rs` with `From<StepIndex> for i64`, `TryFrom<i64> for StepIndex`, `From<u16>`, `Display`, `PartialEq`, `Eq`, `Clone`, `Copy`, `Debug`
- [x] 2.2 Change `Location.step`, `ActionOverride.step`, and `ManifestEntryRaw.step` from `Option<usize>` to `Option<StepIndex>`
- [x] 2.3 Update `src/infra/manifest/convert.rs`: replace `i64::try_from(step).expect(...)` with `i64::from(step)`
- [x] 2.4 Update `src/infra/manifest/patch.rs`: replace `i64::try_from(step).expect(...)` with `i64::from(step)`, replace `usize::try_from(s).unwrap_or(usize::MAX)` with `StepIndex::try_from(s)`
- [x] 2.5 Update all other references to step as `usize` across `src/`

## 3. Production Expect/Unwrap Elimination

- [x] 3.1 Delete `impl Default for Registry` in `src/infra/github/mod.rs`
- [x] 3.2 Replace `.as_array_mut().expect(...)` with `.ok_or(ManifestError::Validation(...))?` in `src/infra/manifest/patch.rs`
- [x] 3.3 Simplify `dominant_version` in `src/domain/workflow_actions.rs`: replace `Version::highest(&candidates).unwrap_or_else(|| candidates[0].clone())` with `Version::highest(&candidates)`

## 4. Enable Restriction Lints

- [x] 4.1 Add 33 remaining restriction lints to `[lints.clippy]` in `Cargo.toml` as specified in `clippy-restriction-config` spec
- [x] 4.2a Add `#[expect]` annotations to test modules in `src/domain/action/` (identity.rs, resolved.rs, spec.rs, tag_selection.rs, upgrade.rs, uses_ref.rs — skip specifier.rs, already done)
- [x] 4.2b Add `#[expect]` annotations to test modules in `src/domain/` (event.rs, lock/entry.rs, lock/mod.rs, manifest/mod.rs, manifest/overrides.rs, plan.rs, resolution.rs, workflow_actions.rs)
- [x] 4.2c Add `#[expect]` annotations to test modules in `src/infra/` (github/resolve.rs, lock/convert.rs, lock/migration.rs, lock/mod.rs, manifest/convert.rs, manifest/mod.rs, manifest/patch.rs, workflow_scan/mod.rs, workflow_update.rs)
- [x] 4.2d Add `#[expect]` annotations to test modules in `src/upgrade/` (cli.rs, types.rs, report.rs — skip plan.rs, already done) and `src/config.rs`
- [x] 4.2e Add `#[expect]` annotations to test modules in `src/tidy/` (lock_sync.rs, manifest_sync.rs, mod.rs, report.rs — skip patches.rs, already done)
- [x] 4.2f Add `#[expect]` annotations to test modules in `src/lint/` (mod.rs, report.rs, sha_mismatch.rs, stale_comment.rs, unpinned.rs, unsynced_manifest.rs)
- [x] 4.2g Add `#[expect]` annotations to test modules in `src/output/` (lines.rs, printer.rs) and `src/init/report.rs`
- [x] 4.3a Add crate-level `#[expect]` annotations to `tests/common/setup.rs` and `tests/common/registries.rs`
- [x] 4.3b Add crate-level `#[expect]` annotations to integration test files: `tests/integ_upgrade.rs`, `tests/integ_tidy.rs`, `tests/integ_lint.rs`, `tests/integ_pipeline.rs`, `tests/integ_repo.rs`
- [x] 4.3c Add crate-level `#[expect]` annotations to e2e test files: `tests/e2e_github.rs`, `tests/e2e_pipeline.rs`
- [x] 4.3d Add crate-level `#[expect]` annotations to `tests/code_health.rs`

## 5. Verification

- [x] 5.1 Run `mise run clippy` and confirm zero errors
- [x] 5.2 Run `mise run test` and confirm all tests pass
