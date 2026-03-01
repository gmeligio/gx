## Why

The lock entry metadata feature (date, ref_type, repository) shipped with three bugs that make it non-functional: all entries get `ref_type = "commit"` and `date = ""` regardless of actual ref type. The root cause is a design issue where metadata resolution has two code paths — one that works and one that doesn't — combined with two GitHub API parsing bugs.

## What Changes

- **BREAKING**: Remove `ActionResolver::validate_and_correct()`. Replace with two orthogonal operations: `correct_version()` for version correction, and `resolve()` for metadata. All lock entries flow through `resolve()` → `lookup_sha()`, ensuring metadata is always populated.
- Fix `CommitDetailResponse` struct to parse `commit.committer.date` (nested) instead of top-level `committer` (GitHub user object with no date field).
- Fix `resolve_ref()` to detect GitHub Releases: after tag resolution succeeds, check for an associated Release and return `RefType::Release` instead of `RefType::Tag`.

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `resolution-metadata`: Add requirement that version correction and metadata resolution are separate operations, and that all lock entries must flow through `resolve()`.

## Impact

- `crates/gx-lib/src/domain/resolution.rs` — remove `validate_and_correct`, add `correct_version`
- `crates/gx-lib/src/commands/tidy.rs` — update `update_lock` to use new two-step flow
- `crates/gx-lib/src/infrastructure/github.rs` — fix `CommitDetailResponse` nesting, add Release detection in `resolve_ref`
- Test files: `crates/gx-lib/tests/tidy_test.rs`, `crates/gx-lib/tests/upgrade_test.rs` — update mock registries and test assertions
