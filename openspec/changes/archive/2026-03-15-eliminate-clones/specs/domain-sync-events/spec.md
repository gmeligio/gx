<!-- MODIFIED: Modifies openspec/specs/domain-sync-events/spec.md -->
<!-- Change: LockKey type deleted; lock_keys() returns Vec<Spec> instead of Vec<LockKey>. -->

### Requirement: Manifest lock key computation uses Spec type

`Manifest` SHALL provide a `lock_keys(&self) -> Vec<Spec>` method that computes all lock keys needed (global + override versions). `Spec` serves as the lock lookup key (see domain-composition spec for `LockKey` deletion and `Spec` derive requirements).

#### Scenario: Manifest with overrides
- **WHEN** a manifest has action `A@^4` globally and an override `A@^3` for a specific workflow
- **THEN** `lock_keys()` SHALL return `Spec` values for both `A@^4` and `A@^3`
