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
