### Requirement: WorkflowPatch carries domain types [NEW]

`WorkflowPatch.pins` SHALL be `Vec<ResolvedAction>` instead of `Vec<(ActionId, String)>`. The domain plan struct SHALL NOT carry pre-formatted serialization strings.

#### Scenario: WorkflowPatch uses ResolvedAction
- **GIVEN** a tidy plan produces workflow patches
- **WHEN** `WorkflowPatch` is constructed
- **THEN** `pins` contains `ResolvedAction` values with `id`, `sha`, and `version`
- **AND** no `"SHA # version"` string formatting has occurred yet

### Requirement: Updater trait deleted [NEW]

The `Updater` trait in `domain/workflow.rs` SHALL be deleted. It has exactly one implementation (`FileUpdater`), is never mocked, and its `HashMap<ActionId, String>` signature forces callers to pre-format serialization strings. Callers SHALL use `WorkflowWriter` (renamed from `FileUpdater`) directly from the infra layer.

### Requirement: FileUpdater renamed to WorkflowWriter [NEW]

`FileUpdater` in `infra/workflow_update.rs` SHALL be renamed to `WorkflowWriter`. It SHALL accept `&[WorkflowPatch]` instead of `HashMap<ActionId, String>`.

### Requirement: Single serialization point for YAML comment syntax [NEW]

The `"SHA # version"` YAML comment formatting SHALL appear in exactly one function: `format_uses_ref()` in `infra/workflow_update.rs`. No other module SHALL construct this format string.

#### Scenario: format_uses_ref with version annotation
- **GIVEN** a `ResolvedAction` with `sha = "abc123..."` and `version = Some("v4.2.1")`
- **WHEN** `format_uses_ref()` is called
- **THEN** the output is `"abc123... # v4.2.1"`

#### Scenario: format_uses_ref with bare SHA (no annotation)
- **GIVEN** a `ResolvedAction` with `sha = "abc123..."` and `version = None`
- **WHEN** `format_uses_ref()` is called
- **THEN** the output is `"abc123..."` (no `#` comment)

### Requirement: Lock::build_update_map deleted [NEW]

`Lock::build_update_map()` SHALL be deleted. It is dead code in production (never called from any command path). It also leaks serialization by producing `"SHA # version"` formatted strings.

**Note**: This method is used in `tests/integ_upgrade.rs` — that test must be updated to use the new workflow output path instead.
