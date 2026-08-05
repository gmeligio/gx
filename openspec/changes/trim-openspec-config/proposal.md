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
- Add two rules the config lacks: update the spec on archive to match what
  shipped, and point to a rule that already lives upstream instead of restating it.
- Keep unchanged: the `context` paragraph, the persona wording, the
  error-classification rule, and the `rules.design` block.

## Capabilities

### New Capabilities

None. This edits the project's own planning config; it changes nothing a `gx` user
can do.

### Modified Capabilities

None.

Per the relevance gate itself, this is a tooling chore with no user-visible change,
so the change declares `skip_specs: true`.

## Impact

- `openspec/config.yaml` — roughly 37 lines down to about 27, plus the retained
  design block.
- No Rust code, no CLI behavior, no build or CI configuration is touched.
- Affects how future changes are planned and reviewed, not how any existing spec
  is interpreted.
