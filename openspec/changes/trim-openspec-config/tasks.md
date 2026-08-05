## 1. Confirm the rules are owned elsewhere

- [ ] 1.1 Confirm the spec-driven schema mandates `#### Scenario:` with WHEN/THEN and no GIVEN; if it does not, stop and keep the line
- [ ] 1.2 Confirm the review skill's prompt carries the six CRITICAL/WARNING checks, reading it at `~/.claude/skills/openspec-review-proposal/SKILL.md` (it resolves from user scope, not from this repo); if it does not, stop and keep them

## 2. Trim rules.specs

- [ ] 2.1 Remove the six `CRITICAL:`/`WARNING:` lines
- [ ] 2.2 Remove the GIVEN/WHEN/THEN line, replacing it with a pointer to the schema as the authority on scenario format

## 3. Restructure the gate and add missing rules

- [ ] 3.1 Move the relevance gate from `context:` into `rules.proposal` as quoted gate/skip items, leaving the four opening `context` lines in place
- [ ] 3.2 Add "When archiving, update the spec to match what actually shipped"
- [ ] 3.3 Add "When a rule already lives upstream, point to it instead of restating it"
- [ ] 3.4 Scope the `rules.design` "must be present" clause to designs that exist, so it stops contradicting the schema's conditional design artifact

## 4. Verify

- [ ] 4.1 Confirm the four opening `context` lines, the persona wording, the error-classification rule, and the two `rules.design` section requirements are unchanged
- [ ] 4.2 Confirm the file parses as YAML
- [ ] 4.3 Confirm `openspec instructions proposal` reads back `rules.proposal` as exactly: the justify-the-gate rule, then the quoted gate items, then the two added rules
- [ ] 4.4 Run `mise run test` as a no-regression smoke check; it does not read this file, so it cannot detect a fault in this change
