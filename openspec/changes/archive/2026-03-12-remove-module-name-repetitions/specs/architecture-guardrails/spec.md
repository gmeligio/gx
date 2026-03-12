## MODIFIED Requirements

### Requirement: Lint configuration in Cargo.toml
The `[lints.clippy]` section SHALL deny `module_name_repetitions` and `pub_use` restriction lints.

#### Scenario: Type with module name prefix triggers lint
- **WHEN** a developer defines `pub struct TidyPlan` in the `tidy` module
- **THEN** `cargo clippy` SHALL report an error for `clippy::module_name_repetitions`

#### Scenario: pub use re-export triggers lint
- **WHEN** a developer adds `pub use submodule::SomeType` in a `mod.rs`
- **THEN** `cargo clippy` SHALL report an error for `clippy::pub_use`
