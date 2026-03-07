## Decision 1: Two traits — `Command` and `CommandReport`

```rust
// src/command/traits.rs

pub trait CommandReport: Debug + Default {
    fn render(&self) -> Vec<OutputLine>;

    fn exit_code(&self) -> i32 {
        0
    }
}

pub trait Command {
    type Report: CommandReport;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Self::Report, AppError>;
}
```

**Why two traits**: The `Command` trait enforces the entry-point contract. The `CommandReport` trait enforces the output contract and absorbs the `render_*` free functions. Keeping them separate avoids coupling report rendering to the command that produced it (reports are defined in `output::report`, not in command modules).

**Why `&self`**: Allows command-specific configuration (e.g., `UpgradeRequest`) to live on the struct. Unit structs (`Tidy`, `Init`, `Lint`) carry no state — `&self` costs nothing.

**Why `&mut dyn FnMut(&str)` over generics**: The codebase already erases the callback to `&mut dyn FnMut(&str)` inside private helpers. Using `dyn` at the trait boundary is consistent with the interior, keeps the trait object-safe, and has no measurable cost for a progress callback called <20 times per run.

**Why `Config` by value**: Three of four commands already consume it. Lint currently borrows, but there's no reason it *needs* to — it can take ownership and borrow internally. Uniform ownership keeps the trait simple.

## Decision 2: Command structs

```rust
// In each command module:

pub struct Tidy;
pub struct Init;
pub struct Lint;
pub struct Upgrade {
    pub request: UpgradeRequest,
}
```

Tidy, Init, Lint are unit structs. Upgrade carries its `UpgradeRequest` (parsed from CLI args before `run()` is called).

Call sites in `main.rs`:

```rust
Tidy.run(&repo_root, config, &mut cb)?;
Init.run(&repo_root, config, &mut cb)?;
Lint.run(&repo_root, config, &mut cb)?;
Upgrade { request }.run(&repo_root, config, &mut cb)?;
```

## Decision 3: `exit_code()` replaces `ViolationsFound`

Currently lint returns `Err(LintError::ViolationsFound { ... })` and `main.rs` catches it to `process::exit(1)`. This conflates "lint found problems" (a successful outcome) with error propagation.

Instead:

```rust
impl CommandReport for LintReport {
    fn exit_code(&self) -> i32 {
        if self.error_count > 0 { 1 } else { 0 }
    }
}
```

`LintError::ViolationsFound` is removed. `format_and_report()` is simplified to always return `Ok(LintReport)`. The `main.rs` lint arm becomes identical to the others.

**Why**: Lint violations are expected output, not errors. The report already carries `error_count` — deriving the exit code from it is natural. This also means any future command that needs a non-zero exit code (e.g., `tidy --check`) just implements `exit_code()`.

## Decision 4: Move `render_*` into `CommandReport` impls

The free functions `render_tidy`, `render_upgrade`, `render_lint`, `render_init` in `output/render.rs` become the `render()` method of `CommandReport` on each report type.

```rust
// In output/report.rs (or a new output/render_impls.rs if report.rs gets too large)
impl CommandReport for TidyReport {
    fn render(&self) -> Vec<OutputLine> { /* body of render_tidy */ }
}
```

The `render.rs` file is removed after migration. Its tests move to `report.rs` (or stay in a `render_tests` submodule).

**Why**: The trait provides compile-time enforcement that every report type can render. No more risk of adding a new report type and forgetting to add a `render_*` function.

## Decision 5: Lint gets `on_progress`

Lint's inner scanning function (`collect_diagnostics`, renamed from `run`) gains an `on_progress` parameter. Progress messages:
- `"Scanning workflows..."` at start
- Per-workflow messages if useful (optional, can add later)

This aligns lint with the other commands and makes the `Command` trait impl natural.

## Decision 6: `main.rs` uniform dispatch

After the refactor, every match arm follows:

```rust
Commands::X => {
    let spinner = printer.spinner("...");
    let mut lf = log_file.take();
    let mut cb = make_cb(spinner.as_ref(), &mut lf, is_ci);
    let report = XCmd.run(&repo_root, config, &mut cb)?;
    finish_spinner(spinner);
    let mut lines = report.render();
    append_log_path(lf.as_ref(), &mut lines);
    printer.print_lines(&lines);
    if report.exit_code() != 0 {
        std::process::exit(report.exit_code());
    }
    log_file = lf;
}
```

The lint special case is gone. The `exit_code()` check is generic — today only lint uses it, but it's ready for `tidy --check` or similar future features.

**Note on the current lint rendering bug**: Today, when lint finds violations, `run_command` returns `Err(ViolationsFound)` and `main.rs` calls `process::exit(1)` *without rendering the report*. The diagnostics are never printed in this path. With the new design, the report is always rendered before checking `exit_code()`, which fixes this.

## Decision 7: Trait location

Traits go in `src/command/traits.rs`, re-exported from `src/command/mod.rs`:

```rust
// src/command/mod.rs
pub mod traits;
pub use traits::{Command, CommandReport};
```

**Why not `output/`**: `CommandReport` could logically live in `output/` since it deals with rendering, but `Command` references `CommandReport` as an associated type bound. Keeping both traits together avoids a circular dependency between `command` and `output` modules. The report *types* stay in `output/report.rs`; the report *trait* lives in `command/traits.rs`.

## Alternatives considered

- **Trait in `output/`**: Would require `command` to depend on `output` for the trait (it already depends on `output` for report types, but adding a trait there creates a tighter coupling).
- **Single `Command` trait without `CommandReport`**: Simpler, but loses compile-time enforcement of rendering. Every new report type would need a manual `render_*` function with no compiler help.
- **Generic `F: FnMut(&str)` on the trait method**: Makes the trait not object-safe. The callback is already erased to `dyn` inside private helpers. No benefit for this use case.
- **`Box<dyn CommandReport>` return from match arms**: Heap allocation + dynamic dispatch for 4 commands. Not worth the small reduction in match-arm boilerplate.
