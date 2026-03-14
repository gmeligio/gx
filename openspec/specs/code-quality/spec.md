## Clippy Configuration

### Requirement: Lint configuration in Cargo.toml
The `[lints.clippy]` section in `Cargo.toml` SHALL include individually selected restriction lints in addition to the existing group-level denials (`pedantic`, `perf`, `nursery`). The blanket `restriction = "deny"` SHALL NOT be used. The section SHALL also deny `module_name_repetitions` and `pub_use` restriction lints.

### Requirement: Restriction lints denied globally
The `[lints.clippy]` section in `Cargo.toml` SHALL deny the following restriction lints:

- `allow_attributes`, `allow_attributes_without_reason`, `arithmetic_side_effects`, `as_conversions`
- `assertions_on_result_states`, `dbg_macro`, `doc_paragraphs_missing_punctuation`, `expect_used`
- `field_scoped_visibility_modifiers`, `get_unwrap`, `if_then_some_else_none`, `impl_trait_in_params`
- `indexing_slicing`, `let_underscore_must_use`, `let_underscore_untyped`, `missing_docs_in_private_items`
- `multiple_inherent_impl`, `panic`, `print_stdout`, `redundant_test_prefix`, `ref_patterns`
- `shadow_reuse`, `shadow_unrelated`, `single_char_lifetime_names`, `str_to_string`, `string_slice`
- `unneeded_field_pattern`, `unnecessary_safety_comment`, `unreachable`, `unseparated_literal_suffix`
- `unused_result_ok`, `unused_trait_names`, `unwrap_in_result`, `unwrap_used`, `wildcard_enum_match_arm`

### Requirement: Restriction lints not adopted
The following restriction lints SHALL NOT be denied (remain at `allow`): `absolute_paths`, `arbitrary_source_item_ordering`, `blanket_clippy_restriction_lints`, `default_numeric_fallback`, `exhaustive_enums`, `exhaustive_structs`, `implicit_return`, `iter_over_hash_type`, `min_ident_chars`, `missing_inline_in_public_items`, `mod_module_files`, `module_name_repetitions` (separate), `non_ascii_literal`, `pattern_type_mismatch`, `pub_use` (separate), `pub_with_shorthand`, `question_mark_used`, `same_name_method`, `self_named_module_files`, `single_call_fn`, `std_instead_of_alloc`, `std_instead_of_core`.

### Requirement: Test module lint suppression
All `#[cfg(test)]` modules and integration test files SHALL use `#[expect]` to suppress safety lints inappropriate for test code:
`unwrap_used`, `expect_used`, `get_unwrap`, `indexing_slicing`, `string_slice`, `panic`, `assertions_on_result_states`, `shadow_reuse`, `shadow_unrelated`, `arithmetic_side_effects`, `as_conversions`, `wildcard_enum_match_arm`, `unreachable`, `missing_docs_in_private_items`.

Each `#[expect]` annotation SHALL include a `reason` string.

### Requirement: Allow attributes use expect with reason
All lint suppression annotations SHALL use `#[expect(..., reason = "...")]` instead of `#[allow(...)]`.

---

## Code Conventions

### Requirement: Production code fixes for str_to_string
All `.to_string()` on `&str` SHALL be replaced with `.to_owned()`.

### Requirement: Production code fixes for redundant_test_prefix
All `#[test]` functions with a `test_` prefix SHALL have the prefix removed.

### Requirement: Safe indexing in non-test code
All array/slice indexing in non-test code SHALL use `.get()`, destructuring, or iterator methods.

### Requirement: Safe string slicing in non-test code
All string slicing (`&s[n..]`) in non-test code SHALL use `.get(n..)`, `.strip_prefix()`, or `.split_at()`.

### Requirement: Documentation completeness
All private items SHALL have doc comments. Doc comment paragraphs SHALL end with terminal punctuation (`.`, `!`, or `?`).

---

## Type Naming

### Requirement: Types do not repeat their module name
All type names SHALL NOT include their containing module name as a prefix or suffix, unless the prefix is part of the domain concept (e.g., `ActionId` where `Id` alone is ambiguous). Consumers access types via module qualifier: `tidy::Error`, not `TidyError`.

### Requirement: No pub use re-exports in facade modules
Module facade files SHALL NOT use `pub use` to re-export types from submodules. Consumers SHALL import types from their defining module.

### Requirement: Qualified import convention
When multiple modules define a type with the same short name, consumers SHALL use the module as a qualifier (`tidy::Error`, `lint::Error`) rather than renaming with `as`.

---

## Architecture Enforcement

### Requirement: Layer dependency direction enforcement
Domain modules (`src/domain/**/*.rs`) SHALL NOT import from command modules (`crate::tidy`, `crate::upgrade`, `crate::lint`, `crate::init`) or infrastructure modules (`crate::infra`).

### Requirement: Duplicate function detection across commands
Private function names duplicated across command modules (`src/tidy/`, `src/upgrade/`, `src/lint/`, `src/init/`) SHALL be detected by `code_health` tests. Public functions with same name across modules are allowed.

### Requirement: File size budget
Maximum 500 lines per `.rs` source file (excluding test modules).

### Requirement: Folder file count budget
Maximum 8 `.rs` files per directory in `src/`.

### Requirement: Import path hygiene
- **Rule 1**: No `super::super::` anywhere (use `crate::` instead)
- **Rule 2**: No `use crate::<parent>::` when `use super::` suffices (depth-aware parent prefix detection for inline mod blocks and tests.rs)
- **Rule 3**: No `use self::` (always redundant)

### Requirement: Mise lint:size task
The system SHALL provide a `mise` task `lint:size` that checks file size budgets. The `clippy` task SHALL depend on `lint:size`.
