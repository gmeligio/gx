## MODIFIED Requirements

### Requirement: Workflow-validity rules detect structurally broken references

`gx lint` SHALL run a family of workflow-validity rules that flag references which GitHub Actions accepts at parse time but that fail or silently resolve to nothing at run time. These rules operate on the structural parse of each workflow and are configured under `[lint.rules]` like every other rule.

The user who benefits is the workflow maintainer refactoring a multi-job workflow (renaming jobs, rewiring `needs:`, moving `steps.<id>.outputs` references). They would otherwise discover a dangling reference only when a scheduled or dispatched run misfires — a blank output or an "unknown job" error far from the edit that caused it. With these rules, `gx lint` surfaces the break at edit time, in the tool they already run, without reaching for a second binary.

The rules:

- `dangling-reference` (default: error) — a job's `needs:` lists a job id that does not exist in the workflow.
- `invalid-expression` (default: error) — a `${{ }}` reference to `needs.<job>.…` where `<job>` is not in the referencing job's `needs:` list, or `steps.<id>.…` where no earlier step in the same job declares that `id`.

Both rules SHALL only flag references they can fully resolve to a bare identifier. A reference whose job/step segment is dynamic (indexed by `matrix`, built by `format(...)`, or otherwise not a literal identifier) SHALL NOT be flagged. Contexts other than `needs.*` and `steps.*` (such as `env`, `vars`, `matrix`, `inputs`, `github`, `secrets`, `runner`, `job`) SHALL NOT be flagged.

#### Scenario: needs references a nonexistent job

- **GIVEN** a workflow with a job `deploy` whose `needs:` is `[buld]` (typo for `build`)
- **AND** no job named `buld` exists in the workflow
- **WHEN** the user runs `gx lint`
- **THEN** a `dangling-reference` diagnostic is reported for job `deploy` naming the missing `buld`
- **AND** the command exits with code 1 (default error level)

#### Scenario: needs as a scalar is accepted

- **GIVEN** a workflow where job `test` declares `needs: build` (scalar form) and a job `build` exists
- **WHEN** the user runs `gx lint`
- **THEN** no `dangling-reference` diagnostic is reported for job `test`

#### Scenario: expression reads an undeclared needs job

- **GIVEN** a job `pr` that declares `needs: [compose]` and references `${{ needs.validate.outputs.id }}` in a step
- **AND** `validate` is a real job in the workflow but is not in `pr`'s `needs:` list
- **WHEN** the user runs `gx lint`
- **THEN** an `invalid-expression` diagnostic is reported for job `pr` explaining that `needs.validate` is unresolvable because `validate` is not in `pr`'s `needs:`
- **AND** the command exits with code 1

#### Scenario: expression reads a nonexistent step id

- **GIVEN** a job whose step references `${{ steps.upload.outputs.artifact-id }}`
- **AND** no earlier step in that job declares `id: upload`
- **WHEN** the user runs `gx lint`
- **THEN** an `invalid-expression` diagnostic is reported naming the missing step id `upload`

#### Scenario: expression reads a valid step id

- **GIVEN** a job with an earlier step `id: upload` and a later step referencing `${{ steps.upload.outputs.artifact-id }}`
- **WHEN** the user runs `gx lint`
- **THEN** no `invalid-expression` diagnostic is reported for that reference

#### Scenario: dynamic reference is not flagged

- **GIVEN** a step referencing `${{ needs[matrix.target].outputs.x }}`
- **WHEN** the user runs `gx lint`
- **THEN** no `invalid-expression` diagnostic is reported for that reference (the job segment is not a bare identifier)

#### Scenario: out-of-scope context is not flagged

- **GIVEN** a step referencing `${{ env.FLUTTER_VERSION_PATH }}` and `${{ matrix.os }}`
- **WHEN** the user runs `gx lint`
- **THEN** no `invalid-expression` diagnostic is reported for those references

#### Scenario: rule can be disabled

- **GIVEN** `gx.toml` sets `dangling-reference = { level = "off" }`
- **WHEN** the user runs `gx lint` on a workflow with a dangling `needs:`
- **THEN** no `dangling-reference` diagnostic is reported
