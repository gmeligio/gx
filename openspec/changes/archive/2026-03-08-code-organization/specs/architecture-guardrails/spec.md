## ADDED Requirements

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

### Requirement: Mise lint:size task
The system SHALL provide a `mise` task `lint:size` that checks file size budgets. The `clippy` task SHALL depend on `lint:size` so that `mise run clippy` runs both.

#### Scenario: Running mise run clippy triggers size check
- **WHEN** a developer runs `mise run clippy`
- **THEN** the `lint:size` task SHALL execute before or alongside clippy
- **AND** if any file exceeds the size budget, the task SHALL fail with a non-zero exit code
