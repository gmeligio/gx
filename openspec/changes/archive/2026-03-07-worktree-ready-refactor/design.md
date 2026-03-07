## Context

The codebase currently funnels all command orchestration through two files:

```
main.rs (258 lines)          app.rs (280 lines)
├── CLI struct + enum        ├── tidy()        — orchestrates tidy
├── Commands enum            ├── init()        — orchestrates init
├── match dispatch           ├── upgrade()     — orchestrates upgrade
├── make_cb()                ├── lint()        — orchestrates lint
├── finish_spinner()         └── AppError      — all error variants
├── append_log_path()
└── resolve_upgrade_mode()
```

Any feature touching a command must edit both files. With parallel worktree agents, this means conflicts.

## Goals / Non-Goals

**Goals:**
- Each command module owns its full orchestration (plan + run)
- Adding a new command only requires: new file + 1 line in `commands/mod.rs` + 1 variant in `Commands` enum + 1 match arm in `main.rs`
- Modifying an existing command touches only that command's file
- `app.rs` and `main.rs` become thin, stable files that rarely change

**Non-Goals:**
- Cargo workspace split (too heavy for this codebase size)
- Auto-discovery of modules (Rust doesn't support this natively)
- Changing any user-facing behavior
- Restructuring `domain/` or `infrastructure/` modules

## Decisions

### 1. Each command module gets a `run()` function

**Decision:** Move orchestration from `app.rs` into each command module as a public `run()` function.

```
commands/tidy.rs      → pub fn run(repo_root, config, on_progress) -> Result<TidyReport, AppError>
commands/init.rs      → pub fn run(repo_root, config, on_progress) -> Result<InitReport, AppError>
commands/upgrade.rs   → pub fn run(repo_root, config, request, on_progress) -> Result<UpgradeReport, AppError>
commands/lint/mod.rs  → pub fn run_command(repo_root, config) -> Result<LintReport, AppError>
```

The function signatures stay identical to what `app.rs` currently exposes. This is a pure move, not a redesign.

**Why `run_command` for lint?** `lint/mod.rs` already has a `pub fn run()` that runs the lint checks and returns diagnostics. The orchestration wrapper (which handles formatting) needs a different name to avoid collision.

### 2. `app.rs` becomes a shared types module

**Decision:** `app.rs` retains only `AppError` and any shared helper types. No command-specific logic.

```rust
// app.rs after refactor (~50 lines)
pub enum AppError {
    AlreadyInitialized,
    Manifest(ManifestError),
    Lock(LockFileError),
    Workflow(WorkflowError),
    Github(GithubError),
    Tidy(TidyError),
    Upgrade(UpgradeError),
    Lint(LintError),
}
```

**Alternative considered:** Delete `app.rs` entirely and let each command define its own error. Rejected because `AppError` serves as the unified error type that `main.rs` matches on, and several commands share the same error variants (e.g., `ManifestError`, `LockFileError`).

### 3. CLI helpers move to `commands/common.rs`

**Decision:** `make_cb`, `finish_spinner`, `append_log_path` move from `main.rs` to a new `commands/common.rs` module. These are shared by all commands.

```rust
// commands/common.rs
pub fn make_cb<'a>(...) -> impl FnMut(&str) + 'a { ... }
pub fn finish_spinner(spinner: Option<ProgressBar>) { ... }
pub fn append_log_path(log_file: Option<&LogFile>, lines: &mut Vec<OutputLine>) { ... }
```

**Alternative considered:** Keep them in `main.rs`. Rejected because moving them out makes `main.rs` shorter and these helpers are logically "command infrastructure," not CLI parsing.

### 4. `resolve_upgrade_mode` moves to `commands/upgrade.rs`

**Decision:** This function parses upgrade-specific CLI arguments. It belongs in the upgrade module, not in `main.rs`.

### 5. `main.rs` becomes a thin dispatcher

**Decision:** After refactoring, `main.rs` contains only:
- `Cli` struct and `Commands` enum (clap definitions)
- `GxError` enum
- `fn main()` — parse, setup shared state, match command → `<command>::run()`, render, print

```rust
// main.rs after refactor (~80 lines)
fn main() -> Result<(), GxError> {
    let cli = Cli::parse();
    let printer = Printer::new();
    // ... shared setup (log_file, repo_root, config) ...

    match cli.command {
        Commands::Tidy => {
            let spinner = printer.spinner("Running tidy...");
            let mut lf = log_file.take();
            let report = tidy::run(&repo_root, config, common::make_cb(...))?;
            common::finish_spinner(spinner);
            let mut lines = render_tidy(&report);
            common::append_log_path(lf.as_ref(), &mut lines);
            printer.print_lines(&lines);
            log_file = lf;
        }
        // ... similar for other commands
    }
}
```

### 6. Accept append-only conflicts in hub files

**Decision:** `mod.rs` files and the `Commands` enum are append-only registration points. These create trivial conflicts (two agents both add a line at the end) that git can usually auto-resolve with `merge -s ort`. No structural changes needed — just accept these as the cost of Rust's module system.

## File ownership after refactor

```
Agent working on TIDY only touches:
  src/commands/tidy.rs
  tests/tidy_test.rs

Agent working on UPGRADE only touches:
  src/commands/upgrade.rs
  tests/upgrade_test.rs

Agent working on LINT only touches:
  src/commands/lint/*.rs
  tests/lint_test.rs

Agent adding NEW COMMAND touches:
  src/commands/new_cmd.rs       (new file — no conflict)
  src/commands/mod.rs           (append 1 line — trivial conflict)
  src/main.rs                   (add enum variant + match arm — trivial conflict)
```

## Risks / Trade-offs

**[Circular dependency risk]** Command modules will import from `app.rs` (for `AppError`) while `app.rs` used to import from them. After refactor, the dependency is one-way: command modules depend on `app.rs`, not vice versa. Clean.

**[`init` reuses `tidy::plan()`]** The new `init.rs` will call `super::tidy::plan()`. This creates a dependency between command modules, but it's read-only (init calls tidy's plan function, doesn't modify tidy's code). Acceptable.

**[Test file conflicts remain]** Integration test files (e.g., `tidy_test.rs` at 1005 lines) are still monoliths. Two agents adding tests to the same command will conflict. This is lower priority — test conflicts are easy to resolve and could be addressed in a follow-up by splitting into `tests/tidy/*.rs` directories.
