---
model: opus
name: openspec-review-proposal
description: Review an OpenSpec change proposal for cross-artifact consistency before implementation.
license: MIT
compatibility: Requires openspec CLI.
metadata:
  author: custom
  version: "1.1"
---

Review an OpenSpec change proposal before implementation begins.

**Input**: Optionally specify a change name. If omitted, infer from context.
AskUserQuestion if ambiguous.

---

## Steps

### 1. Resolve the change name

- Use provided name, or `openspec list --json` + infer/auto-select
- AskUserQuestion if ambiguous
- Announce: `Reviewing change: <n>`

### 2. Collect artifacts

```bash
openspec status --change "<n>" --json
```

Collect `outputPath` for each artifact with `status: "done"`. Also collect
existing main specs (`openspec/specs/**/spec.md`).

If no artifacts exist: "No artifacts found for `<n>`. Run /opsx:propose first."

### 3. Dispatch review subagent

Use **Agent tool** (`subagent_type: "general-purpose"`).
**Never pass session history** — pass only artifact content.

Subagent prompt:

```
You are a spec reviewer. Evaluate only the artifacts provided.

First read openspec/config.yaml to load the spec philosophy (spec_definition,
relevance_gate, review_criteria). Apply both the philosophy checks and the
cross-artifact consistency checks below.

## Change: <n>
## Existing main specs
<full content of each openspec/specs/**/spec.md, labelled by path>
## Proposal
<proposal.md content>
## Delta specs
<each openspec/changes/<n>/specs/**/*.md>
## Design
<design.md content, or "NOT PRESENT">
## Tasks
<tasks.md content>

---
CRITICAL (blocks):
- Cross-artifact: task without requirement, delta contradicts main spec
  without marker, design makes requirement impossible, required artifact missing.
- Philosophy: spec has no traceable user value (fails "name the user who
  benefits" test), spec describes implementation without connecting it to
  user-facing behavior, change contradicts or silently duplicates an existing
  spec's user value.

WARNING (fix before apply):
- Cross-artifact: unmarked duplication, unjustified design, vague tasks,
  design exceeds proposal scope.
- Philosophy: architectural guardrail included but not justified as
  load-bearing, GIVEN/WHEN/THEN scenarios missing for claimed user behaviors,
  spec scope too broad — covers multiple unrelated user capabilities.

SUGGESTION: missing edge cases, splittable tasks, unstated alternatives.

Output (exactly):
### Review result: <APPROVED | APPROVED_WITH_WARNINGS | BLOCKED>
### CRITICAL issues
### Warnings
### Suggestions
### Verdict (one sentence)
```

### 4. Act on the result

- **BLOCKED**: List CRITICAL issues. Ask what to fix. Re-run after fixes (max 3 iterations, then surface to human).
- **APPROVED** / **APPROVED_WITH_WARNINGS**: Write marker `echo "reviewed" > "openspec/changes/<n>/.review-passed"`. List warnings. Prompt to apply.

## Guardrails

- Never pass session history to subagent
- Never suppress CRITICAL issues
- Reviews only — never writes application code
- Warnings do not block implementation