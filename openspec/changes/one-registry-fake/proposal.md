## Why

`VersionRegistry` is a three-method trait with **eight** hand-written test doubles spread
across six files. A contributor adding a trait method must update all eight; choosing the
right double for a new test means reading all eight to learn how they differ.

Worse, two of those doubles are *both named `FakeRegistry`* and disagree about what the
real registry does. `tests/common/registries.rs::FakeRegistry` keys `describe_sha` on the
`(id, sha)` pair — matching `infra::github::Registry`, which calls
`get_tags_for_sha(id, sha)`. `src/domain/resolution_testutil.rs::FakeRegistry` ignores the
`sha` argument entirely and returns the action's tag list for *any* SHA. Its
`with_sha_tags(id, sha, tags)` builder takes a SHA it never reads. Six tests in
`src/tidy/lock_sync_tests.rs` configure tags under SHA `aaaa…aaaa` and then query a
different SHA (`6d1e696…`); they pass only because the fake ignores the mismatch.

That is not a merge detail — it is a fake that lies about the production contract, and the
duplication is what let it hide.

## What Changes

- Add ONE configurable in-memory `FakeRegistry` in `src/domain/resolution_testutil.rs`,
  exported from the library so both unit and integration tests use the same type.
- Express every existing scenario as **configuration on that one fake**, not as a new type:
  - canned tag lists per action (`all_tags`)
  - tags per `(action, sha)` pair (`describe_sha`), keyed on the SHA as production does
  - a fixed SHA for `lookup_sha`
  - per-action failure and whole-registry failure (`AuthRequired`, `ResolveFailed`)
  - empty commit dates
- **Delete** all eight bespoke doubles: `MockRegistry`, `AuthRequiredRegistry` (×2),
  `FakeRegistry` (the unfaithful one), `EmptyDateRegistry`, `FailingDescribeRegistry`,
  `MixedRegistry`, `NoopRegistry`.
- **Fix the six `lock_sync_tests` that relied on the unfaithful `describe_sha`.** Under a
  faithful fake they configure tags for a SHA nobody asks about. Each is re-pointed at the
  SHA the test actually feeds in, so it asserts what its name claims.
- No production code changes. `VersionRegistry`, `Error`, and `Registry` are untouched.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None.

**Spec decision — justified against the relevance gate.** `openspec/config.yaml` lists
"Skip spec: internal refactoring with no user-visible change". This change is entirely
test infrastructure: no production file is modified, no CLI output, exit code, file format,
or error message changes. It adds, removes, or changes nothing a gx user can observe, and
introduces no domain concept.

The one behavior worth flagging — `describe_sha` is keyed on the SHA — is *already* the
production contract implemented by `infra::github::Registry` and *already* covered by
`openspec/specs/action-resolution/`, which states it twice: the "Init preserves a workflow's
pinned SHA" scenario requires "the registry reports tags `[v3, v3.6, v3.6.1]` **for that
SHA**", and "SHA with no tags uses the SHA as version" fixes what happens when no tags point
at it. Writing a delta here would duplicate an existing spec — the gate's fourth skip
reason. The spec was already right; the test double was wrong, so the correction lands in
the tests.

**No `specs/` directory is present in this change, deliberately.** `openspec validate`
rejects a delta file that declares no requirements, and openspec 1.3.1 has no `skip_specs`
declaration to record a skip structurally — so the absence of the directory, plus this
paragraph, *is* the record. `openspec status` will report `specs` as `ready` and `tasks` as
`blocked` on it; per the workflow's own rule that dependencies are enablers rather than
gates, `tasks.md` is written regardless.

## Impact

- **Test-only.** `src/domain/resolution_testutil.rs` (rewritten), `src/domain/resolution.rs`
  (test module + `#[path]` include only), `src/tidy/lock_sync_tests.rs`,
  `src/tidy/command_tests.rs`, `src/tidy/manifest_sync.rs`, `src/upgrade/plan.rs`,
  `tests/common/registries.rs` (deleted), `tests/integ_pipeline.rs`, `tests/integ_tidy.rs`,
  `tests/integ_upgrade.rs`.
- **Risk:** collapsing doubles can silently weaken a suite if the unified fake defaults
  permissively. Mitigated by mutation-checking a representative sample of migrated tests
  (see design.md).
- **Known parallel work:** a `Caching<R>` decorator on another branch carries its own
  counting fake. Reconciled at merge, not here.
