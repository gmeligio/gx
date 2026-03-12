### Requirement: Lint configuration in Cargo.toml
The `[lints.clippy]` section in `Cargo.toml` SHALL include individually selected restriction lints in addition to the existing group-level denials (`pedantic`, `perf`, `nursery`). The blanket `restriction = "deny"` SHALL NOT be used. The section SHALL also deny `module_name_repetitions` and `pub_use` restriction lints.

#### Scenario: Cargo.toml has individual restriction lints
- **WHEN** a developer inspects `[lints.clippy]` in `Cargo.toml`
- **THEN** they SHALL see individual restriction lint entries (e.g., `unwrap_used = "deny"`)
- **AND** they SHALL NOT see `restriction = "deny"`

#### Scenario: Clippy CI check passes
- **WHEN** `mise run clippy` is executed in CI
- **THEN** the check SHALL pass with all configured lints enforced

#### Scenario: Type with module name prefix triggers lint
- **WHEN** a developer defines `pub struct TidyPlan` in the `tidy` module
- **THEN** `cargo clippy` SHALL report an error for `clippy::module_name_repetitions`

#### Scenario: pub use re-export triggers lint
- **WHEN** a developer adds `pub use submodule::SomeType` in a `mod.rs`
- **THEN** `cargo clippy` SHALL report an error for `clippy::pub_use`

### Requirement: Layer dependency direction enforcement
The system SHALL include a code health test that verifies domain modules (`src/domain/**/*.rs`) do not import from command modules (`crate::tidy`, `crate::upgrade`, `crate::lint`, `crate::init`) or infrastructure modules (`crate::infra`).

#### Scenario: Domain file imports from infra
- **WHEN** a file in `src/domain/` contains `use crate::infra`
- **THEN** the `code_health` test suite SHALL fail with a message identifying the violating file and import

#### Scenario: Domain file imports from command module
- **WHEN** a file in `src/domain/` contains `use crate::tidy` (or upgrade, lint, init)
- **THEN** the `code_health` test suite SHALL fail with a message identifying the violating file and import

#### Scenario: Command module imports from domain (allowed)
- **WHEN** a file in `src/tidy/` contains `use crate::domain`
- **THEN** the `code_health` test suite SHALL pass (this dependency direction is allowed)

### Requirement: Duplicate function detection across commands
The system SHALL include a code health test that detects private (non-`pub`) function names duplicated across command modules (`src/tidy/`, `src/upgrade/`, `src/lint/`, `src/init/`).

#### Scenario: Same function name in tidy and upgrade
- **WHEN** `src/tidy/mod.rs` defines `fn diff_manifests(` and `src/upgrade/mod.rs` also defines `fn diff_manifests(`
- **THEN** the `code_health` test suite SHALL fail listing the duplicated function name and the files it appears in

#### Scenario: Same function name in same module (allowed)
- **WHEN** `src/tidy/mod.rs` and `src/tidy/diff.rs` both define a private function with the same name
- **THEN** the `code_health` test suite SHALL pass (duplication within a module is not flagged)

#### Scenario: Public functions with same name (allowed)
- **WHEN** two command modules define `pub fn plan(` (public API with same name)
- **THEN** the `code_health` test suite SHALL pass (each command's public `plan()` is intentionally independent)

### Requirement: File size budget
The system SHALL enforce a maximum line count per `.rs` source file. The budget SHALL be configurable in the test but default to 500 lines (excluding test modules).

#### Scenario: File exceeds budget
- **WHEN** a `.rs` file in `src/` exceeds the line budget
- **THEN** the `code_health` test suite SHALL fail listing the file and its line count

#### Scenario: File within budget
- **WHEN** all `.rs` files in `src/` are at or below the line budget
- **THEN** the `code_health` test suite SHALL pass

### Requirement: Folder file count budget
The system SHALL enforce a maximum number of `.rs` files per directory in `src/`. The budget SHALL default to 8 files.

#### Scenario: Folder exceeds file count budget
- **WHEN** a directory in `src/` contains more than 8 `.rs` files
- **THEN** the `code_health` test suite SHALL fail listing the directory and file count

#### Scenario: Folder within budget
- **WHEN** all directories in `src/` contain 8 or fewer `.rs` files
- **THEN** the `code_health` test suite SHALL pass

### Requirement: Import path hygiene
The system SHALL include a code health test that verifies all `.rs` files in `src/` follow three import path rules.

**Rule 1**: No `super::super::` anywhere — use `crate::` instead.
**Rule 2**: No `use crate::<parent>::` when `use super::` suffices — i.e., when the target module is the direct parent of the current module scope.
**Rule 3**: No `use self::` — it is always redundant.

Rule 2 SHALL use **depth-aware parent prefix detection**:
- For file-level `use` statements (indent 0): the parent prefix is derived from the file's parent module in the filesystem.
- For indented `use` statements (indent 4+, inside inline `mod` blocks): the parent prefix is the file's own module path (one level more specific than file-level).
- For `tests.rs` files: the parent prefix SHALL be derived from the actual includer (the file containing the `mod tests;` declaration), not from the filesystem path.

#### Scenario: File uses super::super:: in an import
- **WHEN** any `.rs` file in `src/` contains `super::super::` (in a `use` statement or path expression)
- **THEN** the `code_health` test suite SHALL fail identifying the file, line number, and offending line

#### Scenario: File-level use of crate:: when super:: suffices
- **WHEN** a file at `src/domain/plan.rs` (parent = `domain`) contains `use crate::domain::Specifier` at indent 0
- **THEN** the `code_health` test suite SHALL fail (correct form: `use super::Specifier`)

#### Scenario: File at depth 2+ correctly uses crate:: (allowed)
- **WHEN** a file at `src/domain/lock/entry.rs` (parent = `domain::lock`) contains `use crate::domain::CommitSha`
- **THEN** the `code_health` test suite SHALL pass (target is not under `domain::lock`, so `super::` would need two hops)

#### Scenario: Inline test module imports from file's parent module (allowed)
- **WHEN** file `src/infra/lock/convert.rs` contains `use crate::infra::lock::{FileLock}` inside an inline `mod tests {}` block (indented)
- **THEN** the `code_health` test suite SHALL pass (from `convert::tests`, `crate::infra::lock::` is the grandparent — `super::` would need two hops)

#### Scenario: Inline test module imports from file's own module (violation)
- **WHEN** file `src/infra/lock/convert.rs` contains `use crate::infra::lock::convert::SomeItem` inside an inline `mod tests {}` block (indented)
- **THEN** the `code_health` test suite SHALL fail (correct form: `use super::SomeItem`)

#### Scenario: tests.rs included by non-mod.rs file (correct parent)
- **WHEN** `src/infra/github/tests.rs` is included via `mod tests;` in `resolve.rs` and contains `use crate::infra::github::responses::{GitObject}`
- **THEN** the `code_health` test suite SHALL pass (actual parent is `infra::github::resolve`, not `infra::github` — `crate::infra::github::` is not replaceable by `super::`)

#### Scenario: tests.rs included by mod.rs file (standard case)
- **WHEN** `src/infra/lock/tests.rs` is included via `mod tests;` in `mod.rs` and contains `use crate::infra::lock::FileLock` at indent 0
- **THEN** the `code_health` test suite SHALL fail (parent is `infra::lock`, so `super::FileLock` suffices)

#### Scenario: File uses use self:: prefix
- **WHEN** any `.rs` file in `src/` contains `use self::foo` or `pub use self::foo`
- **THEN** the `code_health` test suite SHALL fail (correct form: `use foo` or `pub use foo`)

### Requirement: Mise lint:size task
The system SHALL provide a `mise` task `lint:size` that checks file size budgets. The `clippy` task SHALL depend on `lint:size` so that `mise run clippy` runs both.

#### Scenario: Running mise run clippy triggers size check
- **WHEN** a developer runs `mise run clippy`
- **THEN** the `lint:size` task SHALL execute before or alongside clippy
- **AND** if any file exceeds the size budget, the task SHALL fail with a non-zero exit code
