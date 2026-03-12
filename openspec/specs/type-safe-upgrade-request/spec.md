### Requirement: UpgradeRequest construction is infallible
`UpgradeRequest::new()` SHALL accept `UpgradeMode` and `UpgradeScope` and return `Self` directly (not `Result`). The invalid combination of `Pinned` mode with `All` scope SHALL be unrepresentable in the type system.

#### Scenario: Constructing a Safe+All request
- **WHEN** `UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All)` is called
- **THEN** it SHALL return an `UpgradeRequest` directly (no `Result` wrapping)

#### Scenario: Constructing a Pinned request
- **WHEN** a pinned upgrade is requested for `actions/checkout` at version `v5`
- **THEN** the `ActionId` SHALL be embedded in `UpgradeMode::Pinned`
- **AND** no runtime validation SHALL be needed

#### Scenario: Pinned+All is unrepresentable
- **WHEN** a developer attempts to construct a `Pinned` upgrade targeting all actions
- **THEN** the type system SHALL prevent this at compile time (the `Pinned` variant requires an `ActionId`)

### Requirement: No expect/unwrap in Result-returning functions
The `resolve_upgrade_mode()` function SHALL NOT use `.expect()` or `.unwrap()` on any value. All construction SHALL be infallible for known-valid combinations.

#### Scenario: resolve_upgrade_mode builds requests without panicking
- **WHEN** `resolve_upgrade_mode(None, false)` is called
- **THEN** it SHALL return `Ok(UpgradeRequest { mode: Safe, scope: All })` without any `.expect()` call in the code path

### Requirement: PinnedRequiresSingleScope error variant removed
The `UpgradeError` enum SHALL NOT contain a `PinnedRequiresSingleScope` variant. This error condition SHALL be prevented by the type system instead of runtime validation.

#### Scenario: UpgradeError has no scope validation variant
- **WHEN** a developer inspects `UpgradeError`
- **THEN** no variant for scope validation errors SHALL exist
