## MODIFIED Requirements

### Requirement: Lint configuration in Cargo.toml
The `[lints.clippy]` section in `Cargo.toml` SHALL include individually selected restriction lints in addition to the existing group-level denials (`pedantic`, `perf`, `nursery`). The blanket `restriction = "deny"` SHALL NOT be used.

#### Scenario: Cargo.toml has individual restriction lints
- **WHEN** a developer inspects `[lints.clippy]` in `Cargo.toml`
- **THEN** they SHALL see individual restriction lint entries (e.g., `unwrap_used = "deny"`)
- **AND** they SHALL NOT see `restriction = "deny"`

#### Scenario: Clippy CI check passes
- **WHEN** `mise run clippy` is executed in CI
- **THEN** the check SHALL pass with all configured lints enforced
