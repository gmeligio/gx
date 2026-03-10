## Architecture

### Current layering (broken)

```
main.rs  →  GxError (wraps AppError)
  ├── init/   ─┐
  ├── lint/    │  command layer — each impl Command, returns AppError
  ├── tidy/    │
  └── upgrade/ ┘
        │
        ▼
   domain/
   ├── command.rs   ← Command trait (hardcoded to AppError)
   ├── error.rs     ← AppError (imports from infra, lint, tidy, upgrade) ← VIOLATION
   ├── action/
   ├── lock/
   ├── manifest/
   ├── plan.rs
   ├── resolution.rs
   ├── workflow.rs
   └── workflow_actions.rs
        │
        ▼
   infra/
```

### Target layering (fixed)

```
main.rs  →  GxError (wraps InitRunError, TidyRunError, UpgradeRunError, LintError)
  │
  ├── command.rs      ← NEW: Command<Error=_>, CommandReport traits
  │
  ├── init/   ─┐
  ├── lint/    │  each impl Command with its own Error type
  ├── tidy/    │
  └── upgrade/ ┘
        │
        ▼
   domain/              ← PURE: no error.rs, no command.rs, no upward imports
   ├── action/
   ├── lock/
   ├── manifest/
   ├── plan.rs
   ├── resolution.rs    ← gains #[cfg(test)] pub(crate) mod testutil
   ├── workflow.rs
   └── workflow_actions.rs
        │
        ▼
   infra/
```

---

## Per-command error types

### Error mapping for each command's `run()`

```
                  GithubError  ManifestError  LockFileError  WorkflowError  TidyError  UpgradeError  Custom
Init::run()           ✓             ✓              ✓             ✓            ✓                      AlreadyInitialized
Tidy::run()           ✓             ✓              ✓             ✓            ✓
Upgrade::run()        ✓             ✓              ✓             ✓                          ✓
Lint::run()                                                                                         (just LintError)
```

### Design

Each command that does I/O defines a run-level error:

```rust
// src/init/mod.rs
#[derive(Debug, Error)]
pub enum InitError {
    #[error("already initialized — use `gx tidy` to update")]
    AlreadyInitialized,
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Lock(#[from] LockFileError),
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
    #[error(transparent)]
    Tidy(#[from] TidyError),
}

// src/tidy/mod.rs — TidyError already exists but only covers plan().
// Rename existing TidyError → TidyPlanError, create TidyRunError for run().
// Or: expand TidyError with infra variants (simpler).
```

### Decision: expand existing errors vs new "run" errors

**Option A — Expand existing errors** with infra variants:
- `TidyError` gains `Github`, `Manifest`, `Lock` variants
- Downside: `plan()` returns `TidyError` but can never produce the infra variants

**Option B — Separate run errors**:
- Keep `TidyError` pure (domain only, returned by `plan()`)
- Create `TidyRunError` that wraps `TidyError` + infra errors
- Clearer separation but more types

**Chosen: Option B.** The domain errors (`TidyError`, `UpgradeError`, `LintError`) stay
pure. Run-level errors are new types in each command module. This maintains the semantic
distinction between "plan failed" and "I/O failed during execution."

For `Lint`, `LintError` is already sufficient — `Lint::run()` only produces `LintError`,
so no `LintRunError` is needed.

### GxError in main.rs (after)

```rust
#[derive(Debug, Error)]
enum GxError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Init(#[from] InitError),
    #[error(transparent)]
    Tidy(#[from] TidyRunError),
    #[error(transparent)]
    Upgrade(#[from] UpgradeRunError),
    #[error(transparent)]
    Lint(#[from] LintError),
    #[error(transparent)]
    Repo(#[from] RepoError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

### Command trait (after)

```rust
// src/command.rs
pub trait Command {
    type Report: CommandReport;
    type Error: std::error::Error;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Self::Report, Self::Error>;
}
```

---

## Duplicate-fn test fix

### Problem

The test at `tests/code_health.rs:119` matches any line starting with `fn ` (non-pub).
This catches trait impl methods which are intentionally same-named across modules.

### Solution

Track whether we're inside an `impl ... for ...` block using brace depth counting.
When inside a trait impl block, skip the function from duplicate detection.

Detection heuristic:
1. When a line matches `impl ... for ...` (and isn't a comment), set a flag and track brace depth
2. Increment depth on `{`, decrement on `}`
3. While inside a trait impl block (depth > 0), skip `fn` lines
4. Reset when depth returns to 0

This is a lightweight text-based heuristic, not a full parser. It handles the actual
code style in this repo (which uses standard Rust formatting).

---

## Shared test mocks

### Location: `domain/resolution.rs`

The `VersionRegistry` trait is defined in `domain/resolution.rs`. Mock implementations
belong next to the trait they mock. Add a `#[cfg(test)] pub(crate) mod testutil` inside
`domain/resolution.rs`.

### Current inline mocks → replacement

| Mock | File | Behavior | Action |
|------|------|----------|--------|
| `NoopRegistry` | manifest_sync.rs | Always `AuthRequired` | → `testutil::AuthRequiredRegistry` |
| `MetadataOnlyRegistry` | lock_sync.rs | lookup OK, tags fail | → `FakeRegistry::new().fail_tags()` |
| `TaggedShaRegistry` | lock_sync.rs | Fixed tags on all methods | → `FakeRegistry` with configured tags |
| `SimpleShaRegistry` | lock_sync.rs | Fixed SHA, tags fail | → `FakeRegistry::new().with_fixed_sha(...).fail_tags()` |
| `TagUpgradeRegistry` | manifest_sync.rs | Version-as-SHA, configurable tags | → `FakeRegistry` with configured tags |
| `MockPlanRegistry` | upgrade/plan.rs | Deterministic SHA, configurable tags | → `FakeRegistry` |
| `MixedRegistry` | lock_sync.rs | Conditional by action id | Keep inline — specialized |
| `MockRegistry` | resolution.rs | Pre-built Results | Keep inline — tests error paths |

### FakeRegistry API

```rust
#[cfg(test)]
pub(crate) mod testutil {
    pub struct FakeRegistry { ... }

    impl FakeRegistry {
        pub fn new() -> Self;
        pub fn with_all_tags(self, id: &str, tags: Vec<&str>) -> Self;
        pub fn with_sha_tags(self, id: &str, sha: &str, tags: Vec<&str>) -> Self;
        pub fn with_fixed_sha(self, sha: &str) -> Self;    // always return this SHA
        pub fn fail_tags(self) -> Self;                      // tags_for_sha/all_tags → AuthRequired
    }

    pub struct AuthRequiredRegistry;
}
```

This replaces 6 of 8 inline mocks. The remaining 2 (`MixedRegistry`, `MockRegistry`)
stay inline — they have genuinely specialized behavior.
