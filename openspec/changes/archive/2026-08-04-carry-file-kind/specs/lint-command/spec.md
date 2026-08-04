## MODIFIED Requirements

### Requirement: Workflow-schema rules do not run on composite action files

The system SHALL evaluate the rules that read workflow-schema structure —
`missing-permissions`, `excessive-permissions`, `dangerous-trigger`,
`missing-concurrency`, `pr-head-checkout`, `unprotected-secrets`,
`dangling-reference`, `invalid-expression`, and `run-shellcheck` — against
workflow files only. A composite action definition has no `on:` block, no
top-level `permissions:` block, and no jobs; these rules SHALL NOT produce
diagnostics for such a file.

This SHALL hold for every file gx has discovered as an action definition,
whatever directory it lives in — not only for files under `.github/actions`.

`run-shellcheck` is included because it reaches `run:` bodies through
workflow-schema structure. Extending it to a composite action's `runs.steps` is
deferred rather than impossible: that schema has no `defaults:` block, so the
shell precedence chain the rule depends on needs its own decision first.

**User value:** a maintainer who adopts composite actions gets the new pinning
coverage without a wall of meaningless diagnostics telling them their
`action.yml` is missing a `permissions:` block it is not allowed to have. This
holds wherever they keep the file: previously the exemption was granted by
checking the path for an `actions` directory under `.github`, so an action
definition kept anywhere else received the full set of workflow diagnostics —
every one of them meaningless for a schema that cannot satisfy them.

#### Scenario: Composite action is not flagged for a missing permissions block
- **GIVEN** `.github/actions/setup/action.yml` with `runs.using: composite` and
  no `permissions:` block
- **WHEN** the user runs `gx lint` with `missing-permissions` enabled
- **THEN** no diagnostic is produced for that file
- **AND** workflow files missing a `permissions:` block are still flagged

#### Scenario: Composite action is not flagged by trigger or job-graph rules
- **GIVEN** `.github/actions/setup/action.yml` with `runs.using: composite`
- **WHEN** the user runs `gx lint`
- **THEN** `dangerous-trigger`, `missing-concurrency`, `unprotected-secrets`,
  `pr-head-checkout`, and `dangling-reference` produce no diagnostics for that
  file

#### Scenario: run-shellcheck does not analyze composite run bodies
- **GIVEN** `.github/actions/setup/action.yml` with `runs.using: composite` and
  a `run:` step containing a shell issue shellcheck would report
- **WHEN** the user runs `gx lint` with `run-shellcheck` enabled
- **THEN** no diagnostic is produced for that file
- **AND** equivalent `run:` bodies in workflow files are still analyzed

#### Scenario: An action definition outside `.github/actions` is exempt too
- **GIVEN** a file gx has discovered as an action definition
- **AND** its path is not under `.github/actions`
- **WHEN** the user runs `gx lint`
- **THEN** none of the workflow-schema rules produce diagnostics for that file
- **AND** the per-action rules (`unpinned`, `sha-mismatch`, `stale-comment`)
  still apply to its `uses:` references
