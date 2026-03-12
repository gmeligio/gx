### Requirement: Restriction lints denied globally
The `[lints.clippy]` section in `Cargo.toml` SHALL deny the following restriction lints:

- `allow_attributes`
- `allow_attributes_without_reason`
- `arithmetic_side_effects`
- `as_conversions`
- `assertions_on_result_states`
- `dbg_macro`
- `doc_paragraphs_missing_punctuation`
- `expect_used`
- `field_scoped_visibility_modifiers`
- `get_unwrap`
- `if_then_some_else_none`
- `impl_trait_in_params`
- `indexing_slicing`
- `let_underscore_must_use`
- `let_underscore_untyped`
- `missing_docs_in_private_items`
- `multiple_inherent_impl`
- `panic`
- `print_stdout`
- `redundant_test_prefix`
- `ref_patterns`
- `shadow_reuse`
- `shadow_unrelated`
- `single_char_lifetime_names`
- `str_to_string`
- `string_slice`
- `unneeded_field_pattern`
- `unnecessary_safety_comment`
- `unreachable`
- `unseparated_literal_suffix`
- `unused_result_ok`
- `unused_trait_names`
- `unwrap_in_result`
- `unwrap_used`
- `wildcard_enum_match_arm`

#### Scenario: Clippy passes with all restriction lints enabled
- **WHEN** `mise run clippy` is executed
- **THEN** the build SHALL succeed with zero errors

#### Scenario: New unwrap in production code is caught
- **WHEN** a developer adds `.unwrap()` to a non-test function in `src/`
- **THEN** `cargo clippy` SHALL report an error for `clippy::unwrap_used`

### Requirement: Restriction lints not adopted
The `[lints.clippy]` section SHALL NOT deny the following restriction lints (they remain at their default `allow` level and SHALL NOT appear in `Cargo.toml`):

- `absolute_paths`
- `arbitrary_source_item_ordering`
- `blanket_clippy_restriction_lints`
- `default_numeric_fallback`
- `exhaustive_enums`
- `exhaustive_structs`
- `implicit_return`
- `iter_over_hash_type`
- `min_ident_chars`
- `missing_inline_in_public_items`
- `mod_module_files`
- `module_name_repetitions` (deferred to separate proposal)
- `non_ascii_literal`
- `pattern_type_mismatch`
- `pub_use` (deferred to separate proposal)
- `pub_with_shorthand`
- `question_mark_used`
- `same_name_method`
- `self_named_module_files`
- `single_call_fn`
- `std_instead_of_alloc`
- `std_instead_of_core`

#### Scenario: Implicit return is not flagged
- **WHEN** a function uses a tail expression without `return`
- **THEN** `cargo clippy` SHALL NOT report an error

#### Scenario: Single-char identifiers are not flagged
- **WHEN** a variable is named `i`, `k`, or `v` in an iterator or closure
- **THEN** `cargo clippy` SHALL NOT report an error

### Requirement: Test module lint suppression
All `#[cfg(test)]` modules in `src/` and all integration test files in `tests/` SHALL use `#[expect]` to suppress safety lints that are inappropriate for test code. The suppressed lints SHALL be:

- `clippy::unwrap_used`
- `clippy::expect_used`
- `clippy::get_unwrap`
- `clippy::indexing_slicing`
- `clippy::string_slice`
- `clippy::panic`
- `clippy::assertions_on_result_states`
- `clippy::shadow_reuse`
- `clippy::shadow_unrelated`
- `clippy::arithmetic_side_effects`
- `clippy::as_conversions`
- `clippy::wildcard_enum_match_arm`
- `clippy::unreachable`
- `clippy::missing_docs_in_private_items`

Each `#[expect]` annotation SHALL include a `reason` string.

#### Scenario: Test module with unwrap compiles
- **WHEN** a `#[test]` function inside a `#[cfg(test)]` module calls `.unwrap()`
- **THEN** `cargo clippy` SHALL NOT report an error because the module-level `#[expect]` suppresses it

#### Scenario: Integration test with indexing compiles
- **WHEN** a test in `tests/` uses array indexing like `parts[0]`
- **THEN** `cargo clippy` SHALL NOT report an error because the crate-level `#[expect]` suppresses it

### Requirement: Production code fixes for str_to_string
All occurrences of `.to_string()` called on a `&str` value in production and test code SHALL be replaced with `.to_owned()`.

#### Scenario: &str uses to_owned
- **WHEN** code converts a `&str` to a `String`
- **THEN** the code SHALL use `.to_owned()` instead of `.to_string()`

### Requirement: Production code fixes for redundant_test_prefix
All `#[test]` functions with a `test_` prefix SHALL have the prefix removed.

#### Scenario: Test function naming
- **WHEN** a function is annotated with `#[test]`
- **THEN** the function name SHALL NOT start with `test_`

### Requirement: Production code fixes for indexing_slicing in non-test code
All array/slice indexing in non-test code SHALL be replaced with safe alternatives (`.get()`, destructuring, or iterator methods).

#### Scenario: Version parsing uses safe indexing
- **WHEN** code parses version parts from a split string
- **THEN** the code SHALL use `.get(n)` or pattern matching instead of `parts[n]`

### Requirement: Production code fixes for string_slice in non-test code
All string slicing (`&s[n..]`) in non-test code SHALL be replaced with safe alternatives (`.get(n..)`, `.strip_prefix()`, or `.split_at()`).

#### Scenario: Specifier parsing uses safe slicing
- **WHEN** code strips a prefix character from a specifier string
- **THEN** the code SHALL use `.get(1..)` or `.strip_prefix()` instead of `&raw[1..]`

### Requirement: Documentation completeness
All private items (functions, structs, fields, modules, constants, type aliases) SHALL have doc comments. Doc comment paragraphs SHALL end with terminal punctuation (`.`, `!`, or `?`).

#### Scenario: Private function has doc comment
- **WHEN** a non-public function exists in `src/`
- **THEN** it SHALL have a `///` doc comment above it

#### Scenario: Doc paragraph ends with punctuation
- **WHEN** a doc comment paragraph exists
- **THEN** it SHALL end with `.`, `!`, or `?`

### Requirement: Allow attributes use expect with reason
All lint suppression annotations SHALL use `#[expect(..., reason = "...")]` instead of `#[allow(...)]`.

#### Scenario: Lint suppression with reason
- **WHEN** a lint needs to be suppressed
- **THEN** the code SHALL use `#[expect(lint_name, reason = "explanation")]`
- **AND** the reason SHALL explain why the suppression is necessary
