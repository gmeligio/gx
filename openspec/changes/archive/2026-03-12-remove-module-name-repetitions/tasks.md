## 1. Lint Configuration

- [x] 1.1 Add `module_name_repetitions = "deny"` and `pub_use = "deny"` to `[lints.clippy]` in `Cargo.toml`

## 2. Rename Types

- [x] 2.1 Rename error types: `ConfigError` → `Error`, `InitError` → `Error`, `TidyError` → `Error`, `UpgradeError` → `Error`, `LintError` → `Error`, `GithubError` → `Error`, `LockFileError` → `Error`, `ManifestError` → `Error`, `RepoError` → `Error`, `ResolutionError` → `Error`, `WorkflowError` → `Error`, `ResolveError` → `Error`
- [x] 2.2 Rename plan/report types: `TidyPlan` → `Plan`, `UpgradePlan` → `Plan`, `TidyReport` → `Report`, `UpgradeReport` → `Report`, `LintReport` → `Report`, `InitReport` → `Report`
- [x] 2.3 Rename domain types: `ResolvedAction` → `Resolved`, `ActionSpec` → `Spec`, `UpgradeAction` → `Action`, `UpgradeCandidate` → `Candidate`, `SyncEvent` → `Event`, `LockEntry` → `Entry`
- [x] 2.4 Rename workflow types: `WorkflowScanner` → `Scanner`, `WorkflowUpdater` → `Updater`, `LocatedAction` → `Located`, `WorkflowActionSet` → `ActionSet`, `WorkflowLocation` → `Location`
- [x] 2.5 Rename infra types: `GithubRegistry` → `Registry`, `FileWorkflowScanner` → `FileScanner`, `FileWorkflowUpdater` → `FileUpdater`
- [x] 2.6 Rename upgrade types: `UpgradeMode` → `Mode`, `UpgradeScope` → `Scope`, `UpgradeRequest` → `Request`
- [x] 2.7 Rename output types: `OutputLine` → `Line`
- [x] 2.8 Rename lint types: `LintContext` → `Context`, `LintRule` → `Rule`

## 3. Remove pub use Re-exports

- [x] 3.1 Remove all `pub use` lines from `src/domain/mod.rs`
- [x] 3.2 Remove all `pub use` lines from `src/domain/action/mod.rs`
- [x] 3.3 Remove all `pub use` lines from `src/domain/lock/mod.rs` and `src/domain/manifest/mod.rs`
- [x] 3.4 Remove all `pub use` lines from `src/infra/mod.rs`
- [x] 3.5 Remove all `pub use` lines from `src/output/mod.rs`
- [x] 3.6 Remove all `pub use` lines from `src/upgrade/mod.rs` and `src/infra/manifest/mod.rs`
- [x] 3.7 Remove `pub use` from `src/domain/action/identity.rs`

## 4. Update Import Paths

- [x] 4.1 Update imports in `src/main.rs` and `src/command.rs` to use qualified module paths
- [x] 4.2 Update imports in `src/tidy/` modules
- [x] 4.3 Update imports in `src/upgrade/` modules
- [x] 4.4 Update imports in `src/lint/` modules
- [x] 4.5 Update imports in `src/init/` modules
- [x] 4.6 Update imports in `src/infra/` modules
- [x] 4.7 Update imports in `src/domain/` modules (internal cross-references)
- [x] 4.8 Update imports in `src/config.rs` and `src/output/` modules
- [x] 4.9 Update imports in all `tests/**/*.rs` files

## 5. Verification

- [x] 5.1 Run `mise run clippy` and confirm zero errors
- [x] 5.2 Run `rtk cargo test` and confirm all tests pass
- [x] 5.3 Verify no `pub use` statements remain in `src/`
- [x] 5.4 Verify no type names trigger `module_name_repetitions`
