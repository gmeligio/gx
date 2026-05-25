# Workflow security rules — design notes

## Why not depend on zizmor

zizmor (Rust, MIT) covers a strict superset of the rules proposed here. The reasons not to wrap or depend on it:

1. **gx is itself a Rust binary**. Shelling out to a second binary for lint contradicts gx's single-binary value proposition.
2. **Linking zizmor as a library** is not supported — it is structured as a CLI, not a published crate. Vendoring its rule sources would be a maintenance burden (track upstream changes, manage version skew with the gx version it ships with).
3. **gx's rule API is already well-shaped** (`Rule` trait, `Context`, `Diagnostic`, severity, ignore-targeting). The six rules here are 50-200 lines of Rust each and reuse this API. There is nothing to gain from reaching for an external tool.
4. **Diagnostic formatting consistency**. Users have to read both tools' output today; folding the rules into gx means one severity convention, one ignore syntax, one summary line.

The cost is reproducing zizmor's research (which patterns are dangerous, what edge cases exist). This is mitigated by the rules below being **narrow and well-defined** — `dangerous-trigger` is "does the workflow have `on: pull_request_target:`," which is a one-line YAML query.

## Workflow parsing

The existing `domain::workflow::Scanner` reads workflow files and emits `WorkflowAction` instances (essentially `uses:` lines with location info). It does not parse the rest of the YAML — triggers, permissions, jobs are invisible to the rules engine.

To support workflow-level rules, introduce `domain::workflow::Parsed`:

```rust
pub struct Parsed {
    pub path: WorkflowPath,
    pub on: Trigger,              // pull_request | pull_request_target | push | schedule | ...
    pub permissions: Option<Permissions>,
    pub concurrency: Option<Concurrency>,
    pub jobs: Vec<Job>,
}

pub struct Job {
    pub id: String,
    pub permissions: Option<Permissions>,
    pub r#if: Option<String>,     // raw expression text
    pub steps: Vec<Step>,
}

pub struct Step {
    pub uses: Option<String>,
    pub r#if: Option<String>,
    pub with: serde_yaml::Value,  // structured but heterogeneous; opaque is fine for now
    pub run: Option<String>,
}
```

The `Scanner` runs `serde_saphyr::from_str` once per workflow (saphyr is already the YAML parser in `src/infra/workflow_scan/scanner.rs:192`) and produces a `Parsed`; it derives the existing `WorkflowAction` list from `Parsed.jobs[].steps[].uses`. This avoids parsing the same file twice and reuses saphyr's handling of GitHub workflow YAML edge cases — notably the `on:` key, which collides with YAML 1.1's truthy literal parsing in many other deserializers.

`Context` gains:

```rust
pub struct Context<'ctx> {
    pub manifest: &'ctx Manifest,
    pub lock: &'ctx Lock,
    pub workflows: &'ctx [LocatedAction],
    pub workflows_full: &'ctx [Parsed],   // NEW
    pub action_set: &'ctx WorkflowActionSet,
}
```

Existing rules continue to use `workflows`; new rules use `workflows_full`. No breaking change to rule implementations.

## Rule-by-rule design

### `missing-permissions` (error)

Pure structural check: is there a top-level `permissions:` block? If absent, emit diagnostic. No false positives possible.

### `excessive-permissions` (warn)

Top-level `permissions:` is "excessive" when it contains anything other than `contents: read`. This encodes the principle "broader scopes belong at job level."

False-positive cases:
- A workflow whose **every job** needs `contents: write` would prefer to declare it once at top level. The ignore mechanism (`ignore = [{ workflow = "..." }]`) covers this.
- The rule defaults to **warn**, not error, acknowledging it is a stylistic guideline.

### `dangerous-trigger` (error)

Single-condition check: does `Parsed.on` contain `pull_request_target` OR `workflow_run`? Emit one diagnostic per match, naming the trigger.

Both triggers share the same exploit shape: they run in the **target repository's context** (full secrets, write-scoped `GITHUB_TOKEN`) and are triggerable by fork PRs.

- `pull_request_target` is the textbook "pwn request" trigger — runs base-repo workflow code with PR metadata in scope.
- `workflow_run` is the GitHub-recommended *mitigation* for `pull_request_target` (run untrusted code in an unprivileged `pull_request` workflow, consume its artifacts in a privileged `workflow_run`). It is still dangerous: the privileged half has full secrets, and attacker-controlled artifact contents can drive arbitrary behavior in the consumer. Per zizmor's `dangerous-triggers` audit, `github.repository == ...` guards do not help — `workflow_run` always runs in the target repo.

Users with a reviewed `workflow_run` consumer that strictly validates artifact provenance (e.g., a publish workflow gated on a tag trigger) opt out per-workflow via `ignore`. Likewise for `pull_request_target` workflows that operate only on PR metadata and never check out PR code.

Sources:
- https://docs.zizmor.sh/audits/#dangerous-triggers
- https://securitylab.github.com/research/github-actions-preventing-pwn-requests/

### `pr-head-checkout` (error)

Two-condition check:
1. The workflow has at least one job with `permissions:` containing a write scope OR uses any `secrets.*` reference.
2. A step's `with.ref` contains the literal substring `github.event.pull_request.head.sha`, `github.head_ref`, or `github.event.pull_request.head.ref`.

If both are true, emit. The "privileged" condition is the load-bearing one — checking out PR HEAD on a *non-privileged* workflow (e.g., a build that only reads code and pushes nowhere) is safe.

### `missing-concurrency` (warn)

`Parsed.on` contains `push` or `schedule`, and `Parsed.concurrency` is `None`. Emit warning. Pure structural check.

False-positive: workflows where two simultaneous runs are intentional (e.g., a fan-out scheduler). Ignore via per-workflow opt-out. Default-warn (not error) reflects that this is a "you probably want this" rule, not a security boundary.

### `unprotected-secrets` (error)

The hardest rule. Logic:

1. `Parsed.on` contains `pull_request` (not `pull_request_target`; that's `dangerous-trigger`).
2. Walk every step in every job.
3. For each step that references `secrets.<NAME>` where `<NAME> != "GITHUB_TOKEN"` (in `with`, `env`, `run`) — collect the step's effective `if:` (job-level `if:` AND step-level `if:`, concatenated).
4. If the effective `if:` does NOT contain `github.event.pull_request.head.repo.full_name == github.repository` (or the equivalent `github.repository_owner ==` shape) — emit.

This is a textual contains-check on the raw expression text, not a full expression parser. It will miss exotic equivalents (e.g., a separate environment variable set elsewhere). Document this in the rule's help text — gx prefers conservative false-negatives over false-positives for the default-error rules.

**Why `GITHUB_TOKEN` is excluded.** Unlike user-managed secrets, GitHub automatically downgrades `secrets.GITHUB_TOKEN` to read-only on fork PRs regardless of the workflow's declared permissions. Flagging it on every `pull_request` workflow that touches `actions/checkout` (which implicitly uses `GITHUB_TOKEN`) would dominate the diagnostics with low-value noise. Workflows that have widened `GITHUB_TOKEN` via top-level `permissions:` are already caught by `excessive-permissions`.

False-positive analysis:
- A step gated by a custom expression (e.g., `if: env.IS_FORK != 'true'` set in an earlier step) is still flagged. **This is intentional.** The rule's value is making the canonical gate explicit; opt out via `ignore` if the custom gate has been reviewed.
- Reusable workflows invoked with `secrets: inherit` — the actual secret reference is in the callee and gx cannot see across files. Document as a known limitation.
- Workflows that intentionally accept secret risk from fork PRs (none should exist, but if they do) opt out via ignore.

## Determinism and ordering

Diagnostics are emitted per-rule, sorted by `(workflow_path, job_id, step_index, rule_name)` for stable output. This matches the existing rule output style.

## Automated Test Strategy

**Critical path**: end-to-end `gx lint` invocation against a fixture repo where each new rule has at least one violating workflow and one clean workflow. The integration test asserts exit code, diagnostic count per rule, and the rendered output. This catches regressions in rule wiring (forgot to register a rule, wrong default level, severity mapping bug) that pass unit tests but break the user-visible contract.

**Unit level**: each rule gets four standard test shapes:
- A "passes on clean workflow" test
- A "fails on the documented bad case" test
- An "ignore mechanism scopes correctly" test
- A "rule disabled (`level = "off"`) = no diagnostic" test

**New test infrastructure**: a `tests/fixtures/workflow_security/` directory with a handful of small workflow YAMLs covering the six rule categories. Re-uses the existing per-rule test harness pattern from `src/lint/sha_mismatch.rs` etc. — no new test framework.

**Parser regression test**: one test that round-trips the existing `WorkflowAction` list through the new `Parsed` type to confirm task 1.3's claim that the single parse produces equivalent output to today's scan. This guards against silent drift in the action-extraction logic.

## Observability

The lint command's failure modes already surface through the `Diagnostic` channel — every rule violation becomes a printed diagnostic with rule name, severity, file path, and (for new step-scoped rules) job + step index. No silent failures from rule logic itself.

Parse failures on workflow YAML route through the existing `WorkflowError::ParseFailed { path, reason }` shape (`src/domain/workflow.rs`) and surface as an error message naming the file and the underlying saphyr error. The lint runner already prints these and exits non-zero (see `command-output` spec). The new full-YAML parse uses the same error path — if a workflow fails to parse, the user sees the file and the reason, not a generic "lint failed."

Rule-config drift (e.g., a user setting `level = "off"` on a rule then upgrading gx) is detected at manifest-parse time via the existing `RuleName::from_str` validation, which fails the parse with the unrecognized rule name. The six new rule names extend the same enum so they participate in this check automatically.

Determinism: diagnostics are emitted sorted by `(workflow_path, job_id, step_index, rule_name)`. Non-deterministic ordering in CI logs is a real debugging hazard; the sort key matches the existing rule output style so the regression set stays comparable across runs.

## Alternatives considered and rejected

- **Embed zizmor as a subprocess.** Rejected: contradicts single-binary; doubles install surface.
- **New `gx audit` subcommand instead of expanding `lint`.** Rejected: users want one command to fail CI. Two commands means two CI steps and two exit-code reconciliations.
- **One new capability per rule.** Rejected: would fragment `gx lint --help` and the lint-command spec into seven pages. Users discover rules through one capability.
- **Defer everything to a "policy plugin" interface.** Rejected: speculative. Six built-in rules cover the documented use cases; a plugin interface can be added later if a third party wants to ship rules.
- **Cover `workflow_run` in a separate rule.** Rejected. `workflow_run` and `pull_request_target` share the same exploit shape (target-repo context + fork-triggerable + full secrets). One rule with one ignore mechanism is less noisy than two rules with parallel configs. The diagnostic message names the specific trigger so the user can act on the right line.
- **Extend `missing-concurrency` to also check `cancel-in-progress: true`.** Deferred to a follow-up. The presence check covers the most common omission; adding a value check requires a small expansion of the `Parsed.concurrency` shape and one more scenario. Worth doing, but cleanly separable from this change.
- **Silence `pr-head-checkout` under `pull_request_target`.** Rejected. The two rules describe two independent fixes (drop the dangerous trigger AND don't check out PR head). Users who keep `pull_request_target` via `ignore` still need to know the checkout is unsafe. The `unprotected-secrets` silencing under `pull_request_target` is different — there, the diagnostic is genuinely redundant because the trigger itself already grants secret access.
