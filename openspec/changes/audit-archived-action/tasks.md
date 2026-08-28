## 1. Repository-metadata seam

- [x] 1.1 Add `src/infra/github/repo_meta.rs`: the `RepoMeta` struct (`archived`,
      `pushed_at`), the `RepoMetadata` trait, and `RestRepoMetadata`, the real adapter
      wrapping `Registry` and calling `GET /repos/{owner}/{repo}` via `Registry::get_json`,
      which already sequences authenticate, classify status, then decode. No new dependency.
- [x] 1.2 Add `FakeRepoMetadata` inside `repo_meta.rs`, under `#[cfg(test)]` — **not** a
      `repo_meta_fake.rs` of its own, per design Decision 2. It returns a canned `RepoMeta`
      or a canned failure, recording every repository slug it was asked about so tests can
      assert which lookups happened.
- [x] 1.3 Register the one new module in `src/infra/github/mod.rs` and re-export the public
      names.
- [x] 1.4 Unit-test the adapter's decoding against a literal payload in GitHub's documented
      response shape, including the field names, so a rename fails a test.

## 2. The check

- [ ] 2.1 Add `ArchivedAction => "archived-action"` to the `rule_ids!` list in
      `src/audit/check_name.rs`. Confirm this is the only edit that file needs.
- [ ] 2.2 Add `src/audit/archived.rs` with the check: takes an `&AuditTarget` and a
      `&dyn RepoMetadata`, derives the repository via `ActionId::base_repo()`, and returns
      `Option<Finding>`.
- [ ] 2.3 Implement the archived case — `warn`, message naming the action, the date part of
      `pushed_at`, and migration as the remedy.
- [ ] 2.4 Implement the failure case — `error`, message leading with the uncertainty and
      naming the underlying reason.
- [ ] 2.5 Confirm the not-archived case is the only path returning `None`.
- [ ] 2.6 Wire the check into `collect_findings` in `src/audit/mod.rs`, and construct
      `RestRepoMetadata` from the token-bearing `Registry` in `Audit::run`.
- [ ] 2.7 Verify `src/audit/target.rs` needs no change, per design Decision 1.

## 3. Tests

- [ ] 3.1 Archived repository produces a `warn` finding; assert the message contains the
      action name, the date, and migration guidance.
- [ ] 3.2 Non-archived repository produces no finding; assert the fake recorded the lookup,
      so the test cannot pass by the check never running.
- [ ] 3.3 Subpath action is looked up by its base repository; assert on the slug the fake
      recorded.
- [ ] 3.4 Subpath action in an archived repository is reported under its full action name
      including the subpath.
- [ ] 3.5 Failed lookup produces an `error` finding naming the action and the reason.
- [ ] 3.6 A failed lookup for one entry does not suppress a finding for another entry,
      whichever order they are processed in — assert with the failing entry both before and
      after the archived one, so the test cannot pass on a lucky ordering.
- [ ] 3.7 A `pushed_at` with no `T` is shown whole rather than blanked, per design
      Decision 8.
- [ ] 3.8 `archived-action` round-trips through its literal string and appears in
      `CheckName::ALL`.

## 4. Verification

- [ ] 4.1 Mutation-test every scenario in section 3: break the behavior, confirm the test
      fails, restore. Record which mutations were run and their results.
- [ ] 4.2 Confirm `src/audit/*.rs` is at 5 files and `src/infra/github/*.rs` at 8 — the
      latter exactly at the 8-file budget, which is why the fake must not be a second file.
- [ ] 4.3 `mise run test` passes with no numeric budget in `tests/code_health.rs` raised.
- [ ] 4.4 `mise run integ` passes.
- [ ] 4.5 Clippy strict gate passes — pedantic, private-item and field docs, `#[expect]`
      fulfilled, `#[cfg(test)]` at file bottom.
