## Report Structs

### Requirement: Commands return report structs
Each command's apply/format function SHALL return a typed report struct instead of printing output directly:
- `UpgradeReport`, `TidyReport`, `LintReport`, `InitReport`

No command function SHALL call `println!`, `eprintln!`, `info!`, `warn!`, or any output macro.

### Requirement: Structured OutputLine enum
The rendering layer SHALL produce `Vec<OutputLine>` from report structs. Variants: `Upgraded`, `Added`, `Removed`, `Changed`, `Skipped`, `Warning`, `LintDiag`, `Summary`, `LogPath`, `CiNotice`.

---

## Formatting

### Requirement: Colors applied at print boundary
The `Printer` struct SHALL apply ANSI colors only when writing `OutputLine` values to the terminal. Render functions SHALL NOT embed ANSI escape codes in `OutputLine` data.

Color enabled when: output is a TTY AND `NO_COLOR` not set.

Color mapping: `+`/green, `-`/red, `^`/cyan (upgrade), `!`/yellow (warn), summary/green, CI notice/blue.

### Requirement: Default output formatting
- No `[INFO]`/`[WARN]` prefixes
- Emoji symbols as line prefixes: `^` upgrade, `+` add, `-` remove, `~` change, checkmark success, X error, warning
- Version transitions use Unicode arrow, not `->`
- Summary line uses middle dot as separator between counts
- One blank line before summary

---

## Spinner

### Requirement: Spinner during long operations
The system SHALL display an ephemeral spinner on stderr during network calls via `Printer::spinner()` returning `Option<ProgressBar>` (indicatif). Spinner is cleared before final output, not shown in CI or when stderr is not a TTY.

### Requirement: Progress callback pattern
Commands accept `on_progress: impl Fn(&str)` for phase transitions. The orchestrator connects the callback to the spinner's `set_message()` and the log file's `write()`. Commands SHALL NOT depend on `indicatif` or any terminal crate.

---

## Logging and CI

### Requirement: Log file always written for local runs
Every local (non-CI) invocation writes a detailed log to `{tmp}/gx/{command}/{RFC-3339-date}.log` with timestamped entries. The log path is printed as the last output line.

### Requirement: CI auto-detection with verbose inline output
When `CI` env var is set:
1. Print `CiNotice` as first line
2. Print all progress inline with timestamps
3. Skip spinner (`None`)
4. Skip log file creation
