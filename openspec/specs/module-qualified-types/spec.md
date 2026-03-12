### Requirement: Types do not repeat their module name
All type names (structs, enums, traits, type aliases) SHALL NOT include their containing module name as a prefix or suffix, unless the prefix is part of the domain concept (e.g., `ActionId` where `Id` alone is ambiguous).

#### Scenario: Error type in tidy module
- **WHEN** the `tidy` module defines an error enum
- **THEN** it SHALL be named `Error`, not `TidyError`
- **AND** consumers SHALL access it as `tidy::Error`

#### Scenario: Plan type in upgrade module
- **WHEN** the `upgrade` module defines a plan struct
- **THEN** it SHALL be named `Plan`, not `UpgradePlan`
- **AND** consumers SHALL access it as `upgrade::Plan`

#### Scenario: Domain type with meaningful prefix
- **WHEN** a type like `ActionId` exists in `domain::action::identity`
- **THEN** the name SHALL be kept as `ActionId` because `Id` alone is ambiguous across the codebase

### Requirement: No pub use re-exports in facade modules
Module facade files (`mod.rs` or named module files) SHALL NOT use `pub use` to re-export types from submodules. Consumers SHALL import types from their defining module.

#### Scenario: Importing a domain type
- **WHEN** code needs to use `ActionId` defined in `domain::action::identity`
- **THEN** it SHALL import via `use crate::domain::action::ActionId` (from the direct parent module)
- **AND** `domain/mod.rs` SHALL NOT contain `pub use action::ActionId`

#### Scenario: Importing an error type
- **WHEN** code needs to use the tidy error type
- **THEN** it SHALL import via `use crate::tidy` and reference `tidy::Error`
- **AND** no intermediate module SHALL re-export it

### Requirement: Qualified import convention
When multiple modules define a type with the same short name (e.g., `Error`, `Plan`, `Report`), consumers SHALL use the module as a qualifier rather than renaming with `as`.

#### Scenario: Two error types in scope
- **WHEN** code needs both `tidy::Error` and `lint::Error`
- **THEN** it SHALL import both modules (`use crate::tidy; use crate::lint;`)
- **AND** reference them as `tidy::Error` and `lint::Error`
- **AND** it SHALL NOT use `use crate::tidy::Error as TidyError`
