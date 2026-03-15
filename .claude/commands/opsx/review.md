---
model: opus
name: "OPSX: Review"
description: Review an OpenSpec change proposal for cross-artifact consistency before implementation
category: Workflow
tags: [workflow, review, artifacts]
---

Review an OpenSpec change proposal before implementation begins.

**Input**: Optionally specify a change name (e.g., `/opsx:review add-auth`).
If omitted, infer from context or prompt for selection.

**Steps**

1. **Select the change**

   Use provided name, or `openspec list --json` + infer/auto-select.
   AskUserQuestion if ambiguous. Announce: "Reviewing change: <n>"

2. **Collect artifacts**

   ```bash
   openspec status --change "<n>" --json
   ```
   Collect `outputPath` for each artifact with `status: "done"`.
   Also collect existing main specs (`openspec/specs/**/spec.md`).

3. **Dispatch review subagent**

   Use **Agent tool** (`subagent_type: "general-purpose"`).
   **Never pass session history** — pass only artifact content.

   Subagent prompt:
   ```
   You are a spec reviewer. Evaluate only the artifacts provided.

   ## Change: <n>
   ## Existing main specs
   <each openspec/specs/**/spec.md>
   ## Proposal
   <proposal.md>
   ## Delta specs
   <each openspec/changes/<n>/specs/**/*.md>
   ## Design
   <design.md or "NOT PRESENT">
   ## Tasks
   <tasks.md>

   ---
   CRITICAL (blocks): task without requirement, delta contradicts main spec
   without marker, design makes requirement impossible, required artifact missing.

   WARNING (fix before apply): unmarked duplication, unjustified design,
   vague tasks, missing GIVEN/WHEN/THEN, design exceeds proposal scope.

   SUGGESTION: missing edge cases, splittable tasks, unstated alternatives.

   Output (exactly):
   ### Review result: <APPROVED | APPROVED_WITH_WARNINGS | BLOCKED>
   ### CRITICAL issues
   ### Warnings
   ### Suggestions
   ### Verdict (one sentence)
   ```

4. **Act on result**

   - **BLOCKED**: List CRITICAL issues. Ask what to fix. Re-run after fixes (max 3 iterations).
   - **APPROVED** / **APPROVED_WITH_WARNINGS**: Write marker
     `echo "reviewed" > "openspec/changes/<n>/.review-passed"`.
     List warnings. Prompt: "Fix these now, or proceed to apply?"

**Guardrails**
- Never pass session history to subagent
- Never suppress CRITICAL issues
- Reviews only — never writes application code
- Warnings do not block implementation