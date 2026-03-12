## 1. Fix `use_self` — unnecessary structure name repetition (62 errors)

Replace explicit type names with `Self` inside `impl` blocks.

- [x] 1.1 Fix `src/domain/action/specifier.rs` (28 occurrences)
- [x] 1.2 Fix `src/output/lines.rs` (11 occurrences)
- [x] 1.3 Fix `src/domain/action/uses_ref.rs` (8 occurrences)
- [x] 1.4 Fix `src/domain/event.rs` (6 occurrences)
- [x] 1.5 Fix `src/infra/workflow_scan/mod.rs` (4 occurrences)
- [x] 1.6 Fix `src/domain/action/identity.rs` (2 occurrences)
- [x] 1.7 Fix `src/domain/lock/mod.rs` (1 occurrence)
- [x] 1.8 Fix `src/domain/manifest/mod.rs` (1 occurrence)
- [x] 1.9 Fix `src/infra/workflow_update.rs` (1 occurrence)

## 2. Fix `missing_const_for_fn` — functions that could be const (21 errors)

Add `const` keyword to trivial constructors and accessors.

- [x] 2.1 Fix `src/domain/resolution.rs` (4 occurrences)
- [x] 2.2 Fix `src/domain/action/specifier.rs` (3 occurrences)
- [x] 2.3 Fix `src/domain/action/spec.rs` (2 occurrences)
- [x] 2.4 Fix `src/domain/action/upgrade.rs` (2 occurrences)
- [x] 2.5 Fix `src/domain/lock/entry.rs` (2 occurrences)
- [x] 2.6 Fix `src/domain/manifest/mod.rs` (2 occurrences)
- [x] 2.7 Fix `src/domain/plan.rs` (2 occurrences)
- [x] 2.8 Fix `src/domain/action/resolved.rs` (1 occurrence)
- [x] 2.9 Fix `src/domain/action/uses_ref.rs` (1 occurrence)
- [x] 2.10 Fix `src/domain/lock/mod.rs` (1 occurrence)
- [x] 2.11 Fix `src/output/log_file.rs` (1 occurrence)

## 3. Fix `option_if_let_else` — use `map_or_else`/`map_or` instead (11 errors)

Rewrite `if let Some(x) = opt { ... } else { ... }` to functional style.

- [x] 3.1 Fix `src/domain/action/upgrade.rs` (3 occurrences)
- [x] 3.2 Fix `src/tidy/manifest_sync.rs` (2 occurrences)
- [x] 3.3 Fix `src/domain/action/uses_ref.rs` (1 occurrence)
- [x] 3.4 Fix `src/domain/lock/mod.rs` (1 occurrence)
- [x] 3.5 Fix `src/domain/resolution.rs` (1 occurrence)
- [x] 3.6 Fix `src/domain/resolution_testutil.rs` (1 occurrence)
- [x] 3.7 Fix `src/tidy/lock_sync.rs` — `map_or_else` (1 occurrence)
- [x] 3.8 Fix `src/tidy/lock_sync.rs` — `map_or` (1 occurrence)

## 4. Fix `elided_lifetimes_in_paths` — add `<'_>` lifetime annotation (5 errors)

Add anonymous lifetime `<'_>` to `LintContext` in the trait definition and all implementors.

- [x] 4.1 Fix `src/lint/mod.rs` — trait definition (1 occurrence)
- [x] 4.2 Fix `src/lint/sha_mismatch.rs` (1 occurrence)
- [x] 4.3 Fix `src/lint/stale_comment.rs` (1 occurrence)
- [x] 4.4 Fix `src/lint/unpinned.rs` (1 occurrence)
- [x] 4.5 Fix `src/lint/unsynced_manifest.rs` (1 occurrence)

## 5. Fix `redundant_clone` — remove unnecessary `.clone()` (3 errors)

- [x] 5.1 Fix `src/infra/workflow_update.rs` (2 occurrences)
- [x] 5.2 Fix `src/domain/lock/mod.rs` (1 occurrence)

## 6. Fix `non_binding_let` — destructors dropped immediately (3 errors)

Fix `let _ = expr` on types with destructors to use proper binding or `drop()`.

- [x] 6.1 Fix `src/output/log_file.rs` (2 occurrences)
- [x] 6.2 Fix `src/tidy/mod.rs` (1 occurrence)

## 7. Fix remaining one-off lint errors (9 errors)

- [x] 7.1 Fix `src/output/lines.rs` — `derive_partial_eq_without_eq`: add `Eq` derive
- [x] 7.2 Fix `src/domain/action/specifier.rs` — `needless_collect`: remove unnecessary `.collect()`
- [x] 7.3 Fix `src/infra/lock/convert.rs` — `or_fun_call`: use lazy `map_or_else` instead of `map_or` with function call
- [x] 7.4 Fix `src/infra/manifest/mod.rs` — `or_fun_call`: use `.unwrap_or_else` instead of `.unwrap_or` with function call
- [x] 7.5 Fix `src/domain/action/tag_selection.rs` — `doc_markdown`: shorten first doc comment paragraph
- [x] 7.6 Fix `src/infra/lock/mod.rs` — `doc_markdown`: shorten first doc comment paragraph
- [x] 7.7 Fix `src/domain/resolution_testutil.rs` — `redundant_pub_crate` (2 occurrences): remove `pub(crate)` from private module structs
- [x] 7.8 Fix `src/tidy/lock_sync.rs` — `needless_pass_by_ref_mut`: change `&mut` to `&` for unused mutable reference

## 8. Verify

- [x] 8.1 Run `mise run clippy` and confirm zero errors
- [x] 8.2 Run `mise run test` (if available) to confirm no regressions
