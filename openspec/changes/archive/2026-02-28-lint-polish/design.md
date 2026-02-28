## Context

The `gx lint` command was implemented in `lint-command`. Three areas need polish before production use.

**Error handling**: The lint function returns `AppError::Manifest(ManifestError::Validation("lint check failed with errors"))` when violations are found. This conflates policy violations (lint findings) with I/O errors (file parsing failures). The CLI output is `Error: App(Manifest(Validation("...")))` which is ugly and confusing.

**Stale-comment rule**: Currently a stub returning `Vec::new()`. The data is already available — `LocatedAction` has `sha: Option<CommitSha>` (set when a comment like `# v4` is present) and `version: Version` (the comment text). The lock file can be reverse-queried via `Lock::entries()` to find what version maps to a given SHA.

**Test gaps**: 5 integration tests vs tidy's 17. No sha-mismatch rule coverage. No tests for diagnostics with mixed severity levels. No edge case coverage.

## Goals / Non-Goals

**Goals:**
- Clean CLI output for lint violations (no error type wrapper noise)
- Proper exit code handling: code 1 for violations, code 2 for I/O errors
- Working stale-comment rule using existing scanner data
- Integration test coverage matching tidy test depth

**Non-Goals:**
- Changing the scanner or `LocatedAction` struct (data is already sufficient)
- Adding new rules beyond the four from v1
- Changing lint config format

## Decisions

### Decision 1: Dedicated `LintError` type with `ViolationsFound` variant

Follow the `UpgradeError` pattern from `crates/gx-lib/src/commands/upgrade.rs`:

```rust
#[derive(Debug, Error)]
pub enum LintError {
    #[error(transparent)]
    Workflow(#[from] WorkflowError),

    #[error("{errors} error(s) and {warnings} warning(s) found")]
    ViolationsFound {
        errors: usize,
        warnings: usize,
    },
}
```

Add `LintError` to `AppError`:
```rust
#[error(transparent)]
Lint(#[from] LintError),
```

**Why**: Policy violations are not errors in the traditional sense — they're expected output. `ViolationsFound` signals "lint ran successfully and found problems" vs "lint crashed." In `main.rs`, match on this variant to exit with code 1 and print the summary without the `Error: App(...)` wrapper.

### Decision 2: Handle `ViolationsFound` in main.rs without error wrapper

```rust
match cli.command {
    Commands::Lint => {
        if let Err(GxError::App(AppError::Lint(LintError::ViolationsFound { .. }))) = commands::app::lint(&repo_root, config) {
            std::process::exit(1);
        }
    }
    // ...
}
```

**Why**: The diagnostic output is already printed by the lint function. The only thing the error boundary needs to do is set the exit code. No error message should be printed for `ViolationsFound` — the diagnostics ARE the output.

**Alternatives considered**:
- *Return `(Vec<Diagnostic>, bool)` from app::lint and handle in main*: Leaks domain types into the binary crate. The error type is cleaner.
- *Use `std::process::exit(1)` directly in app::lint*: Violates separation of concerns. The library shouldn't decide exit behavior.

### Decision 3: Stale-comment rule via lock reverse lookup

The data pipeline:
```
Workflow file:  actions/checkout@abc123 # v4
                                ^^^^^^   ^^
                                 SHA    comment

LocatedAction:  version = "v4", sha = Some("abc123")

Lock file:      (actions/checkout, v4) -> "abc123"  ← correct
                (actions/checkout, v4) -> "def456"  ← stale comment!
```

The rule iterates `LocatedAction` entries where `sha.is_some()` (meaning they have a version comment). For each, it looks up the lock entry for `(action_id, version)` and compares the lock's SHA against `LocatedAction.sha`. If they differ, the comment is stale.

```rust
fn check(&self, ctx: &LintContext) -> Vec<Diagnostic> {
    for located in ctx.workflows {
        let sha = match &located.sha {
            Some(s) => s,
            None => continue, // No comment to validate
        };
        let key = LockKey::new(located.id.clone(), located.version.clone());
        if let Some(lock_sha) = ctx.lock.get(&key) {
            if lock_sha != sha {
                // Comment says "v4" but the SHA doesn't match what lock says v4 resolves to
                // → stale comment
            }
        }
    }
}
```

**Why**: No scanner changes needed. `LocatedAction` already carries the SHA when a comment is present. The lock already supports `get(&LockKey) -> Option<&CommitSha>`. This is a pure read operation.

## Risks / Trade-offs

- **Exit code 2 for I/O errors**: Currently all errors exit with code 1. Changing lint I/O errors to code 2 is more correct but differs from other commands. For now, only `ViolationsFound` gets special handling; other errors use the existing error display path.

- **Stale-comment false positives**: If the lock is stale itself (e.g., user ran `gx init` but not `gx tidy`), the stale-comment rule will report mismatches that are really lock staleness. This is acceptable — `gx tidy` is the fix for both cases.

## Open Questions

<!-- none -->
