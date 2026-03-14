---
name: openspec-review-proposal
description: >
  Automatically review an OpenSpec change proposal before implementation begins.
  Dispatches an isolated subagent to check semantic consistency across all
  artifacts (proposal, specs, design, tasks). Triggered automatically by
  openspec-propose after artifact generation and by openspec-apply-change before
  implementation starts. Also callable directly when the user asks to review a
  proposal.
license: MIT
compatibility: Requires openspec CLI.
metadata:
  author: custom
  version: "1.0"
---

Review an OpenSpec change proposal by dispatching an isolated subagent that
checks cross-artifact consistency. Must run BEFORE any implementation.

**Input**: Optionally specify a change name. If omitted, infer from context.
Use AskUserQuestion if ambiguous. Must not proceed without knowing the change.

---

## Steps

### 1. Resolve the change name

If provided, use it. Otherwise:
- `openspec list --json` + infer from context
- Auto-select if exactly one active change exists
- AskUserQuestion if ambiguous

Announce: `Reviewing change: <n>`

### 2. Collect artifact paths

```bash
openspec status --change "<n>" --json
```

Collect `outputPath` for each artifact with `status: "done"`. Also collect:
```bash
find openspec/specs -name "spec.md" 2>/dev/null
```

### 3. Dispatch the review subagent

Use the **Task tool** with `subagent_type: "general-purpose"`.

**CRITICAL**: Never pass session history. Pass only artifact content.

Subagent prompt template:
```
You are a spec reviewer for a software change proposal. Evaluate only the
artifacts provided. No conversation history is available to you.

## Change: <n>

## Existing main specs (current system state)
<paste full content of each openspec/specs/**/spec.md, labelled by path>

## Proposal
<paste full content of proposal.md>

## Delta specs (requirements being changed)
<paste full content of each openspec/changes/<n>/specs/**/*.md>

## Design
<paste full content of design.md, or "NOT PRESENT" if absent>

## Tasks
<paste full content of tasks.md>

---

CRITICAL (blocks implementation):
- A task has no corresponding requirement in proposal or delta specs
- A delta spec requirement contradicts an existing main spec without explicit
  MODIFIED or REMOVED markers
- The design describes an approach that makes a stated requirement impossible
- A required artifact is missing (proposal or tasks must be present)

WARNING (should fix before apply):
- Semantic duplication with existing specs not marked MODIFIED
- Design decisions not justified in proposal or specs
- Tasks vague enough that an implementer would need to guess
- Missing GIVEN/WHEN/THEN for a non-trivial requirement
- Design scope broader than what the proposal states

SUGGESTION:
- Missing edge-case scenarios
- Tasks that could be split for clearer checkpoints
- Unstated design alternatives worth documenting

Output format (exactly):

### Review result: <APPROVED | APPROVED_WITH_WARNINGS | BLOCKED>

### CRITICAL issues
<list each, or "None">

### Warnings
<list each, or "None">

### Suggestions
<list each, or "None">

### Verdict
One sentence. State whether implementation should proceed, and why.
```

### 4. Display the review result

```
## Proposal review: <change-name>
<full subagent output>
```

### 5. Act on the result

**If BLOCKED:**
- Do NOT proceed to implementation
- List CRITICAL issues clearly
- Ask which issues to fix now
- After fixes, re-run this skill (max 3 auto-iterations, then surface to human)

**If APPROVED or APPROVED_WITH_WARNINGS:**
- Write the marker: `echo "reviewed" > "openspec/changes/<n>/.review-passed"`
- Announce: "Review passed. Ready for implementation."
- List any warnings, ask: "Fix these now, or proceed to apply?"
- Hand off to `openspec-apply-change` if the user confirms

---

## Guardrails

- Never pass session history to the review subagent
- Never suppress CRITICAL issues to keep momentum
- Cap automatic re-review iterations at 3, then surface to human
- If no artifacts exist: fail gracefully — "No artifacts found for `<n>`.
  Run /opsx:propose first."
- This skill reviews only — never writes application code
- Warnings do not block implementation
- Always announce which change is being reviewed before dispatching subagent