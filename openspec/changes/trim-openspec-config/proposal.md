## Why

`openspec/config.yaml` carries rules that are enforced somewhere else, so editing
them has no effect — a developer who tightens one of those lines gets nothing.

Two separate cases:

- The six `CRITICAL:`/`WARNING:` lines in `rules.specs` are also pasted into the
  review skill's subagent prompt. The prompt copy is what actually runs, so the
  config copy is inert.
- `rules.specs` requires "Use GIVEN/WHEN/THEN format for behavioral scenarios",
  while the spec-driven schema this project resolves mandates
  `#### Scenario: <name>` with **WHEN/THEN** and no GIVEN. Two sources of truth
  that disagree, and the schema is the one that validates.

Separately, the relevance gate sits in the `context:` free text, where it reads as
background rather than as the rule it is.

## What Changes

- Drop the six `CRITICAL:`/`WARNING:` lines from `rules.specs`.
- Drop the GIVEN/WHEN/THEN line from `rules.specs`.
- Move the relevance gate out of `context:` into `rules.proposal` as quoted
  gate/skip items.
- Add two rules the config lacks. "When archiving, update the spec to match what
  actually shipped" closes a gap nothing else covers — no schema or skill checks
  that an archived spec still describes the code. "When a rule already lives
  upstream, point to it instead of restating it" is the rule whose absence
  produced the two defects above; writing it down is what stops them recurring.
- Scope the `rules.design` "must be present" clause to designs that exist. As
  written it demands sections unconditionally while the schema makes `design.md`
  itself conditional — the same restate-and-contradict defect this change removes.
- Keep unchanged: the `context` paragraph, the persona wording, the
  error-classification rule, and both `rules.design` section requirements.

## Capabilities

### New Capabilities

None. This edits the project's own planning config; it changes nothing a `gx` user
can do.

### Modified Capabilities

None.

Per the relevance gate itself, this is a tooling chore with no user-visible change,
so the change declares `skip_specs: true`.

## Impact

- `openspec/config.yaml` is the only file touched.
- No Rust code, no CLI behavior, no build or CI configuration is touched.
- Affects how future changes are planned and reviewed, not how any existing spec
  is interpreted.
- Severity for the dropped checks now lives solely in the review skill at user
  scope, which this project does not version. That is the point — one owner
  instead of two — but it does mean a future edit to those checks happens
  outside this repo.
