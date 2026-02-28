## 1. Proper error type

- [x] 1.1 Add `LintError` enum to `crates/gx-lib/src/commands/lint/mod.rs` with `Workflow(WorkflowError)` and `ViolationsFound { errors, warnings }` variants
- [x] 1.2 Add `Lint(LintError)` variant to `AppError` in `crates/gx-lib/src/commands/app.rs`
- [x] 1.3 Update `lint::run()` return type from `(Vec<Diagnostic>, bool)` to `Result<Vec<Diagnostic>, LintError>` where `ViolationsFound` is returned when errors exist
- [x] 1.4 Update `app::lint()` to use `LintError` instead of `ManifestError::Validation` hack
- [x] 1.5 Handle `ViolationsFound` in `main.rs` to exit with code 1 without printing error wrapper

## 2. Stale-comment rule implementation

- [x] 2.1 Implement `stale_comment::check()` using `LocatedAction.sha` and lock reverse lookup via `Lock::get(&LockKey)`
- [x] 2.2 Add unit test: comment matches lock SHA produces no diagnostic
- [x] 2.3 Add unit test: comment does not match lock SHA produces warn diagnostic
- [x] 2.4 Add unit test: action without comment (sha is None) is skipped

## 3. Integration test expansion

- [x] 3.1 Add test: sha-mismatch rule detects workflow SHA not in lock
- [x] 3.2 Add test: stale-comment rule detects mismatched version comment
- [x] 3.3 Add test: mixed severity output (errors + warnings), exit code is 1
- [x] 3.4 Add test: warning-only output (all error rules disabled), exit code is 0
- [x] 3.5 Add test: workflow with only local actions (./path) produces no diagnostics
- [x] 3.6 Add test: rule severity override (promote warn to error via config)
- [x] 3.7 Add test: ignore scoped to specific workflow suppresses only that workflow's diagnostics
