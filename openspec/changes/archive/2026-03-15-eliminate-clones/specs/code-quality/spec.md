<!-- MODIFIED: Adds nursery lint requirement to openspec/specs/code-quality/spec.md -->
<!-- Change: Add redundant_clone nursery lint as separately configured deny. -->

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
