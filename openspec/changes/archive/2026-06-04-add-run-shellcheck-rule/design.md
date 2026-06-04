## Context

`gx lint` rules operate on `Context.workflows_full: &[ParsedWorkflow]`. `Step` already exposes `run: Option<String>` and `scalar_text()`. What it lacks for this rule is `shell: Option<String>` — needed to skip non-shell `run:` steps (`shell: pwsh`/`python`).

[shellcheck](https://github.com/koalaman/shellcheck) is the de-facto-standard shell static analyzer (~250 checks, Haskell, GPL-3.0). [actionlint](https://github.com/rhysd/actionlint) integrates it by extracting `run:` bodies and invoking `shellcheck` as a subprocess ([`rule_shellcheck.go`](https://github.com/rhysd/actionlint/blob/main/rule_shellcheck.go)); this change does the same, rather than reimplementing shellcheck.

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

### Decision: Shell out to `shellcheck` rather than build a Rust linter

The alternative — analyze `run:` bodies natively in Rust — was investigated and rejected on evidence, not assumption.

A crates.io survey found mature shell **parsers** but no shell **linter**:

| Crate | Recent dl | What it is |
|---|---|---|
| [tree-sitter-bash](https://crates.io/crates/tree-sitter-bash) | 3.3M | bash grammar (parser/CST) |
| [brush-parser](https://crates.io/crates/brush-parser) | 102k | POSIX/bash parser (AST) |
| [conch-parser](https://crates.io/crates/conch-parser) | 2k | shell parser, ~unmaintained |
| [yash-syntax](https://crates.io/crates/yash-syntax) | 3k | POSIX parser, **GPL-3.0** |
| [shellcheck-sarif](https://crates.io/crates/shellcheck-sarif) | 17k | reformats shellcheck output — *still needs the binary* |

Parsing bash is the easy 20%; shellcheck's value is the ~250 dataflow/quoting/dialect checks behind it — the 80% nobody has reimplemented in Rust. So "native library" is not a live option: it reduces to reimplementing shellcheck on top of a parser, which is the explicit non-goal. The decision is purely *how* to integrate the binary, not *whether* to.

Reuse the canonical analyzer: maximum coverage, zero rule-maintenance burden (upstream owns it), and parity with what actionlint users already get. Default level `warn` because shellcheck findings span real bugs to style; a default-error rule would be too aggressive on upgrade. Users opt into `error` for CI-blocking behavior.

### Decision: Isolate the subprocess behind a `ShellChecker` trait

gx has **no `std::process::Command` usage today** — this is its first external-binary call. Rather than entangle process I/O with rule logic, introduce a seam:

```text
RunShellcheckRule (pure: sanitize ${{ }}, resolve shell, map findings → Diagnostic)
        │ depends on
        ▼
trait ShellChecker { fn check(&self, script, shell) -> Outcome }
   ├─ ShellcheckCli   (real: spawns the binary, parses -f json)
   └─ FakeChecker     (tests: returns canned findings)

enum Availability { Present(ShellcheckCli), Absent }   // probed once per run
```

The rule's logic becomes pure and unit-testable with no binary on PATH (satisfies tasks 2.2/3.5). Missing-binary is a typed `Availability::Absent`, not a stringly error. A future swap (a hypothetical Rust linter, actionlint-as-library) is a one-adapter change. This is the one bit of "idiomatic Rust" that earns its keep — it is what makes the rule testable, not gold-plating.

### Decision: Match actionlint's preprocessing (not a naive `shellcheck < body`)

A raw `shellcheck` over a `run:` body produces false positives on any workflow containing `${{ }}`, because `$` collides with shell variable syntax. actionlint solves this and we port its pipeline verbatim ([`rule_shellcheck.go:130-202`](https://github.com/rhysd/actionlint/blob/main/rule_shellcheck.go)):

1. **Blank `${{ ... }}` to equal-length underscores** (`echo '${{ matrix.os }}'` → `echo '________________'`) — preserves columns. Without this the rule is unusable; this is correctness-blocking, not optional.
2. **Exclude six SC codes** that the blanking would otherwise trip: `SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043` (file-not-found in CI env; "constant expression"/"word is constant"/`-z` artifacts of the underscore substitution; var-referenced-not-assigned, since `run:` reads `env:`).
3. **Prepend the runtime shell setup** GitHub uses: `set -eo pipefail` for bash, `set -e` for sh — so masked-pipeline findings match real behavior. Reported line numbers are then offset by `-1` to account for the prepended line.

Invocation matches actionlint: script on **stdin** (`-`), `--norc -f json -x --shell <sh> -e <codes>`. One subprocess per analyzed step (actionlint does not batch; neither do we — batching complicates line mapping for no measured gain).

### Decision: Graceful degradation when the binary is absent

On `check`, probe for `shellcheck` on `PATH` once. If absent, emit exactly one informational diagnostic ("`run-shellcheck` skipped: `shellcheck` binary not found on PATH") and return — the lint run still succeeds. This is spec'd behavior, not silent: a user who configured the rule learns why it isn't firing. Rationale: gx is a pinning tool first; forcing a shellcheck install on every user would regress its zero-dependency story.

### Decision: Analyze only bash/sh `run:` steps, resolving shell through `defaults.run.shell`

A step is analyzed when `run:` is present AND its **effective shell** ∈ {`bash`, `sh`} (incl. `bash -e ...`/`sh ...` prefix forms, normalized to `bash`/`sh`). Steps whose effective shell is `pwsh`/`python`/`cmd`/etc. are skipped.

actionlint resolves the effective shell through four sources in precedence order ([`rule_shellcheck.go:106-122`](https://github.com/rhysd/actionlint/blob/main/rule_shellcheck.go)):

```text
step.shell  →  job.defaults.run.shell  →  workflow.defaults.run.shell  →  runner-OS default
```

This change models the **first three** (step + job/workflow `defaults.run.shell`) — the correctness floor for the Linux/macOS repos gx targets, where `defaults.run.shell: sh` (or bash) is common. This requires adding to the parse model:

- `Step.shell: Option<String>`
- `defaults.run.shell` at workflow and job level (a small `Defaults { run: Option<RunDefaults { shell: Option<String> }> }`).

The **fourth source (runner-OS)** is a documented Non-Goal for this cut: absent any `shell:`/`defaults`, treat the step as bash (the common case). On `runs-on: windows-*` the real default is `pwsh`, so an absent-shell Windows step would be analyzed as bash — a false-positive risk mitigated by (a) `ignore` and (b) a follow-up that reads `runs-on`. Modeling `runs-on` + matrix labels is more than the first cut warrants.

### Decision: Step-level diagnostic locus

shellcheck's in-script line cannot be reliably mapped to the YAML line (block scalars `|`, `>`, `|-`, etc. strip indentation/blank lines; an exact map needs a sourcemap). Like actionlint, report the **step** as the locus (`.with_workflow().with_job().with_step()`) and include shellcheck's in-script line + `SCxxxx` code in the message as secondary context. Actionable without exact YAML line mapping.

## Risks / Trade-offs

- **[Risk] Missing `shellcheck` binary** → Mitigation: probe once (`Availability::Absent`) + single skip diagnostic + lint succeeds (spec'd).
- **[Risk] False positives from `${{ }}`** → Mitigation: blank expressions to underscores + exclude the six artifact-prone SC codes (actionlint parity).
- **[Risk] Line-number drift** (block scalars, prepended setup line) → Mitigation: step-level locus is primary; shellcheck's in-script line (offset `-1`) is secondary context.
- **[Risk] Windows default-shell misdetection** → Mitigation: documented; `ignore` available; runner-OS refinement is a follow-up. `defaults.run.shell` is modeled so the common non-Windows case is correct.
- **[Trade-off] Subprocess per step** → Mitigation: default-warn keeps it off the critical CI-failure path for most users; one process per step matches actionlint (no batching, which would complicate line mapping).

## Migration Plan

- Additive, default-warn. CHANGELOG notes the rule and the optional `shellcheck` dependency. gx's CI/toolchain adds `shellcheck` (mise) so the rule is active in gx's own pipeline.
- No config migration.

## Open Questions

- None blocking. Runner-OS-aware default-shell detection is a deliberate follow-up (Non-Goal for this cut).
