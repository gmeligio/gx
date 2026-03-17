## Formatting

### Requirement: Output uses symbolic prefixes instead of log-level tags
Command output SHALL NOT include `[INFO]`, `[WARN]`, or similar log-level prefixes. Each output line uses a single-character emoji symbol to convey meaning at a glance:

| Symbol | Meaning |
|--------|---------|
| `^` | Upgrade |
| `+` | Add |
| `-` | Remove |
| `~` | Change |
| checkmark | Success |
| X | Error |
| warning | Warning |

### Requirement: Version transitions use a Unicode arrow
Version changes (e.g., `1.2.0 -> 1.3.0`) SHALL use a Unicode arrow character, not `->`.

### Requirement: Summary line is visually separated and compact
Command output SHALL print one blank line before the summary. The summary line uses a middle dot as separator between counts.

#### Scenario: User runs an upgrade that changes three dependencies
- **GIVEN** three dependencies are upgraded
- **WHEN** the command completes
- **THEN** the user sees one blank line followed by a single summary line like `3 upgraded · 0 added · 0 removed`

---

## Colors

### Requirement: Colors reflect the type of change
When color is enabled, output lines are colored by their symbol:

| Symbol | Color |
|--------|-------|
| `+` (add) | Green |
| `-` (remove) | Red |
| `^` (upgrade) | Cyan |
| `!` (warning) | Yellow |
| summary | Green |
| CI notice | Blue |

### Requirement: Colors respect terminal capability and user preference
Color is enabled when both conditions are met: output is a TTY AND the `NO_COLOR` environment variable is not set. When either condition fails, output is plain text with no ANSI escape codes.

#### Scenario: User pipes gx output to a file
- **GIVEN** stdout is not a TTY
- **WHEN** the user runs any gx command with output redirected
- **THEN** the output contains no ANSI escape codes

#### Scenario: User sets NO_COLOR
- **GIVEN** the `NO_COLOR` environment variable is set
- **WHEN** the user runs any gx command
- **THEN** the output contains no ANSI escape codes

---

## Spinner

### Requirement: Long operations show a spinner with phase messages
During network calls or other long-running operations, an ephemeral spinner SHALL appear on stderr showing the current phase (e.g., "Resolving versions...", "Fetching tags..."). The spinner is cleared before final output is printed.

#### Scenario: User runs an upgrade with network resolution
- **GIVEN** the command performs network requests
- **WHEN** the operation is in progress
- **THEN** the user sees a spinner on stderr with a message describing the current phase
- **AND** the spinner disappears before the final result is printed

### Requirement: Spinner is suppressed in non-interactive contexts
The spinner SHALL NOT appear when stderr is not a TTY or when running in CI. This prevents garbled output in log files and CI transcripts.

#### Scenario: User runs gx in a CI pipeline
- **GIVEN** the `CI` environment variable is set
- **WHEN** the user runs any gx command
- **THEN** no spinner is displayed

---

## Logging

### Requirement: Local runs produce a detailed log file
Every local (non-CI) invocation SHALL write a timestamped log to `{tmp}/gx/{command}/{RFC-3339-date}.log`. The log path is printed as the last line of output so the user can find it.

#### Scenario: User runs gx upgrade locally
- **GIVEN** the `CI` environment variable is not set
- **WHEN** the user runs `gx upgrade`
- **THEN** a log file is created at `{tmp}/gx/upgrade/{RFC-3339-date}.log`
- **AND** the last line of output shows the log file path

#### Scenario: User runs gx upgrade in CI
- **GIVEN** the `CI` environment variable is set
- **WHEN** the user runs `gx upgrade`
- **THEN** no log file is created

---

## CI Detection

### Requirement: CI runs use verbose inline output instead of spinners and log files
When the `CI` environment variable is set, the output adapts for non-interactive consumption:
1. A CI notice is printed as the first line
2. All progress phases are printed inline with timestamps (replacing the spinner)
3. No log file is created

#### Scenario: User reads gx output in a GitHub Actions log
- **GIVEN** the `CI` environment variable is set
- **WHEN** the user runs any gx command
- **THEN** the first line is a CI notice
- **AND** progress messages appear inline with timestamps
- **AND** the final result follows with the same formatting as local runs (symbols, colors if supported)

---

## Guardrail: Commands do not print directly

Command logic SHALL NOT call print or logging macros directly. All user-visible output flows through a single rendering boundary. This guardrail ensures that colors, CI detection, spinner suppression, and log-file writing behave consistently across every command without per-command bugs.
