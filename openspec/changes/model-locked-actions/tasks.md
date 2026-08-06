## 1. Pin the format baseline first

- [x] 1.1 Add a byte-identity test to `src/infra/lock/tests.rs`: parse a literal two-tier `gx.lock` string covering a tag pin, a branch pin, a bare-commit pin, and two specs sharing one commit; re-serialize; assert the output equals the literal input exactly.
- [x] 1.2 Add a sort-order test alongside it whose *input* is deliberately unsorted (at least two action IDs, and two specifiers under one ID, in reverse lexicographic order) and whose *expected output* is a literal sorted string distinct from that input. Byte-identity alone cannot fail on sort order — a fixture that is already sorted serializes in read order whether the sort is correct, inverted, or dropped.
- [x] 1.3 Run both tests before touching any domain code and confirm they pass against the current implementation. A test that only passes after the refactor proves nothing.

## 2. Introduce the type

- [x] 2.1 Create `src/domain/locked_action.rs` with `LockedAction<'lock> { spec: &'lock Spec, reference: &'lock ResolvedRef, commit: &'lock Commit }`, a crate-visible constructor, and accessors `id()`, `specifier()`, `reference()`, `sha()`, `repository()`, `version_label()`. Doc-comment it as the read view over one lock row, naming `LockEntry` as the storage counterpart, and state on the type that accessors surface stored values verbatim with no completeness guarantee — an incomplete row yields an empty `repository()`; callers needing a guarantee ask `Lock::is_complete`.
- [x] 2.2 Register `pub mod locked_action;` in `src/domain/mod.rs`.
- [x] 2.3 Add unit tests at the bottom of `locked_action.rs`: accessors match the viewed row; `version_label()` is the SHA for a bare-commit pin and the tag string otherwise.

## 3. Make the lock yield it

- [x] 3.1 Change `Lock::entries()` to return `impl Iterator<Item = LockedAction<'_>>`, constructing one per `(spec, entry)` pair. Update its doc comment.
- [x] 3.2 Add a `lock.rs` test asserting `entries()` yields one `LockedAction` per stored row, each carrying its own spec.

## 4. Update the one production consumer

- [x] 4.1 Rewrite `build_lock_document` in `src/infra/lock/format.rs` to iterate `LockedAction`: sort by `id()` then `specifier()`, write the `[resolutions]` tier from `version_label()`, and key the `[actions]` dedup map on `(id(), version_label())`. Preserve the existing sort and first-wins dedup semantics exactly. `version_label()` is not a new expression: `format.rs:140` already writes `entry.version_label()` into the `version` slot and `:155` already keys dedup on it, so `LockedAction::version_label()` must forward to the identical `LockEntry::version_label()` — verify this rather than reimplementing the label logic.
- [x] 4.2 Confirm the 1.1 byte-identity test, the 1.2 sort-order test, and the existing `format.rs` round-trip and sort-order tests still pass.

## 5. Verify

- [x] 5.1 Run `mise run test` — must pass, including the byte-identity test and `tests/code_health.rs` budgets, with no budget number raised.
- [x] 5.2 Run `mise run integ`.
- [x] 5.3 Confirm `src/domain/` is at 7/8 files and `src/domain/action/` is untouched at 8/8.
- [x] 5.4 Confirm no file under `src/infra/github/**`, `src/output/lines.rs`, `src/main.rs`, `src/lint/**`, `src/audit/**`, `src/domain/action/tag_selection.rs`, or `src/domain/resolution.rs` was modified.
