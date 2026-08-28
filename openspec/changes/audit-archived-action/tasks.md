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

- [x] 2.1 Add `ArchivedAction => "archived-action"` to the `rule_ids!` list in
      `src/audit/check_name.rs`. Confirm this is the only edit that file needs.
- [x] 2.2 Add `src/audit/archived.rs` with the check: takes an `&AuditTarget` and a
      `&dyn RepoMetadata`, derives the repository via `ActionId::base_repo()`, and returns
      `Option<Finding>`.
- [x] 2.3 Implement the archived case — `warn`, message naming the action, the date part of
      `pushed_at`, and migration as the remedy.
- [x] 2.4 Implement the failure case — `error`, message leading with the uncertainty and
      naming the underlying reason.
- [x] 2.5 Confirm the not-archived case is the only path returning `None`.
- [x] 2.6 Wire the check into `collect_findings` in `src/audit/mod.rs`, and construct
      `RestRepoMetadata` from the token-bearing `Registry` in `Audit::run`.
- [x] 2.7 Verify `src/audit/target.rs` needs no change, per design Decision 1.

## 3. Tests

- [x] 3.1 Archived repository produces a `warn` finding; assert the message contains the
      action name, the date, and migration guidance.
- [x] 3.2 Non-archived repository produces no finding; assert the fake recorded the lookup,
      so the test cannot pass by the check never running.
- [x] 3.3 Subpath action is looked up by its base repository; assert on the slug the fake
      recorded.
- [x] 3.4 Subpath action in an archived repository is reported under its full action name
      including the subpath.
- [x] 3.5 Failed lookup produces an `error` finding naming the action and the reason.
- [x] 3.6 A failed lookup for one entry does not suppress a finding for another entry,
      whichever order they are processed in — assert with the failing entry both before and
      after the archived one, so the test cannot pass on a lucky ordering.
- [x] 3.7 A `pushed_at` with no `T` is shown whole rather than blanked, per design
      Decision 8.
- [x] 3.8 `archived-action` round-trips through its literal string and appears in
      `CheckName::ALL`.

## 4. Verification

- [x] 4.1 Mutation-test every scenario in section 3: break the behavior, confirm the test
      fails, restore. Nine mutations run, all caught:

      | Mutation | Caught by |
      |---|---|
      | `if !meta.archived` never returns `None` | `active_repository_is_not_reported_but_is_still_looked_up`, `branch_entry_produces_a_finding` |
      | failed lookup returns `None` | `failed_lookup_is_an_error_naming_the_action_and_the_reason`, `a_failed_lookup_does_not_suppress_other_entries` |
      | failed lookup downgraded to `warn` | same two |
      | archived escalated to `error` | `archived_repository_is_reported_as_a_warning`, `every_check_runs_over_every_entry` |
      | `day_of` never truncates | `archived_repository_is_reported_as_a_warning`, `a_timestamp_without_a_time_is_shown_whole` |
      | lookup uses the full action id | `subpath_action_is_looked_up_by_its_base_repository` |
      | finding names the base repo | `subpath_action_is_reported_under_its_full_name` |
      | `archived` gains `#[serde(default)]` | `a_payload_missing_archived_does_not_decode_as_not_archived` |
      | check unwired from `collect_findings` | compile error — the seam parameter cannot go unused |
- [x] 4.2 Confirm `src/audit/*.rs` is at 5 files and `src/infra/github/*.rs` at 8 — the
      latter exactly at the 8-file budget, which is why the fake must not be a second file.
- [x] 4.3 `mise run test` passes with no numeric budget in `tests/code_health.rs` raised.
- [x] 4.4 `mise run integ` passes.
- [x] 4.5 Clippy strict gate passes — pedantic, private-item and field docs, `#[expect]`
      fulfilled, `#[cfg(test)]` at file bottom.
