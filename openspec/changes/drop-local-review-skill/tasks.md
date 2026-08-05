## 1. Confirm the user-scope overlay serves the command

- [ ] 1.1 Verify `~/.claude/commands/opsx/review.md` exists
- [ ] 1.2 Verify `~/.claude/skills/openspec-review-proposal/SKILL.md` exists

## 2. Remove the local copies

- [ ] 2.1 Delete `.claude/skills/openspec-review-proposal/`
- [ ] 2.2 Delete `.claude/commands/opsx/review.md`

## 3. Verify

- [ ] 3.1 Confirm no file left in the repo references the deleted skill or command
- [ ] 3.2 Run `mise run test` and confirm it passes
