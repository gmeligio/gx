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

---

## Guardrail: Diagnostic producers do not embed kind-nouns

### Requirement: Diagnostic producers do not embed kind-nouns

Upstream producers — lint rules and command reports — SHALL emit structured data
(identifier, version, level, location) and SHALL NOT prefix an identifier in a
diagnostic message with a noun naming its kind. Choosing user-facing vocabulary is
reserved to the rendering boundary; a producer that has already committed to a noun
has taken that choice away from it.

This constrains a kind-noun used as a *label on an identifier the message goes on
to name* (`action actions/checkout uses …`), which is redundant and fixes the
vocabulary. It does not constrain a noun used as the grammatical subject of a
message that names no identifier (`workflow has no top-level permissions: block`),
where the word carries the sentence and the rule is type-narrowed to that kind.

**User value:** the maintainer running `gx lint` reads one consistent voice across
every rule, instead of `action actions/checkout uses tag reference…` from one rule
and `actions/checkout SHA … not found` from the next. It also keeps gx from
asserting the wrong word: the same rules now fire on composite action files, and
are planned to fire on GitLab CI components, so a message that hardcodes "action"
is a message that will eventually mislabel the artifact it just flagged.

This guardrail is load-bearing for the same reason as "Commands do not print
directly": a producer that composes its own sentence fragment reintroduces the
double-render and inconsistent-phrasing failures the single rendering boundary
exists to prevent.

#### Scenario: A rule reports a violation against an action
- **GIVEN** the `unpinned` rule flags `actions/checkout@v4` in `.github/workflows/ci.yml`
- **WHEN** the user runs `gx lint`
- **THEN** the diagnostic message names the identifier and the problem, e.g.
  `actions/checkout uses tag reference v4 instead of SHA pin`
- **AND** the message does not open with a noun naming the kind of thing
  (`action `, `workflow `, `component `)

#### Scenario: Every rule reports in the same voice
- **GIVEN** `gx lint` produces diagnostics from more than one rule in a single run
- **WHEN** the user reads the output
- **THEN** no diagnostic message labels an identifier with a kind-noun, so all
  rules that name an identifier read alike
- **AND** consistency is achieved by absence — a run in which every message carried
  the same label would be uniform but still violates this requirement

#### Scenario: A rule reports against a whole file it is type-narrowed to
- **GIVEN** the `missing-permissions` rule flags a workflow that declares no
  top-level `permissions:` block
- **WHEN** the user runs `gx lint`
- **THEN** the message may open with `workflow`, because it names no identifier and
  the rule cannot receive any other kind of file
- **AND** the renderer still supplies the file path, which the message does not repeat
