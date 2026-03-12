## 1. Lint Configuration

- [x] 1.1 Replace `restriction = "deny"` in `Cargo.toml` with individual restriction lint entries (35 lints denied, rest omitted to stay at default `allow`)
- [x] 1.2 Verify `mise run clippy` reports only the expected fixable violations (no spurious lints from non-adopted rules)

## 2. Test Module Suppression

- [x] 2.1 Add `#[expect(..., reason = "...")]` annotations to all `#[cfg(test)]` modules in `src/` for safety lints (unwrap_used, expect_used, get_unwrap, indexing_slicing, string_slice, panic, assertions_on_result_states, shadow_reuse, shadow_unrelated, arithmetic_side_effects, as_conversions, wildcard_enum_match_arm, unreachable, missing_docs_in_private_items)
- [x] 2.2 Add crate-level `#[expect(...)]` annotations to all integration test files in `tests/`
- [x] 2.3 Add `#[expect(...)]` to `tests/common/setup.rs` (shared test helper module)

## 3. Mechanical Fixes — str_to_string

- [x] 3.1 Replace all `.to_string()` on `&str` values with `.to_owned()` across `src/` and `tests/`

## 4. Mechanical Fixes — redundant_test_prefix

- [x] 4.1 Remove `test_` prefix from all `#[test]` function names across `src/` and `tests/`

## 5. Mechanical Fixes — Safety (Production Code)

- [x] 5.1 Replace array/slice indexing with `.get()` or destructuring in non-test `src/` code (~15-20 occurrences)
- [x] 5.2 Replace string slicing (`&s[n..]`) with `.get(n..)` or `.strip_prefix()` in non-test `src/` code (~2 occurrences)
- [x] 5.3 Replace `.ok()` with `let _ =` or proper error handling for `unused_result_ok` (~7 occurrences)
- [x] 5.4 Eliminate variable shadowing in non-test `src/` code (~7 occurrences)
- [x] 5.5 Replace `as` conversions with `From`/`Into` or add `#[expect]` with reason (~3 occurrences)
- [x] 5.6 Replace `if cond { Some(x) } else { None }` with `.then_some()` or `.then()` (~2 occurrences)

## 6. Mechanical Fixes — Style

- [x] 6.1 Separate literal suffixes: `0u8` → `0_u8` (~3 occurrences)
- [x] 6.2 Convert anonymous trait imports to `use Trait as _` (~21 occurrences)
- [x] 6.3 Replace `#[allow(...)]` with `#[expect(..., reason = "...")]` for all existing suppressions
- [x] 6.4 Add `reason = "..."` to any remaining `#[allow]` that cannot be converted to `#[expect]`

## 7. Mechanical Fixes — impl_trait_in_params

- [x] 7.1 Replace `impl Trait` in function parameters with explicit generic parameters (`fn foo<T: Trait>(x: T)`) (~5 occurrences)

## 8. Mechanical Fixes — Other Restriction Lints

- [x] 8.1 Fix `field_scoped_visibility_modifiers` violations — remove `pub(...)` from struct fields, adjust module visibility as needed
- [x] 8.2 Fix `multiple_inherent_impl` violations — merge split impl blocks (~3 occurrences)
- [x] 8.3 Fix `ref_patterns` violations — replace `ref` bindings with alternatives (~1 occurrence)
- [x] 8.4 Fix `unneeded_field_pattern` violations — use `..` instead of named wildcards (~1 occurrence)
- [x] 8.5 Fix `unnecessary_safety_comment` violations (~1 occurrence)
- [x] 8.6 Fix `single_char_lifetime_names` — rename to descriptive 3+ char names (~6 occurrences)
- [x] 8.7 Fix `let_underscore_must_use` and `let_underscore_untyped` violations (~3 occurrences)
- [x] 8.8 Fix `wildcard_enum_match_arm` — replace `_` catch-all with explicit variants (~9 occurrences)
- [x] 8.9 Add `#[expect(clippy::print_stdout, reason = "...")]` to `Printer::print_lines` and `main.rs` timer output

## 9. Documentation

- [x] 9.1 Add doc comments to all undocumented private items (functions, structs, fields, modules, constants, type aliases) (~64+ items)
- [x] 9.2 Add terminal punctuation to all doc comment paragraphs (~140 occurrences)

## 10. Verification

- [x] 10.1 Run `mise run clippy` and confirm zero errors
- [x] 10.2 Run `rtk cargo test` and confirm all tests pass
- [x] 10.3 Verify no `#[allow]` attributes remain (all converted to `#[expect]` with reasons)
