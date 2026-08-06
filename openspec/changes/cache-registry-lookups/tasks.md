## 1. Dependency

- [ ] 1.1 Add `elsa = "1.11"` to `[dependencies]` in `Cargo.toml` (default features only — `indexmap` stays off)
- [ ] 1.2 Confirm `mise run lock:check` passes and `stable_deref_trait` does not gain a second version in `Cargo.lock`

## 2. The caching decorator

- [ ] 2.1 Create `src/infra/registry/mod.rs` declaring the `caching` submodule and re-exporting `Caching`
- [ ] 2.2 Register `pub mod registry;` in `src/infra/mod.rs`
- [ ] 2.3 Create `src/infra/registry/caching.rs` with `Caching<R: VersionRegistry>` holding the inner registry plus four `FrozenMap` fields, and a `new(inner: R)` constructor
- [ ] 2.4 Implement `VersionRegistry for Caching<R>`: each method checks its map, calls the inner registry on a miss, stores only on `Ok`, and returns a clone of the stored value
- [ ] 2.5 Key each map on exactly that method's arguments — `(ActionId, Version)` for `lookup_sha`, `(ActionId, CommitSha)` for `tags_for_sha` and `describe_sha`, `ActionId` for `all_tags`
- [ ] 2.6 Add doc comments on the struct and every field (clippy denies `missing_docs_in_private_items`)

## 3. Delete `ShaIndex`

- [ ] 3.1 Remove `ShaIndex`, its `Default` impl, and the now-unused imports from `src/domain/action/tag_selection.rs`, keeping `select_most_specific_tag` and `parse_version_components` untouched
- [ ] 3.2 Drop the `sha_index` parameter from `ActionResolver::resolve_from_sha` in `src/domain/resolution.rs` and call `self.registry.describe_sha(id, sha)` directly — keep the diff confined to that parameter and its import
- [ ] 3.3 Drop the `sha_index` parameter from `update_lock` and `populate_lock_entry` in `src/tidy/lock_sync.rs`
- [ ] 3.4 Drop the `sha_index` parameter from `upgrade_sha_versions_to_tags` in `src/tidy/manifest_sync.rs`, calling `resolver.registry().describe_sha(...)` instead of `sha_index.get_or_describe(...)`
- [ ] 3.5 Remove the `ShaIndex::new()` construction and both argument sites in `src/tidy/command.rs`
- [ ] 3.6 Update all test call sites — `src/domain/resolution.rs` tests, `src/tidy/lock_sync_tests.rs`, `src/tidy/manifest_sync.rs` tests — dropping the argument while keeping every assertion unchanged

## 4. Wire the composition roots

- [ ] 4.1 Wrap the registry in `Caching::new(...)` at `src/init/command.rs`
- [ ] 4.2 Wrap the registry in `Caching::new(...)` at `src/tidy/command.rs`
- [ ] 4.3 Wrap the registry in `Caching::new(...)` at `src/upgrade/command.rs`
- [ ] 4.4 Confirm `src/upgrade/plan.rs` is unchanged — `service.registry().all_tags(...)` at both call sites must be cached with no edit, since `registry()` already returns `&R`

## 5. Tests

- [ ] 5.1 Add a counting fake registry local to `caching.rs`'s test module, with a `Cell<usize>` per method
- [ ] 5.2 Four dedup tests — one per trait method: call twice with identical arguments, assert equal results and an inner call count of 1 (`tags_for_sha` has no production caller but is cached and tested for uniformity)
- [ ] 5.3 A key-discrimination test: call with different arguments, assert the inner call count is 2 and each result is its own
- [ ] 5.4 An error test: a fake that always errors, called twice, must show an inner call count of 2 (errors are not cached)
- [ ] 5.5 Confirm `git diff --stat` shows no change to `src/domain/resolution_testutil.rs` or `tests/common/registries.rs` — the eight existing doubles stay unaware of caching

## 6. Gate

- [ ] 6.1 `mise run test` passes, with no budget number in `tests/code_health.rs` raised
- [ ] 6.2 `mise run integ` passes
- [ ] 6.3 Confirm `git diff --stat` against the base shows more lines deleted than added across `src/`
