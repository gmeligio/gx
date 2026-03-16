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

### Requirement: Domain newtypes for semantic string fields

The following newtypes SHALL wrap bare `String` fields to provide type-safe domain identifiers. All use private inner fields with `as_str()` accessors, `Display`, and standard derives (`Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`). None perform validation at construction time (except `WorkflowPath` normalization) — they are type-level markers, not smart constructors.

#### WorkflowPath

`WorkflowPath` SHALL represent a workflow file path with forward-slash normalization. Defined in `domain::workflow_actions`.

##### Scenario: WorkflowPath normalizes Windows-style path
- **GIVEN** a path string `.github\workflows\ci.yml`
- **WHEN** `WorkflowPath::new` is called
- **THEN** `as_str()` SHALL return `.github/workflows/ci.yml`

##### Scenario: WorkflowPath preserves already-normalized path
- **GIVEN** a path string `.github/workflows/ci.yml`
- **WHEN** `WorkflowPath::new` is called
- **THEN** `as_str()` SHALL return `.github/workflows/ci.yml`

##### Scenario: WorkflowPath normalizes mixed slashes
- **GIVEN** a path string `.github/workflows\ci.yml`
- **WHEN** `WorkflowPath::new` is called
- **THEN** `as_str()` SHALL return `.github/workflows/ci.yml`

##### Scenario: WorkflowPath has no From<String> impl
- **GIVEN** the `WorkflowPath` type
- **THEN** construction SHALL only be via `WorkflowPath::new()` (no `From<String>` or `From<&str>`)
- **BECAUSE** the named constructor makes normalization explicit at every call site

##### Scenario: WorkflowPath accepts empty or degenerate input
- **GIVEN** an empty string `""`
- **WHEN** `WorkflowPath::new` is called
- **THEN** `as_str()` SHALL return `""` (no validation, normalization is a no-op on empty input)

#### JobId

`JobId` SHALL represent a workflow job identifier. Defined in `domain::workflow_actions`. Standard construction via `From<String>` and `From<&str>`. No validation — wraps the string as-is.

#### Repository

`Repository` SHALL represent an `owner/repo` identifier. Defined in `domain::action::identity`. Standard construction via `From<String>` and `From<&str>`. No validation at construction time — the string is wrapped as-is. If validation (e.g., must contain exactly one `/`) is needed later, the private field ensures the constructor is the single entry point.

#### CommitDate

`CommitDate` SHALL represent an ISO 8601 date string from commit metadata. Defined in `domain::action::identity`. Standard construction via `From<String>` and `From<&str>`. No validation — wraps the string as-is.

#### GitHubToken

`GitHubToken` SHALL represent a GitHub API token with masked debug output. Defined in `config`. Construction via `From<String>` only. No `Display` impl.

##### Scenario: GitHubToken masks Debug output
- **GIVEN** a `GitHubToken` wrapping the string `"ghp_abc123secret"`
- **WHEN** formatted with `Debug` (e.g., via `{:?}`)
- **THEN** the output SHALL be `GitHubToken(***)` — the token value SHALL NOT appear

##### Scenario: GitHubToken has no Display impl
- **GIVEN** the `GitHubToken` type
- **THEN** it SHALL NOT implement `Display`
- **BECAUSE** tokens should never be formatted for user-facing output; the only consumption path is `as_str()` for the Authorization header

##### Scenario: GitHubToken Clone is acceptable
- **GIVEN** a `GitHubToken` value
- **THEN** it SHALL derive `Clone`
- **BECAUSE** the token already exists as a plain string in environment variables and process memory — `Clone` does not increase the attack surface

### Requirement: StepIndex newtype for workflow step positions

A `StepIndex(u16)` newtype SHALL represent 0-based step positions within workflow jobs. All struct fields currently using `Option<usize>` for step indices SHALL use `Option<StepIndex>` instead.

#### Scenario: StepIndex to i64 conversion is infallible
- **WHEN** a `StepIndex` is converted to `i64` for TOML serialization
- **THEN** the conversion SHALL use `From<StepIndex> for i64` (infallible)
- **AND** no `expect()` or `unwrap()` SHALL appear in the conversion path

#### Scenario: StepIndex from TOML i64
- **WHEN** a step value is read from a TOML file as `i64`
- **THEN** it SHALL be converted to `StepIndex` via `TryFrom<i64>`
- **AND** values outside `0..=u16::MAX` SHALL produce an error

#### Scenario: Negative step value in TOML
- **WHEN** a TOML file contains a negative step value (e.g., `-1`)
- **THEN** `TryFrom<i64>` SHALL return an error
- **AND** the error SHALL propagate as `ManifestError::Validation` with a message describing the invalid step index

#### Scenario: Step value exceeds u16 range
- **WHEN** a TOML file contains a step value exceeding 65535
- **THEN** `TryFrom<i64>` SHALL return an error
- **AND** the error SHALL propagate as `ManifestError::Validation` with a message describing the invalid step index

### Requirement: Structs using step index

The following fields SHALL use `Option<StepIndex>`:
- `workflow_actions::Location.step`
- `manifest::overrides::ActionOverride.step`
- `infra::manifest::convert::ManifestEntryRaw.step`

### Requirement: No expect/unwrap in step serialization

The `i64::try_from(step).expect("step index overflow")` pattern in `infra/manifest/patch.rs` and `infra/manifest/convert.rs` SHALL be replaced with infallible `From<StepIndex> for i64`.

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

---

## Nursery Lints

### Requirement: Nursery lints denied individually

The `[lints.clippy]` section in `Cargo.toml` SHALL deny the following nursery-group lints individually (they are NOT covered by `pedantic`, `perf`, or any blanket group denial):

- `redundant_clone`

Each nursery lint SHALL be listed as its own key-value pair (e.g., `redundant_clone = "deny"`), separate from the restriction lint list.

#### Scenario: redundant_clone catches unnecessary clone
- **GIVEN** code calls `.clone()` on a value that is not used after the clone
- **WHEN** `cargo clippy` runs
- **THEN** the build fails with `clippy::redundant_clone` error

#### Scenario: redundant_clone does not fire on necessary clones
- **GIVEN** code calls `.clone()` on a value that IS used after the clone
- **WHEN** `cargo clippy` runs
- **THEN** no `clippy::redundant_clone` error is produced

#### Scenario: False positive handled with expect
- **GIVEN** a nursery lint produces a false positive on correct code
- **WHEN** the developer suppresses it
- **THEN** `#[expect(clippy::redundant_clone, reason = "...")]` SHALL be used (not `#[allow]`)
