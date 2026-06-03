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
- Cross-workflow analysis (reusable-workflow `outputs`, `workflow_call`). Single-workflow scope only. In particular, a job that is `uses:` a reusable workflow exposes outputs defined in *that* file, not in this one — so `needs.<that-job>.outputs.*` key-checking is intentionally skipped (see the key-existence decision below).
- **`steps.<id>.outputs.<key>` key-existence — out of scope by design, not deferred.** What a step outputs is not in this file: for `uses:` steps it is in the external action's `action.yml`; for `run:` steps it is a shell side-effect (`echo … >> $GITHUB_OUTPUT`). Resolving it requires bundling and continually refreshing a fetched-from-GitHub action-metadata database (how `actionlint` does it) and still degrades to accept-any-key on unknown actions — a permanent maintenance cost for a check that silently does nothing on the long tail. We resolve step *id* existence and stop. A future change should not treat this as low-hanging fruit; the cost/value is structurally unfavorable.

## Decisions

### Decision: Two rules, not one

`dangling-reference` (job-graph integrity) and `invalid-expression` (context-reference resolution) are separate user-facing concerns with separate diagnostics and separate `ignore` scopes, even though both lean on the same domain extension. Splitting matches the security family's one-rule-per-file granularity and lets a user set one to `off` without the other.

### Decision: Conservative expression matching

`invalid-expression` scans each step/job string field (`if_cond`, `with`, `env`, `run`) for `${{ ... }}` spans and within them looks only for two anchored patterns:

- `needs.<id>.…` — `<id>` must be a bare identifier (`[A-Za-z_][A-Za-z0-9_-]*`). If `<id>` is not in the enclosing job's `needs:` set → diagnostic.
- `needs.<id>.outputs.<key>` — when `<id>` *is* a declared, in-`needs:` job AND that job declares a non-empty inline `outputs:` map AND `<key>` is a bare identifier not present in that map → diagnostic (see the key-existence decision). The bare `<id>`-resolves check above still runs first; this is the second, tighter layer.
- `steps.<id>.…` — `<id>` bare identifier. If no earlier step in the same job declares `id: <id>` → diagnostic. The step *output key* (`steps.<id>.outputs.<key>`) is never resolved — out of scope by design.

The rule MUST NOT flag:

- a reference whose segment is itself dynamic (`needs[matrix.x]`, `steps[format(...)]`) — only bare-identifier dotted access is resolved;
- `env.*`, `vars.*`, `matrix.*`, `inputs.*`, `github.*`, `secrets.*`, `runner.*`, `job.*` — out of scope contexts, never flagged;
- a `needs.<id>` when the enclosing job has no `needs:` at all AND `<id>` matches a job that exists (defensive: if we can't be sure, don't flag). Decision: when the enclosing job declares no `needs:`, any `needs.*` reference is unresolvable by definition → flag it (this is a real bug: you can't read `needs` you didn't declare). Documented in the spec scenario.

This keeps false positives near zero: every flagged reference is one the rule fully resolved and found broken.

### Decision: `needs:` accepts scalar or sequence

`needs: build` (scalar) and `needs: [build, test]` (sequence) are both valid. The wire deserializer normalizes both into `Vec<String>`. An absent `needs:` is an empty vec. A custom `Deserialize` (or `deserialize_with`) handles the scalar-or-seq union, mirroring how `JobSecrets` already custom-derives.

### Decision: `Step.id` is `Option<String>`

Most steps have no `id`. Steps without one simply can't be the target of a `steps.<id>` reference. The `invalid-expression` resolver builds a per-job set of declared ids as it walks steps in order, so a reference to a *later* step's id is also flagged (you can't read an output before the step runs).

### Decision: validate `needs.*.outputs.<key>`, never `steps.*.outputs.<key>`

The two output-key checks look symmetric but are not, and the difference is exactly the long-term-maintainability line:

- **`needs.<job>.outputs.<key>` — in scope.** The producing job's `outputs:` map is *in the same workflow file* and already modeled as `Job.outputs`. Checking `<key>` against it costs no new I/O, no external data, and cannot go stale. It catches the real failure the motivating maintainer hits: a cross-job `outputs.<typo>` that silently resolves to empty at run time.
- **`steps.<id>.outputs.<key>` — out of scope by design.** A step's outputs are *not* in this file (external `action.yml`, or a `run:` shell side-effect). The only way to check them is to bundle and perpetually refresh a fetched-from-GitHub action-metadata database, and even then degrade to accept-any on unknown actions. That is a standing maintenance burden with a silent-no-op tail. We decline it. See Non-Goals.

**The zero-false-positive guard for the in-scope half:** key-existence is checked only when the producing job declares a **non-empty inline `outputs:` map**. A reusable-workflow job (`uses: ./…`) has an empty inline map but real outputs from the called file — so an empty `outputs:` means "fall back to job-existence-only, do not flag the key." This preserves the rule invariant: every flag is a reference the rule *fully* resolved and found broken.

## Risks / Trade-offs

- **[Risk] Regex/text scan misses a reference shape** → Mitigation: conservative matching means a missed shape is a false *negative* (rule stays silent), never a false positive. Acceptable: the rule's value is catching the common case, not exhaustiveness.
- **[Risk] Domain extension touches the shared parse used by security rules** → Mitigation: additive fields with `#[serde(default)]`; existing rules ignore them. Covered by the existing security-rule test suite plus new deserialization tests for `needs:` scalar/seq.
- **[Risk] `needs.*.outputs.<key>` false positive on reusable-workflow jobs** → Mitigation: key-checking runs only when the producing job has a non-empty inline `outputs:` map; an empty map (the reusable-workflow case) falls back to job-existence-only. Covered by a scenario.
- **[Trade-off] Step output keys unchecked** → Accepted permanently, not deferred: the data lives outside the file and the only implementations require a continually-refreshed external metadata DB that still no-ops on unknown actions. Job-output keys (in-file) give most of the value at none of that cost.

## Migration Plan

- Additive. New rules default to `error`; users who upgrade and have pre-existing dangling refs (rare — they'd already be failing at dispatch) can set `level = "off"`. CHANGELOG notes the new rules.
- No data migration; no config migration (existing `[lint.rules]` syntax covers it).

## Open Questions

- None blocking. The scope split on output keys (`needs.*` in, `steps.*` out) is a deliberate, documented decision above — not an open question.
