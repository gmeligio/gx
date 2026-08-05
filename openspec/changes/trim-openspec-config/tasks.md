## 1. Confirm the rules are owned elsewhere

- [ ] 1.1 Confirm the spec-driven schema mandates `#### Scenario:` with WHEN/THEN and no GIVEN; if it does not, stop and keep the line
- [ ] 1.2 Confirm the review skill's prompt carries the six CRITICAL/WARNING checks; if it does not, stop and keep them

## 2. Trim rules.specs

- [ ] 2.1 Remove the six `CRITICAL:`/`WARNING:` lines
- [ ] 2.2 Remove the GIVEN/WHEN/THEN line

## 3. Restructure the gate and add missing rules

- [ ] 3.1 Move the relevance gate from `context:` into `rules.proposal` as quoted gate/skip items
- [ ] 3.2 Add "When archiving, update the spec to match what actually shipped"
- [ ] 3.3 Add "When a rule already lives upstream, point to it instead of restating it"

## 4. Verify

- [ ] 4.1 Confirm the `context` paragraph, persona wording, error-classification rule, and `rules.design` block are unchanged
- [ ] 4.2 Confirm the file parses as YAML and `openspec` reads it back with the expected `rules.proposal`
- [ ] 4.3 Run `mise run test` as a no-regression check and confirm it passes
