## 1. Confirm the user-scope overlay serves the command

- [x] 1.1 Verify `~/.claude/commands/opsx/review.md` exists; if absent, stop and delete nothing
- [x] 1.2 Verify `~/.claude/skills/openspec-review-proposal/SKILL.md` exists; if absent, stop and delete nothing

## 2. Remove the local copies

- [x] 2.1 Delete `.claude/skills/openspec-review-proposal/` (contains only `SKILL.md`)
- [x] 2.2 Delete `.claude/commands/opsx/review.md`

## 3. Verify

- [x] 3.1 Grep the repo for `openspec-review-proposal`, `opsx/review`, and `.review-passed`; confirm no remaining hit is a live reference
- [x] 3.2 Confirm `/opsx:review` still resolves, served from `~/.claude/`
- [x] 3.3 Run `mise run test` as a no-regression check and confirm it passes
