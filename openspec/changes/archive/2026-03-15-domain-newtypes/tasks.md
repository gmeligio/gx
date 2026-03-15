## 1. RuleName enum

- [x] 1.1 Define `RuleName` enum in `src/lint/mod.rs` with `ShaMismatch`, `Unpinned`, `StaleComment`, `UnsyncedManifest` variants. Derive `Serialize`/`Deserialize` with `#[serde(rename_all = "kebab-case")]`. Implement `Display` (kebab-case) and `FromStr`.
- [x] 1.2 Change `Diagnostic::rule` from `String` to `RuleName`. Update `Diagnostic::new` signature and all call sites in rule implementations.
- [x] 1.3 Change `Lint::rules` from `BTreeMap<String, Rule>` to `BTreeMap<RuleName, Rule>`. Update `get_rule` signature from `get_rule(&self, name: &str, default_level: Level)` to `get_rule(&self, name: RuleName, default_level: Level)`. The fallback construction (`Rule { level: default_level, ignore: vec![] }`) is unchanged — it already works regardless of key type. Also update `LintData.rules` in `src/infra/manifest/convert.rs` from `BTreeMap<String, Rule>` to `BTreeMap<RuleName, Rule>` — this is where serde deserializes `[lint.rules]` and rejects unknown keys.
- [x] 1.4 Update `Rule` trait: `fn name(&self) -> RuleName` instead of `fn name(&self) -> &str`. Update each rule implementation's `name()` return value to the corresponding `RuleName` variant.
- [x] 1.5 Update lint report, output lines, and integration tests to use `RuleName` variants instead of string literals.

## 2. Repository newtype

### Define type
- [x] 2.1 Define `Repository` newtype in `src/domain/action/identity.rs` with private inner field, `as_str()`, `Display`, `From<String>`, `From<&str>`, standard derives.

### Update core structs
- [x] 2.2 Change `ActionId::base_repo()` to return `Repository` instead of `String`.
- [x] 2.3 Update `Commit::repository`, `ResolvedRef::repository`, and `ShaDescription::repository` fields to `Repository`.

### Update consumers
- [x] 2.4 Update infra lock format and migration structs to convert between `String` and `Repository` at the serialization boundary.
- [x] 2.5 Update GitHub registry and resolution service.
- [x] 2.6 Update test utilities and assertions.

## 3. WorkflowPath and JobId newtypes

### Define types
- [x] 3.1 Define `WorkflowPath` in `src/domain/workflow_actions.rs` with private inner field, `WorkflowPath::new()` that normalizes backslashes, `as_str()`, `Display`, standard derives. No `From<String>` — use named constructor only.
- [x] 3.2 Define `JobId` in `src/domain/workflow_actions.rs` with private inner field, `as_str()`, `Display`, `From<String>`, `From<&str>`, standard derives.

### Update core structs
- [x] 3.3 Update `Location` fields: `workflow: WorkflowPath`, `job: Option<JobId>`.
- [x] 3.4 Update `ActionOverride` fields: `workflow: WorkflowPath`, `job: Option<JobId>`.
- [x] 3.5 Update `Diagnostic::workflow` to `Option<WorkflowPath>` and `Diagnostic::with_workflow` signature.

### Update consumers
- [x] 3.6 Update workflow scanner, manifest convert, override sync/resolve/prune, and lint matching code.
- [x] 3.7 Update lint ignore matching: `matches_ignore_action` currently compares `diag_workflow.ends_with(target_workflow.as_str())` with bare strings. Update so the diagnostic side uses `WorkflowPath::as_str()` and the ignore target side stays `&str` (from `IgnoreTarget.workflow: Option<String>`). The `ends_with` comparison becomes `diag_workflow.as_str().ends_with(target_str)`. `IgnoreTarget` keeps `String` fields — see design decision 4.
- [x] 3.8 Update tests: integration lint tests, override tests, scanner tests.

## 4. VersionComment newtype

### Define type
- [x] 4.1 Define `VersionComment` in `src/domain/action/identity.rs` with private inner field, `as_str()`, `Display`, `From<String>`, `From<&str>`, standard derives. Placed in `identity` (not `lock::resolution`) to avoid a reverse dependency from `domain::action` → `domain::lock`.

### Update core structs
- [x] 4.2 Update `Resolution::comment` and `Specifier::Range::comment` fields to `VersionComment`.
- [x] 4.3 Update `Specifier::to_comment()`: the inner `comment` field becomes `VersionComment`, but the method keeps returning `&str` (via `comment.as_str()`) to avoid breaking call sites. The `const fn` qualifier must be removed — `as_str()` on a newtype with a private `String` field cannot be `const` in stable Rust.

### Update consumers
- [x] 4.4 Update lock operations (`set`, `set_comment`, `is_complete`, `build_update_map`), upgrade plan, tidy patches, and lock format serialization boundary.

## 5. CommitDate newtype

### Define type
- [x] 5.1 Define `CommitDate` in `src/domain/action/identity.rs` with private inner field, `as_str()`, `Display`, `From<String>`, `From<&str>`, standard derives.

### Update core structs
- [x] 5.2 Update `Commit::date`, `ResolvedRef::date`, and `ShaDescription::date` fields to `CommitDate`.

### Update consumers
- [x] 5.3 Update infra lock format and migration structs to convert between `String` and `CommitDate` at the serialization boundary.
- [x] 5.4 Update GitHub registry, resolution service.
- [x] 5.5 Update test utilities and assertions.

## 6. GitHubToken newtype

### Define type
- [x] 6.1 Define `GitHubToken` in `src/config.rs` (currently ~314 lines, well within 500-line budget) with private inner field, `as_str()`, `From<String>`, manual `Debug` impl that masks the value. No `Display`.

### Update consumers
- [x] 6.2 Update `Settings::github_token` to `Option<GitHubToken>`.
- [x] 6.3 Update `Registry` token field and `authenticated_get` to use `GitHubToken`.
- [x] 6.4 Update construction sites: `Settings::from_env()`, `Registry::new()`, and test code.
