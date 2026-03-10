## ADDED Requirements

### Requirement: SyncEvent enum for domain operations
The domain layer SHALL define a `SyncEvent` enum that represents observable transitions during manifest and lock synchronization. Domain methods SHALL return `Vec<SyncEvent>` instead of accepting `on_progress: &mut dyn FnMut(&str)` callbacks.

#### Scenario: Domain method produces events instead of calling callback
- **WHEN** `Manifest::sync_with_workflows()` adds a new action to the manifest
- **THEN** it SHALL return a `Vec<SyncEvent>` containing a variant representing the addition (e.g., `SyncEvent::ActionAdded`)
- **AND** it SHALL NOT accept or invoke any callback parameter

#### Scenario: Multiple events from a single operation
- **WHEN** a sync operation adds 2 actions and corrects 1 version
- **THEN** the returned `Vec<SyncEvent>` SHALL contain 3 events in operation order

### Requirement: SyncEvent covers all domain transitions
The `SyncEvent` enum SHALL include variants for: action added, action removed, version corrected (SHA to tag), SHA upgraded to tag, resolution skipped (with reason), and recoverable warning.

#### Scenario: SHA upgraded to tag
- **WHEN** `Manifest::upgrade_sha_versions()` upgrades a SHA version to a tag via the registry
- **THEN** the returned events SHALL include a variant identifying the action ID and the new tag

#### Scenario: Resolution skipped due to recoverable error
- **WHEN** lock resolution encounters a rate limit or auth-required error
- **THEN** the returned events SHALL include a variant with the spec and error reason

### Requirement: SyncEvent implements Display
Each `SyncEvent` variant SHALL implement `Display` to produce a human-readable message suitable for terminal output.

#### Scenario: Formatting an ActionAdded event
- **WHEN** a `SyncEvent::ActionAdded` event is formatted with `Display`
- **THEN** the output SHALL include the action spec (e.g., `"+ actions/checkout@^4"`)

#### Scenario: Command orchestrator formats events for spinner
- **WHEN** the tidy command orchestrator receives events from a domain method
- **THEN** it SHALL iterate the events and call `on_progress(&event.to_string())` for each

### Requirement: Manifest::diff produces ManifestDiff
`Manifest` SHALL provide a `diff(&self, other: &Manifest) -> ManifestDiff` method that computes the difference between two manifest states, including added, removed, updated actions and override changes.

#### Scenario: Action added in other manifest
- **WHEN** `other` contains an action not in `self`
- **THEN** `ManifestDiff::added` SHALL include that action's ID and specifier

#### Scenario: Action version changed
- **WHEN** both manifests contain the same action but with different specifiers
- **THEN** `ManifestDiff::updated` SHALL include the action ID and the new specifier

#### Scenario: Override added
- **WHEN** `other` has an override for an action that `self` does not
- **THEN** `ManifestDiff::overrides_added` SHALL include the override

### Requirement: Lock::diff produces LockDiff
`Lock` SHALL provide a `diff(&self, other: &Lock) -> LockDiff` method that computes the difference between two lock states. Entries with the same key but different SHAs SHALL be treated as replacements (removed + added).

#### Scenario: New lock entry
- **WHEN** `other` contains a key not in `self`
- **THEN** `LockDiff::added` SHALL include that key and entry

#### Scenario: SHA changed for same key
- **WHEN** both locks contain the same key but with different commit SHAs
- **THEN** the key SHALL appear in both `LockDiff::removed` and `LockDiff::added` (replacement)

### Requirement: Manifest override sync as domain method
`Manifest` SHALL provide methods `sync_overrides(&mut self, located: &[LocatedAction], action_set: &WorkflowActionSet)` and `prune_stale_overrides(&mut self, located: &[LocatedAction])` that operate on the manifest in-place without I/O or callbacks.

#### Scenario: Override created for minority version
- **WHEN** an action appears with version `v5` in one workflow and `v6` in two others
- **THEN** `sync_overrides` SHALL add an override for the `v5` location

#### Scenario: Stale override pruned
- **WHEN** an override references a workflow file that no longer exists in the located actions
- **THEN** `prune_stale_overrides` SHALL remove that override

### Requirement: Manifest lock key computation as domain method
`Manifest` SHALL provide a `lock_keys(&self) -> Vec<LockKey>` method that computes all lock keys needed (global + override versions).

#### Scenario: Manifest with overrides
- **WHEN** a manifest has action `A@^4` globally and an override `A@^3` for a specific workflow
- **THEN** `lock_keys()` SHALL return keys for both `A@^4` and `A@^3`
