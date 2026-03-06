## Context

All gx user-facing output currently goes through the `log` crate (`info!`, `warn!`, `debug!`) with `env_logger` formatting `[INFO] message`. This creates several problems:

1. **Untestable output** — No unit tests verify what the user sees. Existing tests only check file artifacts.
2. **Noisy default output** — `[INFO]` prefix on every line, duplicate information (upgrades listed twice), unnecessary status messages.
3. **No progress feedback** — GitHub API calls can take ~10s with no indication of activity.
4. **Logic/presentation tangled** — 30+ `info!`/`warn!` calls scattered across `upgrade.rs`, `tidy.rs`, `lint/mod.rs`, `app.rs`, `manifest.rs`, `lock.rs`.

The goal is minimal, emoji-based output by default with full detail always captured in a log file, and automatic verbose inline output in CI environments.

## Goals / Non-Goals

**Goals:**
- Minimal, clean default output using emoji symbols (`↑`, `✓`, `✗`, `⚠`)
- Spinner progress during slow operations (GitHub API calls)
- Always-write log file for local debugging
- CI auto-detection (`$CI` env var) with verbose inline output
- Fully testable rendering — unit tests assert on structured output
- Commands return data, rendering is separate

**Non-Goals:**
- `--json` output mode (not needed now)
- `--verbose` flag (CI auto-detects, log file covers local debugging)
- Custom color themes or configurable symbols
- Streaming/incremental output during command execution

## Decisions

### 1. Drop `log` + `env_logger`, use `indicatif` + `console`

**Decision:** Remove `log` and `env_logger` crates entirely. Add `indicatif` (spinners) and `console` (colors, TTY detection).

**Why not keep `log`?** With the log file capturing all detail and CI auto-verbose mode, `RUST_LOG` provides no unique value. The `log` crate is primarily a library concern — well-known Rust CLIs (ripgrep, fd, cargo) own their output systems. `reqwest` internally uses `log`, but its messages are safely dropped without a subscriber.

**Alternatives considered:**
- Keep `log` for `RUST_LOG` backdoor → Adds maintenance burden for a feature no one uses. Log file is strictly better for debugging.
- Use `tracing` instead of `log` → Over-engineered for a synchronous CLI. `tracing` shines in async/service contexts.

### 2. Commands return report structs (no I/O)

**Decision:** Each command returns a typed report struct. No command calls `info!()`, `println!()`, or any output function.

```
upgrade::plan()  → UpgradePlan (already exists, no output changes needed)
upgrade::apply_upgrade_workflows() → UpgradeReport (new, replaces info! calls)
tidy::plan()     → TidyPlan (already exists, no output changes needed)
tidy::apply_*()  → TidyReport (new, replaces info! calls in sync/apply functions)
lint::run()      → Vec<Diagnostic> (already exists, no changes needed)
lint::format_and_report() → LintReport (new struct, replaces info! calls)
```

The plan functions (`upgrade::plan`, `tidy::plan`) already return data structs and are clean — they just have scattered `info!` calls that need removal. The apply/format functions need refactoring to return report data instead of printing.

**Why separate report types per command?** Each command has different data. A shared `enum Report` would add indirection without benefit — `main.rs` always knows which command it ran.

### 3. Structured `OutputLine` enum for rendering

**Decision:** Renderers produce `Vec<OutputLine>` — a semantic representation of output. Colors are applied only when printing to terminal.

```rust
enum OutputLine {
    Upgraded { action: String, from: String, to: String },
    Added { action: String, version: String },
    Removed { action: String },
    Changed { action: String, detail: String },
    Skipped { action: String, reason: String },
    Warning { message: String },
    LintDiag { level: Level, workflow: Option<String>, rule: String, message: String },
    Summary { text: String },
    LogPath { path: PathBuf },
    CiNotice { message: String },
}
```

**Why not plain strings?** Structured output enables:
- Test assertions on semantics (`assert!(lines.contains(OutputLine::Upgraded { .. }))`) instead of brittle string matching
- Color application at boundary without ANSI codes in test output
- Future `--json` mode becomes trivial (serialize variants)

### 4. Callback `Fn(&str)` for spinner progress

**Decision:** Commands accept an `on_progress: impl Fn(&str)` callback to signal phase changes. The orchestrator (app.rs / main.rs) maps this to a spinner in production and to a no-op/capture in tests.

```rust
// Production
let spinner = printer.spinner();
let report = tidy::apply(&plan, &updater, |msg| spinner.set_message(msg))?;
spinner.finish_and_clear();

// Test
let report = tidy::apply(&plan, &updater, |_| {})?;
```

**Why not pass `&Spinner` directly?** Commands would depend on `indicatif`, making them untestable without the terminal crate. The callback keeps commands pure — they don't know spinners exist.

### 5. Log file at `{tmp}/gx/{command}/{RFC-date}.log`

**Decision:** Every local run writes a detailed log file. CI runs skip the log file (output is inline verbose).

- **Location:** OS temp directory (`std::env::temp_dir()`) under `gx/{command}/` (e.g., `/tmp/gx/upgrade/2026-03-06T12-03-01.log`)
- **Content:** Timestamped entries with full detail — resolution steps, API calls, skipped actions, warnings, workflow changes
- **Shown to user:** Log file path printed as last line of output
- **Rotation:** Not implemented initially. OS temp cleanup handles it.

**Log file writer:** A simple struct that wraps `BufWriter<File>`, accepting `&str` messages with automatic timestamps. The same callback used for spinner progress also writes to the log file.

### 6. CI auto-detection

**Decision:** Check `std::env::var("CI")`. If set (any value), run in verbose mode with inline detail. Print a notice: `ℹ CI detected, running in verbose mode`.

In CI mode:
- No spinner (CI terminals don't support ephemeral lines)
- All detail printed inline (equivalent to log file content but to stdout)
- No log file written (stdout IS the log in CI)

### 7. Output rendering architecture

```
main.rs
├── parse CLI args
├── detect CI mode (std::env::var("CI"))
├── create Printer { is_ci, is_tty, use_color }
├── create LogFile (if not CI)
├── match command:
│   ├── Upgrade:
│   │   ├── spinner = printer.spinner("Checking actions...")
│   │   ├── plan = upgrade::plan(manifest, lock, registry, request, |msg| {
│   │   │       spinner.set_message(msg);
│   │   │       log_file.write(msg);
│   │   │   })?
│   │   ├── spinner.finish_and_clear()
│   │   ├── if plan.is_empty(): printer.print(UpgradeReport::up_to_date())
│   │   ├── else: apply plan, build UpgradeReport
│   │   ├── lines = render_upgrade(&report)
│   │   └── printer.print_lines(&lines)
│   ├── Tidy: (similar pattern)
│   ├── Lint:
│   │   ├── diagnostics = lint::run(...)
│   │   ├── report = LintReport::from(diagnostics)
│   │   ├── lines = render_lint(&report)
│   │   └── printer.print_lines(&lines)
│   └── Init: (similar to Tidy)
├── printer.print_log_path(&log_file)  // if not CI
└── exit
```

### 8. Printer struct (not a trait)

**Decision:** Use a concrete `Printer` struct, not a `trait Printer`. The struct holds configuration (color, TTY, CI mode) and methods for printing `OutputLine` values with appropriate formatting.

```rust
struct Printer {
    use_color: bool,
    is_ci: bool,
}

impl Printer {
    fn new() -> Self { /* detect TTY, CI, NO_COLOR */ }
    fn spinner(&self, message: &str) -> Option<ProgressBar> { /* None if CI or !tty */ }
    fn print_lines(&self, lines: &[OutputLine]) { /* apply color, print to stdout */ }
}
```

**Why not a trait?** Traits are useful for mocking in tests, but our architecture doesn't need it — tests assert on `render_*()` output (which returns `Vec<OutputLine>`), not on `Printer` behavior. The Printer is a thin final-mile layer.

### 9. Default output examples

**`gx upgrade --latest`** (changes found):
```
 ↑ actions/checkout            v6 → v6.0.2
 ↑ jdx/mise-action             v3 → v3.6.2
 ↑ actions-rust-lang/rustfmt   v1 → v1.1.2

 ✓ 3 upgraded · 2 workflows updated
 📋 /tmp/gx/upgrade/2026-03-06T12-03-01.log
```

**`gx upgrade`** (nothing to do):
```
 ✓ All actions up to date
```

**`gx tidy`**:
```
 − actions/unused-thing         (removed)
 + actions/new-dep              v2@abc1234
 ↑ actions/checkout             sha → v6.0.2

 ✓ 1 removed · 1 added · 1 upgraded · 2 workflows updated
 📋 /tmp/gx/tidy/2026-03-06T12-05-22.log
```

**`gx lint`** (violations):
```
 ✗ ci.yml: unpinned: actions/checkout@main is not pinned
 ⚠ ci.yml: stale-comment: version comment does not match lock

 1 error · 1 warning
```

**`gx lint`** (clean):
```
 ✓ No lint issues found
```

**`gx init`**:
```
 + actions/checkout             v6@abc1234
 + actions/setup-node           v4@def5678

 ✓ 2 actions discovered · manifest created
 📋 /tmp/gx/init/2026-03-06T12-10-00.log
```

**CI mode** (`gx upgrade --latest`):
```
 ℹ CI detected, running in verbose mode
 [12:03:01] Resolving actions/checkout@v6...
 [12:03:02] GitHub API: tags for actions/checkout → v6.0.2
 [12:03:02] Pinning to abc1234
 ↑ actions/checkout            v6 → v6.0.2
 [12:03:03] Writing .github/workflows/ci.yml
 ✓ 1 upgraded · 1 workflow updated
```

## Risks / Trade-offs

**[Losing RUST_LOG for reqwest debugging]** → The log file captures our own detail but not reqwest's internal HTTP debug logs. Mitigation: If HTTP debugging is ever needed, add a `GX_DEBUG=1` env var that initializes a minimal log subscriber. This is a future concern, not a launch blocker.

**[indicatif + stdout interaction]** → Spinners use stderr by default in indicatif, which can interleave with stdout output. Mitigation: Use `indicatif::ProgressBar::with_draw_target(ProgressDrawTarget::stderr())` and print final output to stdout after spinner is cleared. Clear separation of ephemeral (stderr) vs persistent (stdout) output.

**[Windows terminal emoji support]** → Unicode symbols (`↑`, `✓`, `✗`, `⚠`, `📋`) need Windows Terminal or compatible. Mitigation: Windows Terminal (default since Windows 11) supports all these. Legacy `cmd.exe` with raster fonts may not, but that's a shrinking edge case. `console` crate handles TTY detection.

**[Breaking change for scripts parsing gx output]** → Any script parsing `[INFO] + actions/checkout v6 -> v6.0.2` will break. Mitigation: This is intentional. The old format was never documented as stable. The new format is more parseable (structured, consistent symbols).

**[Log file disk usage]** → Log files accumulate in temp dir. Mitigation: OS temp cleanup handles this. Files are small (< 100KB typically). No rotation needed initially.
