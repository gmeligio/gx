## Why

`run:` blocks in GitHub Actions workflows are shell scripts, and they carry the same hazards as any shell script: unquoted variables that word-split, unset variables that expand to nothing, `cd` that can fail silently, masked pipeline exit codes. These bugs are invisible to YAML validation and to gx's existing action-hygiene and security rules — they only manifest at run time, often as a subtly wrong result rather than a hard failure.

The standard tool is `shellcheck`, and the standard way to apply it to workflows is the `actionlint` binary (which extracts `run:` blocks and pipes them through shellcheck). The `add-workflow-security-rules` proposal explicitly deferred "shellcheck integration" as out of scope for that change. This change picks it up.

The motivating user is the flutter-docker-image maintainer, who runs `gx lint` in CI and whose `update-version.yml` is heavy on `run:` blocks doing `jq`/`curl`/`git` pipelines — exactly the code where an unquoted `$RUNNER_TEMP` path or a masked `jq` failure causes a silently wrong version manifest. They currently reach for the `actionlint` binary to get shellcheck coverage; this brings it into the tool they already run.

**Out of scope (deferred):**

- Re-implementing shellcheck's analysis in Rust. This change shells out to the `shellcheck` binary — reimplementing a mature static analyzer is not justified.
- `run:` blocks with a non-shell `shell:` (e.g. `shell: pwsh`, `shell: python`). Only POSIX/bash `run:` steps are analyzed; others are skipped.
- Workflow-command deprecations inside `run:` (`echo "::set-output::"`). Could layer on later; not part of the shellcheck pass.

## What Changes

- One new rule added to `gx lint`:

| Rule name | Default | What it catches |
|-----------|---------|-----------------|
| `run-shellcheck` | warn | A `run:` step's shell body produces `shellcheck` findings (unquoted expansions, unset vars, masked exit codes, etc.). |

- Default level is **warn**, not error: shellcheck findings range from real bugs to style nits, and a default-error rule that fires on existing workflows would be too aggressive on upgrade. Users who want CI-blocking shell checks set `run-shellcheck = { level = "error" }`.
- **External binary dependency.** The rule invokes the `shellcheck` binary. When `shellcheck` is not on `PATH`, the rule emits a single diagnostic explaining it was skipped (it does NOT fail the lint run). This keeps gx usable without shellcheck installed while making the gap visible. The detection/skip behavior is part of the spec.
- The rule analyzes only `run:` steps whose effective shell is `bash`/`sh` (the Actions default on Linux/macOS runners). Steps with an explicit non-shell `shell:` are skipped. (Note: the current `domain::workflow_parsed::Step` does not model `shell:`; this change adds `Step.shell: Option<String>` so the rule can honor it.)
- shellcheck is invoked with the step body wrapped so line numbers map back to the workflow; the diagnostic message includes shellcheck's `SCxxxx` code and message, scoped to the workflow/job/step.
- `RuleName` gains `RunShellcheck`. `[lint.rules]` config + `ignore` (workflow/job/step scoped) extend with no new syntax — users can `ignore` a specific noisy step.
- README and `docs/lint-rules.md` document the rule and the `shellcheck` binary requirement.

## Capabilities

### Modified Capabilities

- `lint-command`: gains one new rule requirement and the contract for graceful degradation when the `shellcheck` binary is absent. The capability's user-value statement expands to cover **shell-script correctness inside `run:` blocks**.

### New Capabilities

_None._ Expansion of `lint-command`, consistent with the security and validity rule families.

## Spec gate

Required. Adds user-facing behavior (a new rule + a new external-binary dependency with defined degradation behavior). Meets the relevance gate.

## Impact

- **User-visible change**: `gx lint` produces new warn-level diagnostics on workflows with shell issues. Default-warn means it does not break CI on upgrade unless the user opts into error. CHANGELOG notes the new rule and the `shellcheck` dependency.
- **New runtime dependency**: `shellcheck` binary, discovered on `PATH`. Absent → rule self-skips with one informational diagnostic; lint still succeeds. gx's own CI and docs add `shellcheck` to the toolchain (via mise).
- **Affected source**: new `src/lint/run_shellcheck.rs`; `Step.shell` added to `domain::workflow_parsed`; new `RuleName` variant; registry wiring; a small process-invocation helper under `src/infra/`.
- **Performance**: one `shellcheck` subprocess per analyzed `run:` step (batchable — shellcheck accepts multiple scripts). For typical repos this is tens of ms; large workflows with many `run:` steps may add a few hundred ms. Acceptable for a lint pass; consider batching in implementation.
- **Risk — line-number mapping**: extracting a `run:` body and feeding it to shellcheck can misreport line numbers relative to the YAML. Mitigation: report the step (workflow/job/step index) as the primary locus; include shellcheck's in-script line as secondary. The diagnostic is actionable even without exact YAML line mapping.
- **Risk — shell detection**: assuming bash when `shell:` is absent is correct for Linux/macOS runners but not Windows (`pwsh`/`cmd`). Mitigation: the rule analyzes a step only when `shell:` is absent or explicitly bash/sh; a future enhancement could consider `runs-on` to refine the default-shell guess.
