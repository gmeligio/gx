## ADDED Requirements

### Requirement: Spinner during long operations
The system SHALL display an ephemeral spinner on stderr during operations that make network calls (GitHub API). The spinner SHALL be created via `Printer::spinner()` which returns `Option<ProgressBar>`.

The spinner SHALL:
- Use the `indicatif` crate's `ProgressBar` with a spinner style
- Render to stderr (not stdout) to avoid interleaving with final output
- Be ephemeral — cleared before final output is printed
- Display the current phase message (e.g., "Resolving versions...", "Writing workflows...")

#### Scenario: Upgrade with spinner
- **WHEN** `gx upgrade` runs locally with a TTY
- **THEN** a spinner appears on stderr during GitHub API resolution
- **THEN** the spinner message updates as phases change
- **THEN** the spinner is cleared before the final upgrade report is printed to stdout

#### Scenario: Spinner not shown in CI
- **WHEN** `gx upgrade` runs with `CI` env var set
- **THEN** `printer.spinner()` returns `None`
- **THEN** no spinner is rendered

#### Scenario: Spinner not shown when piped
- **WHEN** stderr is not a TTY (output piped)
- **THEN** `printer.spinner()` returns `None`

### Requirement: Progress callback pattern
Commands that perform long operations SHALL accept an `on_progress: impl Fn(&str)` parameter. The callback is invoked at each phase transition with a human-readable message.

The orchestrator (app.rs / main.rs) SHALL connect the callback to:
1. The spinner's `set_message()` (if spinner exists)
2. The log file's `write()` (if log file exists)

Commands SHALL NOT depend on `indicatif` or any terminal crate. The callback is their only output channel during execution.

#### Scenario: Progress callback in production
- **WHEN** `upgrade::plan()` is called with a progress callback
- **THEN** the callback is invoked with messages like "Resolving versions..." and "Checking for upgrades..."
- **THEN** the spinner updates its message accordingly

#### Scenario: Progress callback in tests
- **WHEN** `upgrade::plan()` is called with `|_| {}` as the callback
- **THEN** the function executes identically (same return value, same side effects)
- **THEN** no output is produced

#### Scenario: Progress callback captures messages
- **WHEN** `upgrade::plan()` is called with a capturing callback (`|msg| msgs.push(msg)`)
- **THEN** the captured messages include all phase transitions in order
