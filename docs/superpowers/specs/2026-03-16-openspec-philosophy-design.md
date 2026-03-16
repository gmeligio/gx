# OpenSpec Philosophy: Spec Roots and Consolidation

## Problem

The current openspec system has 15 specs that evolved organically. They lack a
clear definition of what a spec *is* — some describe user-facing behavior, others
describe implementation patterns, and the boundary is fuzzy. The reviewer skill
checks cross-artifact consistency but doesn't enforce a coherent philosophy about
what belongs in a spec.

## Goals

1. Define a rooted philosophy for what a spec is, anchored in user value
2. Establish a process for extracting user value from existing specs and creating
   new specs that replace them
3. Update the reviewer skill to enforce the philosophy and cross-reference living
   specs

## Spec Philosophy

### Two Tiers

**Tier 1 — User Value (required)**

Every spec must trace back to something a user cares about: a command, a
behavior, an output, a guarantee. The spec answers: "What does the system do for
the user, and under what conditions?"

Format: GIVEN/WHEN/THEN scenarios are the primary expression of user value.

**The test:** If you can't name the user who benefits and describe what they'd
notice, it doesn't belong in a spec.

**Tier 2 — Architectural Guardrails (optional)**

A spec may include constraints on how the value is delivered, but only when those
constraints are load-bearing — meaning violating them would degrade the user
value.

These are not implementation details. They're boundaries that future changes must
respect.

**Inclusion test:** Violating this guardrail would degrade the user value.

Example: "Errors must be classified as recoverable or strict" is a guardrail
because the classification determines whether the user sees a warning or a hard
failure.

## Extraction Process

The process for deriving new specs from the existing 15:

1. **Read each existing spec** and identify the user-facing value it describes
   (commands, behaviors, outputs, guarantees)
2. **Group by user capability** — multiple old specs may map to the same user
   value (e.g., `resolution`, `version-resolution`, and `upgrade-scope` all
   serve the user's question "what version do I get?")
3. **Write new specs** rooted in the user value, pulling in relevant
   GIVEN/WHEN/THEN scenarios from the old specs. Architectural guardrails are
   included only if they pass the load-bearing test. The number of new specs
   depends on grouping — expect many-to-one consolidation where old specs that
   serve the same user capability merge into one new spec.
4. **Review new specs** using the updated reviewer skill (with the new
   philosophy checks in place). Each new spec must pass the user-value test.
5. **Delete old specs** once the new ones pass review
6. **Leave archived changes alone** — they are historical snapshots, not living
   documents

### Ordering

The extraction is a batch operation: all new specs are written, reviewed, and
accepted before old specs are deleted. This avoids a mixed state where the
reviewer's cross-reference corpus is partly old and partly new. The config.yaml
and reviewer skill updates must land first, so the new philosophy is enforced
during extraction.

## Config.yaml Structure

The current `config.yaml` has top-level keys `schema`, `context`, and `rules`
(with `proposal` and `specs` sub-keys). The new structure:

- **`schema`** — preserved as-is (value: `spec-driven`)
- **`context`** — preserved as-is (project description)
- **`rules`** — replaced entirely by three new top-level keys:
  - `spec_definition` replaces `rules.specs` (altitude rule)
  - `relevance_gate` replaces `rules.proposal` (relevance gate bullets)
  - `review_criteria` is new (currently implicit in the reviewer skill)

Full target state:

```yaml
spec_definition:
  user_value:
    description: "Every spec must trace to something a user cares about"
    test: "Name the user who benefits and describe what they'd notice"
    format: "GIVEN/WHEN/THEN scenarios"
  architectural_guardrails:
    description: "Constraints on how value is delivered"
    inclusion_test: "Violating this guardrail would degrade the user value"
    examples:
      - "Error classification determines whether user sees warning or hard failure"

relevance_gate:
  requires_spec:
    - "Adds, removes, or changes user-facing behavior"
    - "Introduces new domain concept that changes what users can do"
  skip_spec:
    - "Internal refactoring with no user-visible change"
    - "CI/tooling, dependency updates, packaging chores"
    - "Bug fix with obvious solution"
    - "Would duplicate an existing spec"

review_criteria:
  critical:
    - "Spec has no traceable user value"
    - "Spec describes implementation without user-facing connection"
    - "Change contradicts or duplicates existing spec's user value"
  warning:
    - "Guardrail not justified as load-bearing"
    - "Missing GIVEN/WHEN/THEN for claimed behaviors"
    - "Scope too broad — multiple unrelated user capabilities"
```

The `altitude` rule is replaced by the tier 1/tier 2 structure, which is more
precise.

## Reviewer Skill Update

### Relationship to existing checks

The current reviewer has its own CRITICAL/WARNING/SUGGESTION checks for
cross-artifact consistency (e.g., "task without requirement", "delta contradicts
main spec", "design exceeds proposal scope"). These existing checks are
**retained** — they cover artifact-level consistency which is orthogonal to the
philosophy checks. The new checks below are **added** alongside them.

### New CRITICAL checks

- Spec has no traceable user value (fails "name the user who benefits" test)
- Spec describes implementation without connecting it to user-facing behavior
- Change contradicts or silently duplicates an existing spec's user value

### New WARNING checks

- Architectural guardrail included but not justified as load-bearing
- GIVEN/WHEN/THEN scenarios missing for claimed user behaviors
- Spec scope too broad — covers multiple unrelated user capabilities

### Updated reviewer process

1. Reviewer reads `config.yaml` to get the philosophy (tier 1/tier 2
   definitions, the test)
2. Reviewer reads existing specs as the cross-reference corpus
3. Reviewer applies both philosophical checks and cross-reference checks to the
   proposal

The `propose` skill also reads `config.yaml` when creating specs, so it produces
conforming specs from the start — the reviewer is a safety net, not the primary
enforcement.

## Non-Goals

- Auditing individual specs during this design (that happens during extraction)
- Changing the openspec CLI tool or artifact mechanics
- Modifying archived changes
