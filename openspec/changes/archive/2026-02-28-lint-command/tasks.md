## 1. Lint config parsing

- [x] 1.1 Add `LintConfig` and `RuleConfig` types to `crates/gx-lib/src/config.rs` with `Level` enum (`Error`, `Warn`, `Off`) and `IgnoreTarget` struct (optional `action`, `workflow`, `job` fields)
- [x] 1.2 Extend `ManifestData` TOML wire types in `crates/gx-lib/src/infrastructure/manifest.rs` to include an optional `[lint]` section with `[lint.rules]` entries
- [x] 1.3 Parse `[lint.rules]` into `LintConfig` during `Config::load`, defaulting to all rules at their hardcoded default levels when absent
- [x] 1.4 Add tests: parse config with `[lint.rules]`, parse config without `[lint]` section, parse config with ignore targets, reject unknown level values

## 2. Lint framework

- [x] 2.1 Create `crates/gx-lib/src/commands/lint/mod.rs` with `LintRule` trait, `LintContext`, `Diagnostic`, and `Level` types
- [x] 2.2 Implement the lint orchestrator: build context from manifest/lock/scanned workflows, run enabled rules, filter diagnostics against ignore targets, format output, determine exit code
- [x] 2.3 Add ignore matching logic: a diagnostic matches an ignore entry when all specified keys match (intersection semantics)
- [x] 2.4 Add tests: orchestrator runs enabled rules, skips `off` rules, filters ignored diagnostics, returns correct exit code for errors/warnings/clean

## 3. Lint rules

- [x] 3.1 Implement `sha-mismatch` rule: for each located action with a SHA ref, compare against lock entry for that action+version
- [x] 3.2 Implement `unpinned` rule: for each located action, check if the reference is a tag (not a SHA)
- [x] 3.3 Implement `unsynced-manifest` rule: compare action IDs in workflows vs manifest, report both directions of mismatch
- [x] 3.4 Implement `stale-comment` rule: for each located action with a SHA+comment, check if the comment version matches the lock entry
- [x] 3.5 Add unit tests for each rule in isolation using mock/test context data

## 4. CLI integration

- [x] 4.1 Add `Lint` variant to `Commands` enum in `crates/gx/src/main.rs`
- [x] 4.2 Add `lint` function to `crates/gx-lib/src/commands/app.rs` that builds context and delegates to the lint orchestrator
- [x] 4.3 Wire `Commands::Lint` dispatch in `main.rs` to call `commands::app::lint`
- [x] 4.4 Handle exit code: `gx lint` returns process exit code 1 when errors are present

## 5. Integration tests

- [x] 5.1 Add `crates/gx-lib/tests/lint_test.rs` with end-to-end tests using temp repos: clean repo passes, SHA mismatch detected, unpinned action detected, unsynced manifest detected, stale comment detected
- [x] 5.2 Add integration test: rule disabled via config produces no diagnostics
- [x] 5.3 Add integration test: ignore target suppresses matching diagnostic
