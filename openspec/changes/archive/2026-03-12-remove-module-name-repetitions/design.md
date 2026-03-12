## Context

The codebase uses `pub use` re-exports in facade modules (e.g., `domain/mod.rs` re-exports ~20 types from submodules). This allows flat imports like `use crate::domain::ActionId` but requires globally unique type names, leading to prefixed names like `TidyPlan`, `LintError`, `GithubRegistry`.

By removing `pub use` facades and using qualified imports, types can use shorter names and be accessed via their module path.

## Goals / Non-Goals

**Goals:**
- Rename types to drop module name prefix/suffix where it improves clarity
- Remove `pub use` re-exports from facade modules
- Update all imports to use qualified module paths
- Deny `clippy::module_name_repetitions` and `clippy::pub_use`

**Non-Goals:**
- Changing module structure or moving files
- Renaming types where the module prefix is part of the domain name (e.g., `ActionId` stays — `Id` alone is too vague)
- Renaming types that don't trigger the lint

## Decisions

### Decision 1: Rename strategy

**Choice:** Rename types that repeat their module name. Keep types where the "repeated" prefix is actually part of the domain concept.

Rename map (module → old name → new name):

| Module | Old Name | New Name | Rationale |
|--------|----------|----------|-----------|
| `config` | `ConfigError` | `Error` | Standard Rust convention |
| `domain::action::resolved` | `ResolvedAction` | `Resolved` | Module provides context |
| `domain::action::spec` | `ActionSpec` | `Spec` | `action::Spec` reads well |
| `domain::action::upgrade` | `UpgradeAction` | `Action` | `upgrade::Action` |
| `domain::action::upgrade` | `UpgradeCandidate` | `Candidate` | `upgrade::Candidate` |
| `domain::event` | `SyncEvent` | `Event` | Module provides context |
| `domain::lock` | `LockEntry` | `Entry` | `lock::Entry` |
| `domain::resolution` | `ResolutionError` | `Error` | Standard convention |
| `domain::workflow` | `WorkflowError` | `Error` | Standard convention |
| `domain::workflow` | `WorkflowScanner` | `Scanner` | `workflow::Scanner` |
| `domain::workflow` | `WorkflowUpdater` | `Updater` | `workflow::Updater` |
| `domain::workflow_actions` | `LocatedAction` | `Located` | Module provides context |
| `domain::workflow_actions` | `WorkflowActionSet` | `ActionSet` | `workflow_actions::ActionSet` |
| `domain::workflow_actions` | `WorkflowLocation` | `Location` | `workflow_actions::Location` |
| `infra::github` | `GithubError` | `Error` | Standard convention |
| `infra::github` | `GithubRegistry` | `Registry` | `github::Registry` |
| `infra::lock` | `LockFileError` | `Error` | Standard convention |
| `infra::manifest` | `ManifestError` | `Error` | Standard convention |
| `infra::repo` | `RepoError` | `Error` | Standard convention |
| `infra::workflow_scan` | `FileWorkflowScanner` | `FileScanner` | Drop `Workflow` prefix |
| `infra::workflow_update` | `FileWorkflowUpdater` | `FileUpdater` | Drop `Workflow` prefix |
| `init` | `InitError` | `Error` | Standard convention |
| `init::report` | `InitReport` | `Report` | `init::Report` |
| `lint` | `LintError` | `Error` | Standard convention |
| `lint` | `LintContext` | `Context` | `lint::Context` |
| `lint` | `LintRule` | `Rule` | `lint::Rule` |
| `lint::report` | `LintReport` | `Report` | `lint::Report` |
| `output` | `OutputLine` | `Line` | `output::Line` |
| `tidy` | `TidyPlan` | `Plan` | `tidy::Plan` |
| `tidy` | `TidyError` | `Error` | Standard convention |
| `tidy::report` | `TidyReport` | `Report` | `tidy::Report` |
| `upgrade` | `UpgradePlan` | `Plan` | `upgrade::Plan` |
| `upgrade` | `UpgradeError` | `Error` | Standard convention |
| `upgrade` | `UpgradeMode` | `Mode` | `upgrade::Mode` |
| `upgrade` | `UpgradeScope` | `Scope` | `upgrade::Scope` |
| `upgrade` | `UpgradeRequest` | `Request` | `upgrade::Request` |
| `upgrade::report` | `UpgradeReport` | `Report` | `upgrade::Report` |
| `upgrade::cli` | `ResolveError` | `Error` | `cli::Error` |

Types that keep their current name (no lint violation or prefix is domain-meaningful):
- `ActionId` — `Id` is too generic
- `ActionOverride` — `Override` is a keyword-adjacent name
- `CommitSha` — `Sha` alone loses domain context
- `LockKey` — lives in `spec` module, not `lock`
- `LockDiff`, `ManifestDiff`, `WorkflowPatch` — live in `plan` module

### Decision 2: Import style after removing pub use

**Choice:** Use qualified module imports everywhere.

```rust
// Before:
use crate::domain::{ActionId, TidyPlan, LintError, WorkflowScanner};

// After:
use crate::domain::action::ActionId;
use crate::tidy;
use crate::lint;
use crate::domain::workflow;
// Then use: tidy::Plan, lint::Error, workflow::Scanner
```

**Rationale:** Qualified paths make it obvious where types come from. No ambiguity between `tidy::Error` and `lint::Error`.

### Decision 3: Error type convention

**Choice:** All error types are renamed to `Error` within their module.

**Rationale:** This follows `std::io::Error`, `serde_json::Error`, `reqwest::Error`. The module path disambiguates.

## Risks / Trade-offs

- **Risk: Large diff touching every file** → Mitigated by purely mechanical nature (rename + import update). No logic changes.
- **Risk: Merge conflicts with concurrent branches** → Should be done on a clean branch with no other pending changes.
- **Trade-off: Longer import paths** → Accepted. `use crate::domain::workflow::Scanner` is longer than `use crate::domain::WorkflowScanner`, but the type name at usage site is shorter and the module provides context.
- **Risk: Some short names may feel too generic** → Decision 1 keeps names where the prefix adds real meaning (e.g., `ActionId`, `CommitSha`).
