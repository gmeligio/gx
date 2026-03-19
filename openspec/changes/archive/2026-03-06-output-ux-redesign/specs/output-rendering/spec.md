## ADDED Requirements

### Requirement: Commands return report structs
Each command's apply/format function SHALL return a typed report struct instead of printing output directly. The report structs are:

- `UpgradeReport` — upgrades performed, skipped actions, warnings, workflows updated
- `TidyReport` — actions removed, added, upgraded, corrections applied, workflows updated
- `LintReport` — diagnostics list, error count, warning count
- `InitReport` — actions discovered, manifest/lock created

No command function SHALL call `println!`, `eprintln!`, `info!`, `warn!`, or any output macro.

#### Scenario: Upgrade command returns report
- **WHEN** `upgrade::apply_upgrade_workflows()` completes with 3 upgrades across 2 workflows
- **THEN** it returns an `UpgradeReport` with `upgrades.len() == 3` and `workflows_updated == 2`
- **THEN** no output is written to stdout or stderr

#### Scenario: Tidy command returns report
- **WHEN** `tidy::apply` completes with 1 removal, 2 additions, and 1 SHA upgrade
- **THEN** it returns a `TidyReport` with `removed.len() == 1`, `added.len() == 2`, `upgraded.len() == 1`

#### Scenario: Lint command returns report
- **WHEN** `lint::run()` finds 2 errors and 1 warning
- **THEN** the `LintReport` has `error_count == 2`, `warning_count == 1`, and `diagnostics.len() == 3`

#### Scenario: Upgrade with nothing to do
- **WHEN** all actions are up to date
- **THEN** `UpgradeReport` has empty `upgrades` vec and `up_to_date == true`

### Requirement: Structured OutputLine enum
The rendering layer SHALL produce `Vec<OutputLine>` from report structs. The `OutputLine` enum SHALL have variants for each semantic output type:

- `Upgraded` — action upgraded with from/to versions
- `Added` — action added with version
- `Removed` — action removed
- `Changed` — action changed with detail string
- `Skipped` — action skipped with reason
- `Warning` — warning message
- `LintDiag` — lint diagnostic with level, optional workflow, rule, message
- `Summary` — summary line
- `LogPath` — log file path
- `CiNotice` — CI mode notice message

#### Scenario: Render upgrade report with upgrades
- **WHEN** rendering an `UpgradeReport` with 2 upgrades (actions/checkout v6→v6.0.2, jdx/mise-action v3→v3.6.2)
- **THEN** output contains `OutputLine::Upgraded { action: "actions/checkout", from: "v6", to: "v6.0.2" }`
- **THEN** output contains `OutputLine::Upgraded { action: "jdx/mise-action", from: "v3", to: "v3.6.2" }`
- **THEN** output contains `OutputLine::Summary` with text "2 upgraded · 1 workflow updated"

#### Scenario: Render upgrade report up to date
- **WHEN** rendering an `UpgradeReport` with `up_to_date == true`
- **THEN** output contains a single `OutputLine::Summary` with text "All actions up to date"

#### Scenario: Render lint report with violations
- **WHEN** rendering a `LintReport` with 1 error and 1 warning
- **THEN** output contains `OutputLine::LintDiag` for each diagnostic
- **THEN** output contains `OutputLine::Summary` with text "1 error · 1 warning"

#### Scenario: Render lint report clean
- **WHEN** rendering a `LintReport` with zero diagnostics
- **THEN** output contains a single `OutputLine::Summary` with text "No lint issues found"

### Requirement: Colors applied at print boundary
The `Printer` struct SHALL apply ANSI colors only when writing `OutputLine` values to the terminal. Render functions SHALL NOT embed ANSI escape codes in `OutputLine` data.

Color SHALL be enabled when all conditions are met:
- Output is a TTY (detected via `console` crate)
- `NO_COLOR` environment variable is not set

Color mapping:
- `✓` and `Summary` (success) → green
- `✗` and `LintDiag` (error level) → red
- `⚠` and `LintDiag` (warn level) → yellow
- `↑` (upgraded) → cyan
- `+` (added) → green
- `−` (removed) → red
- `ℹ` (CI notice) → blue

#### Scenario: Output to TTY with colors
- **WHEN** stdout is a TTY and `NO_COLOR` is not set
- **THEN** `OutputLine::Upgraded` prints with cyan-colored `↑` symbol
- **THEN** `OutputLine::Summary` prints with green-colored `✓` symbol

#### Scenario: Output piped to file
- **WHEN** stdout is not a TTY (piped)
- **THEN** output contains no ANSI escape codes
- **THEN** emoji symbols (`↑`, `✓`, `✗`, `⚠`) are still present (they are Unicode, not ANSI)

#### Scenario: NO_COLOR environment variable set
- **WHEN** `NO_COLOR` is set to any value
- **THEN** output contains no ANSI escape codes

### Requirement: Log file always written for local runs
Every local (non-CI) gx invocation SHALL write a detailed log file to `{std::env::temp_dir()}/gx/{command}/{RFC-3339-date}.log`.

The log file SHALL contain:
- Timestamped entries in `[HH:MM:SS]` format
- All progress messages (resolution steps, API calls)
- All warnings and skipped action reasons
- Workflow file changes

The log file path SHALL be printed as the last line of user output via `OutputLine::LogPath`.

#### Scenario: Local upgrade writes log file
- **WHEN** `gx upgrade` runs locally (CI env var not set)
- **THEN** a log file is created at `{tmp}/gx/upgrade/{timestamp}.log`
- **THEN** the log file contains timestamped resolution detail
- **THEN** the last output line shows the log file path

#### Scenario: CI run does not write log file
- **WHEN** `gx upgrade` runs in CI (`CI` env var is set)
- **THEN** no log file is created
- **THEN** no `OutputLine::LogPath` appears in output

### Requirement: CI auto-detection with verbose inline output
When the `CI` environment variable is set (any value), gx SHALL:
1. Print `OutputLine::CiNotice` with message "CI detected, running in verbose mode" as the first output line
2. Print all progress/detail messages inline to stdout with timestamps
3. Skip spinner creation (return `None` from `printer.spinner()`)
4. Skip log file creation

#### Scenario: CI environment detected
- **WHEN** `CI=true` is set in the environment
- **THEN** the first output line is `ℹ CI detected, running in verbose mode`
- **THEN** all resolution steps appear inline with timestamps
- **THEN** no spinner is displayed

#### Scenario: CI not detected
- **WHEN** `CI` environment variable is not set
- **THEN** no CI notice is printed
- **THEN** spinner is displayed for long operations
- **THEN** log file is written

### Requirement: Default output formatting
Default (non-CI) output SHALL use these formatting rules:

- No `[INFO]`/`[WARN]` prefixes
- Emoji symbols as line prefixes: `↑` upgrade, `+` add, `−` remove, `~` change, `✓` success, `✗` error, `⚠` warning
- Action names left-aligned with consistent spacing
- Version transitions use `→` (Unicode arrow, not `->`)
- Summary line uses `·` (middle dot) as separator between counts
- One blank line before summary

#### Scenario: Upgrade output format
- **WHEN** 2 actions are upgraded
- **THEN** each upgrade line starts with ` ↑ ` followed by action name and `vX → vY`
- **THEN** a blank line separates the upgrade list from the summary
- **THEN** the summary reads ` ✓ 2 upgraded · N workflows updated`

#### Scenario: Empty tidy output
- **WHEN** tidy finds nothing to change
- **THEN** output is ` ✓ Up to date`
