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

The `Scanner` runs `serde_yaml::from_str` once per workflow and produces a `Parsed`; it derives the existing `WorkflowAction` list from `Parsed.jobs[].steps[].uses`. This avoids parsing the same file twice.

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

Single-condition check: does `Parsed.on` contain `pull_request_target`? Emit diagnostic listing the file. Users with a documented and reviewed `pull_request_target` workflow (e.g., a comment-handler that operates on PR metadata, never on PR code) opt out via `ignore`.

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
3. For each step that references `secrets.<NAME>` (in `with`, `env`, `run`) — collect the step's effective `if:` (job-level `if:` AND step-level `if:`, concatenated).
4. If the effective `if:` does NOT contain `github.event.pull_request.head.repo.full_name == github.repository` (or the equivalent `github.repository_owner ==` shape) — emit.

This is a textual contains-check on the raw expression text, not a full expression parser. It will miss exotic equivalents (e.g., a separate environment variable set elsewhere). Document this in the rule's help text — gx prefers conservative false-negatives over false-positives for the default-error rules.

False-positive analysis:
- Secrets passed only via `GITHUB_TOKEN` (which auto-scopes down on fork PRs) are still flagged. **This is intentional.** The rule's value is making the gate explicit; `if: ...head.repo.full_name == github.repository` documents intent better than relying on `GITHUB_TOKEN`'s implicit scoping.
- Workflows that intentionally accept secret risk from fork PRs (none should exist, but if they do) opt out via ignore.

## Determinism and ordering

Diagnostics are emitted per-rule, sorted by `(workflow_path, job_id, step_index, rule_name)` for stable output. This matches the existing rule output style.

## Test strategy

Each rule gets:
- A "passes on clean workflow" test
- A "fails on the documented bad case" test
- An "ignore mechanism scopes correctly" test
- A "rule disabled = no diagnostic" test

Plus integration tests that run all six rules against a fixture repo mirroring the flutter-docker-image patterns.

## Alternatives considered and rejected

- **Embed zizmor as a subprocess.** Rejected: contradicts single-binary; doubles install surface.
- **New `gx audit` subcommand instead of expanding `lint`.** Rejected: users want one command to fail CI. Two commands means two CI steps and two exit-code reconciliations.
- **One new capability per rule.** Rejected: would fragment `gx lint --help` and the lint-command spec into seven pages. Users discover rules through one capability.
- **Defer everything to a "policy plugin" interface.** Rejected: speculative. Six built-in rules cover the documented use cases; a plugin interface can be added later if a third party wants to ship rules.
