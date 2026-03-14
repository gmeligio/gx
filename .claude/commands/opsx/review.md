---
name: "OPSX: Review"
description: Review an OpenSpec change proposal before implementation — checks cross-artifact consistency
category: Workflow
tags: [workflow, review, artifacts]
---

Review an OpenSpec change proposal before implementation begins.

**Input**: Optionally specify a change name (e.g., `/opsx:review add-auth`).
If omitted, infer from context or prompt for selection.

**Steps**

1. **Select the change**

   If a name is provided, use it. Otherwise:
   - Infer from conversation context
   - Auto-select if only one active change exists
   - If ambiguous: `openspec list --json` + **AskUserQuestion**

   Announce: "Reviewing change: <n>"

2. **Collect artifact paths**

   ```bash
   openspec status --change "<n>" --json
   ```
   Collect `outputPath` for each artifact with `status: "done"`.

   Also collect existing main specs:
   ```bash
   find openspec/specs -name "spec.md" 2>/dev/null
   ```

3. **Dispatch the review subagent**

   Use the **Task tool** (`subagent_type: "general-purpose"`).
   **Never pass session history** — pass only artifact content.

   Subagent prompt:
   ```
   You are a spec reviewer. Evaluate only the artifacts provided.
   No conversation history is available to you.

   ## Change: <n>

   ## Existing main specs
   <paste full content of each openspec/specs/**/spec.md>

   ## Proposal
   <paste full content of proposal.md>

   ## Delta specs
   <paste full content of each openspec/changes/<n>/specs/**/*.md>

   ## Design
   <paste design.md content, or "NOT PRESENT">

   ## Tasks
   <paste tasks.md content>

   ---

   CRITICAL (blocks implementation):
   - Task has no corresponding requirement
   - Delta spec contradicts main spec without MODIFIED/REMOVED marker
   - Design makes a stated requirement impossible
   - Required artifact missing

   WARNING (fix before apply):
   - Semantic duplication with existing specs not marked MODIFIED
   - Design decision not justified in proposal or specs
   - Tasks vague enough to require guessing
   - Missing GIVEN/WHEN/THEN for non-trivial requirement
   - Design scope broader than proposal scope

   SUGGESTION:
   - Missing edge-case scenarios
   - Tasks that could be split for clearer checkpoints
   - Unstated design alternatives worth documenting

   Output (exactly this structure):

   ### Review result: <APPROVED | APPROVED_WITH_WARNINGS | BLOCKED>

   ### CRITICAL issues
   <list or "None">

   ### Warnings
   <list or "None">

   ### Suggestions
   <list or "None">

   ### Verdict
   One sentence.
   ```

4. **Display the full review result**

   ```
   ## Proposal review: <change-name>
   <full subagent output>
   ```

5. **Act on the result**

   - **BLOCKED**: Do not proceed. List CRITICAL issues. Ask what to fix.
     Re-run review after fixes (max 3 iterations, then ask human).
   - **APPROVED** / **APPROVED_WITH_WARNINGS**:
     Write marker: `echo "reviewed" > "openspec/changes/<n>/.review-passed"`
     Announce: "Review passed. Ready for implementation."
     List any warnings. Ask: "Fix these now, or proceed to apply?"

**Guardrails**
- Never pass session history to the review subagent
- Never suppress CRITICAL issues
- Cap auto-iterations at 3
- This command only reviews — never writes application code
- Warnings do not block implementation