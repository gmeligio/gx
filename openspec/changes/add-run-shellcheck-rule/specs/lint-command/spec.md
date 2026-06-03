## MODIFIED Requirements

### Requirement: run-shellcheck rule analyzes shell bodies of run steps

`gx lint` SHALL provide a `run-shellcheck` rule (default level: warn) that runs the `shellcheck` static analyzer over the shell body of each `run:` step whose effective shell is bash or sh, and reports each shellcheck finding as a diagnostic scoped to the workflow, job, and step. The rule is configured under `[lint.rules]` like every other rule, and its `ignore` list scopes by workflow/job/step.

When the `shellcheck` binary is not available on `PATH`, the rule SHALL NOT fail the lint run; it SHALL emit a single informational diagnostic stating that the rule was skipped because `shellcheck` was not found, and the overall `gx lint` exit code SHALL be unaffected by the rule's absence.

The user who benefits is the workflow maintainer whose `run:` blocks do real shell work (pipelines, path handling, conditional logic). Shell bugs there — an unquoted path that word-splits, a masked pipeline failure that hides an error — otherwise surface only as a subtly wrong run-time result. With this rule, `gx lint` surfaces them at edit time in the tool they already run, without installing and configuring a separate `actionlint`/`shellcheck` step.

#### Scenario: shell issue in a run step is reported

- **GIVEN** `shellcheck` is installed on `PATH`
- **AND** a `run:` step contains `rm $RUNNER_TEMP/file` (unquoted variable expansion)
- **WHEN** the user runs `gx lint`
- **THEN** a `run-shellcheck` diagnostic is reported for that step, including the shellcheck `SCxxxx` code and message
- **AND** the command exits with code 0 (default warn level)

#### Scenario: run-shellcheck escalated to error fails the run

- **GIVEN** `gx.toml` sets `run-shellcheck = { level = "error" }`
- **AND** a `run:` step has a shellcheck finding
- **WHEN** the user runs `gx lint`
- **THEN** the diagnostic is reported at error level and the command exits with code 1

#### Scenario: clean shell body produces no diagnostic

- **GIVEN** `shellcheck` is installed
- **AND** a `run:` step's body has no shellcheck findings
- **WHEN** the user runs `gx lint`
- **THEN** no `run-shellcheck` diagnostic is reported for that step

#### Scenario: shellcheck binary missing degrades gracefully

- **GIVEN** `shellcheck` is NOT on `PATH`
- **AND** a workflow contains `run:` steps
- **WHEN** the user runs `gx lint`
- **THEN** exactly one informational diagnostic states that `run-shellcheck` was skipped because `shellcheck` was not found
- **AND** the missing binary does not by itself cause a non-zero exit code

#### Scenario: non-shell run step is skipped

- **GIVEN** a `run:` step with `shell: pwsh`
- **WHEN** the user runs `gx lint`
- **THEN** no `run-shellcheck` diagnostic is reported for that step (only bash/sh bodies are analyzed)

#### Scenario: a noisy step can be ignored

- **GIVEN** `gx.toml` sets `run-shellcheck = { ignore = [{ workflow = ".github/workflows/build.yml", job = "compile" }] }`
- **WHEN** the user runs `gx lint`
- **THEN** no `run-shellcheck` diagnostic is reported for steps in the `compile` job of `build.yml`
