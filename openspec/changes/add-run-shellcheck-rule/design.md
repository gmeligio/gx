## Context

`gx lint` rules operate on `Context.workflows_full: &[ParsedWorkflow]`. `Step` already exposes `run: Option<String>` and `scalar_text()`. What it lacks for this rule is `shell: Option<String>` — needed to skip non-shell `run:` steps (`shell: pwsh`/`python`).

shellcheck is a mature external static analyzer. `actionlint` integrates it by extracting `run:` bodies and invoking `shellcheck` as a subprocess; this change does the same, natively in gx, rather than reimplementing shellcheck.

## Goals / Non-Goals

**Goals:**

- Bring shellcheck coverage of `run:` blocks into `gx lint`.
- Degrade gracefully when `shellcheck` is not installed — never hard-fail the lint run because of a missing optional binary.
- Match the existing rule idiom (one file, `RuleName`, `[lint.rules]` config, workflow/job/step-scoped `ignore`).

**Non-Goals:**

- Reimplementing shellcheck in Rust.
- Analyzing non-bash shells (`pwsh`, `python`, `cmd`).
- Perfect YAML line-number mapping; step-level locus is sufficient.

## Decisions

### Decision: Shell out to `shellcheck`, default level `warn`

Reuse the canonical analyzer rather than reimplement it. Default `warn` because shellcheck findings span real bugs to style; a default-error rule would be too aggressive on upgrade. Users opt into `error` for CI-blocking behavior.

### Decision: Graceful degradation when the binary is absent

On `check`, probe for `shellcheck` on `PATH` once. If absent, emit exactly one informational diagnostic ("`run-shellcheck` skipped: `shellcheck` binary not found on PATH") and return — the lint run still succeeds. This is spec'd behavior, not silent: a user who configured the rule learns why it isn't firing. Rationale: gx is a pinning tool first; forcing a shellcheck install on every user would regress its zero-dependency story.

### Decision: Analyze only bash/sh `run:` steps

A step is analyzed when `run:` is present AND (`shell:` is absent OR `shell:` ∈ {`bash`, `sh`, `bash -e ...` forms}). Steps with `shell: pwsh`/`python`/etc. are skipped. This requires adding `Step.shell: Option<String>` to the parse model.

Caveat captured as a risk: default shell is bash only on Linux/macOS runners; on `runs-on: windows-*` an absent `shell:` means `pwsh`. The first cut treats absent-`shell:` as bash regardless of `runs-on` (the common case); refining by runner OS is a documented follow-up. The rule's `ignore` lets a user silence a Windows step in the meantime.

### Decision: Subprocess invocation and locus

Extract each analyzed step's `run:` body, feed it to `shellcheck` (format `gcc` or `json1` for parseable output). Map each finding back to its (workflow, job, step-index) and build a `Diagnostic` scoped with `.with_workflow().with_job().with_step()`, message = shellcheck `SCxxxx` + text. The diagnostic locus is the step, not a YAML line — actionable without exact line mapping. Batch multiple scripts per `shellcheck` invocation where practical to bound subprocess overhead.

## Risks / Trade-offs

- **[Risk] Missing `shellcheck` binary** → Mitigation: detect + single skip diagnostic + lint succeeds (spec'd).
- **[Risk] Line-number drift** → Mitigation: step-level locus is primary; shellcheck's in-script line is secondary context.
- **[Risk] Windows default-shell misdetection** → Mitigation: documented; `ignore` available; runner-OS refinement is a follow-up.
- **[Trade-off] Subprocess per step** → Mitigation: batch invocations; default-warn keeps it off the critical CI-failure path for most users.

## Migration Plan

- Additive, default-warn. CHANGELOG notes the rule and the optional `shellcheck` dependency. gx's CI/toolchain adds `shellcheck` (mise) so the rule is active in gx's own pipeline.
- No config migration.

## Open Questions

- None blocking. Runner-OS-aware default-shell detection is a deliberate follow-up (Non-Goal for this cut).
