## Context

`gx lint` runs a set of `Rule` impls over a `Context` that carries, among other things, `workflows_full: &[ParsedWorkflow]` — the structural parse introduced by `add-workflow-security-rules`. Each rule returns `Vec<Diagnostic>`; the orchestrator applies configured severity and `ignore` filtering. Rule identity is the `RuleName` enum (kebab-case via `Display`/`FromStr`).

The parse model in `domain::workflow_parsed` is deliberately partial — `Parsed { on, permissions, concurrency, jobs }`, `Job { id, permissions, if_cond, steps, secrets }`, `Step { uses, if_cond, with, env, run }`. It captures exactly what the security rules read and nothing else. The two validity rules need three fields it does not yet model: `Job.needs`, `Job.outputs`, and `Step.id`.

## Goals / Non-Goals

**Goals:**

- Catch dangling `needs:` and unresolved `needs.*` / `steps.*` expression references statically, in the tool users already run.
- Extend the existing parse model minimally and reuse the single parse pass — no second YAML read.
- Match the established rule idiom exactly (one file per rule, `RuleName` variant, default level, `[lint.rules]` config, workflow/job-scoped `ignore`).

**Non-Goals:**

- A general `${{ }}` expression parser. These rules do targeted reference resolution, not grammar validation.
- Cross-workflow analysis (reusable-workflow `outputs`, `workflow_call`). Single-workflow scope only.
- Validating that a referenced output *value* exists in the producing job's `outputs:` map — modeled data is captured (`Job.outputs`) but the first cut only checks job/step *existence*, not output-key existence. A follow-up can tighten to key-level.

## Decisions

### Decision: Two rules, not one

`dangling-reference` (job-graph integrity) and `invalid-expression` (context-reference resolution) are separate user-facing concerns with separate diagnostics and separate `ignore` scopes, even though both lean on the same domain extension. Splitting matches the security family's one-rule-per-file granularity and lets a user set one to `off` without the other.

### Decision: Conservative expression matching

`invalid-expression` scans each step/job string field (`if_cond`, `with`, `env`, `run`) for `${{ ... }}` spans and within them looks only for two anchored patterns:

- `needs.<id>.…` — `<id>` must be a bare identifier (`[A-Za-z_][A-Za-z0-9_-]*`). If `<id>` is not in the enclosing job's `needs:` set → diagnostic.
- `steps.<id>.…` — `<id>` bare identifier. If no earlier step in the same job declares `id: <id>` → diagnostic.

The rule MUST NOT flag:

- a reference whose segment is itself dynamic (`needs[matrix.x]`, `steps[format(...)]`) — only bare-identifier dotted access is resolved;
- `env.*`, `vars.*`, `matrix.*`, `inputs.*`, `github.*`, `secrets.*`, `runner.*`, `job.*` — out of scope contexts, never flagged;
- a `needs.<id>` when the enclosing job has no `needs:` at all AND `<id>` matches a job that exists (defensive: if we can't be sure, don't flag). Decision: when the enclosing job declares no `needs:`, any `needs.*` reference is unresolvable by definition → flag it (this is a real bug: you can't read `needs` you didn't declare). Documented in the spec scenario.

This keeps false positives near zero: every flagged reference is one the rule fully resolved and found broken.

### Decision: `needs:` accepts scalar or sequence

`needs: build` (scalar) and `needs: [build, test]` (sequence) are both valid. The wire deserializer normalizes both into `Vec<String>`. An absent `needs:` is an empty vec. A custom `Deserialize` (or `deserialize_with`) handles the scalar-or-seq union, mirroring how `JobSecrets` already custom-derives.

### Decision: `Step.id` is `Option<String>`

Most steps have no `id`. Steps without one simply can't be the target of a `steps.<id>` reference. The `invalid-expression` resolver builds a per-job set of declared ids as it walks steps in order, so a reference to a *later* step's id is also flagged (you can't read an output before the step runs).

## Risks / Trade-offs

- **[Risk] Regex/text scan misses a reference shape** → Mitigation: conservative matching means a missed shape is a false *negative* (rule stays silent), never a false positive. Acceptable: the rule's value is catching the common case, not exhaustiveness.
- **[Risk] Domain extension touches the shared parse used by security rules** → Mitigation: additive fields with `#[serde(default)]`; existing rules ignore them. Covered by the existing security-rule test suite plus new deserialization tests for `needs:` scalar/seq.
- **[Trade-off] Not validating output-key existence in the first cut** → Acceptable: job/step *existence* is the high-value, zero-false-positive check; key-level resolution is a documented follow-up.

## Migration Plan

- Additive. New rules default to `error`; users who upgrade and have pre-existing dangling refs (rare — they'd already be failing at dispatch) can set `level = "off"`. CHANGELOG notes the new rules.
- No data migration; no config migration (existing `[lint.rules]` syntax covers it).

## Open Questions

- None blocking. Output-key-level resolution (vs. job/step existence) is a deliberate follow-up, noted in Non-Goals.
