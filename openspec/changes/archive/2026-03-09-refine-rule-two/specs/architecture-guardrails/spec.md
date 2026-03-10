## MODIFIED Requirements

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
