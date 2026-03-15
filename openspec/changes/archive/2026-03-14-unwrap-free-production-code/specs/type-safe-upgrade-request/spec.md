<!-- MODIFIED: Replaces openspec/specs/type-safe-upgrade-request/spec.md -->
<!-- Change: Pinned moves from Mode to Scope. Mode::Pinned(ActionId) becomes Scope::Pinned(ActionId, Version). -->

### Requirement: UpgradeRequest construction is infallible
`Request::new()` SHALL accept `Mode` and `Scope` and return `Self` directly (not `Result`). No invalid mode/scope combination SHALL exist in the type system.

#### Scenario: Constructing a Safe+All request
- **WHEN** `Request::new(Mode::Safe, Scope::All)` is called
- **THEN** it SHALL return a `Request` directly (no `Result` wrapping)

#### Scenario: Constructing a Pinned request
- **WHEN** a pinned upgrade is requested for `actions/checkout` at version `v5`
- **THEN** it SHALL be constructed as `Request::new(Mode::Safe, Scope::Pinned(id, version))`
- **AND** `Scope::Pinned` SHALL carry both `ActionId` and `Version`
- **AND** no runtime validation SHALL be needed

#### Scenario: Pinned+All is unrepresentable
- **WHEN** a developer attempts to construct a pinned upgrade targeting all actions
- **THEN** the type system SHALL prevent this at compile time (the `Pinned` variant is a `Scope` that requires an `ActionId` and `Version`, not a `Mode`)

### Requirement: Mode and Scope enum definitions
`Mode` SHALL have exactly two variants: `Safe` and `Latest`.
`Scope` SHALL have exactly three variants: `All`, `Single(ActionId)`, and `Pinned(ActionId, Version)`.

`Scope::Pinned` carries both `ActionId` and `Version` because it represents "upgrade this specific action to this exact version" — the version is part of the targeting, not the strategy. `Scope::Single` carries only `ActionId` because the version is determined by the `Mode` (safe = within major, latest = absolute latest).

### Requirement: No expect/unwrap in Result-returning functions
The `resolve_upgrade_mode()` function SHALL NOT use `.expect()` or `.unwrap()` on any value. All construction SHALL be infallible.

#### Scenario: resolve_upgrade_mode builds requests without panicking
- **WHEN** `resolve_upgrade_mode(None, false)` is called
- **THEN** it SHALL return `Ok(Request { mode: Safe, scope: All })` without any `.expect()` call in the code path

### Requirement: PinnedRequiresSingleScope error variant removed
The `Error` enum SHALL NOT contain a `PinnedRequiresSingleScope` variant. This error condition is prevented by the type system.
