## Why

The project enforces strict clippy lints (`pedantic`, `perf`, `nursery` all set to `deny`) plus `rust-2018-idioms`. After the edition 2024 upgrade and recent refactoring, 113 clippy errors have accumulated across 29 source files. CI cannot pass and Zed shows diagnostics everywhere. These need to be fixed while keeping the tight lint configuration.

## What Changes

- Fix 62 `use_self` violations: replace explicit enum/struct names with `Self` inside `impl` blocks
- Fix 21 `missing_const_for_fn` violations: add `const` to trivial constructors and accessors
- Fix 10 `option_if_let_else` violations: rewrite `if let Some(x) = ... { } else { }` to `map_or_else`
- Fix 5 `elided_lifetimes_in_paths` violations: add `<'_>` to `LintContext` in the `Lint` trait and implementors
- Fix 3 `redundant_clone` violations: remove unnecessary `.clone()` calls
- Fix 3 `non_binding_let` violations: fix `let _ =` on types with destructors
- Fix 9 remaining misc violations (`derive_partial_eq_without_eq`, `map_or`/`unwrap_or` with function calls, `unused_mut`, `needless_collect`, `pub_crate_in_private_module`, `first_doc_comment_paragraph_too_long`)

## Capabilities

### New Capabilities

None. This is a code quality fix with no new behavior.

### Modified Capabilities

None. No spec-level behavior changes.

## Impact

- 29 source files across `src/domain/`, `src/infra/`, `src/lint/`, `src/output/`, `src/tidy/`
- No behavioral changes, no API changes, no dependency changes
- All fixes are mechanical and guided by clippy's own suggestions
