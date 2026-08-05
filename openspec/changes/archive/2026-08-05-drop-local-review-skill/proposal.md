## Why

This project carries its own copy of the `openspec-review-proposal` skill and the
`/opsx:review` command, both of which are already provided at user scope in
`~/.claude/`. Those user-scope paths are maintained outside this project and were
never generated from these local files, so removing the local copies orphans
nothing.

Two copies means two behaviors, and the local one has drifted: it reads config
keys that do not exist, restates philosophy checks that also live in
`openspec/config.yaml` (making the config copy inert), and writes a `.review-passed`
marker that nothing in this repo reads. Deleting beats repairing, because the
user-scope copy already supersedes it and fixing the local one would only
recreate the divergence.

## What Changes

- Delete `.claude/skills/openspec-review-proposal/`.
- Delete `.claude/commands/opsx/review.md`.
- `/opsx:review` continues to resolve, served by the user-scope overlay in `~/.claude/`.

## Capabilities

### New Capabilities

None. This removes a duplicated tooling artifact; it changes nothing a `gx` user
can do.

### Modified Capabilities

None.

Per the relevance gate in `openspec/config.yaml`, this is a tooling chore with no
user-visible change, so the change declares `skip_specs: true`.

## Impact

- `.claude/skills/openspec-review-proposal/SKILL.md` (deleted)
- `.claude/commands/opsx/review.md` (deleted)
- No Rust code, no CLI behavior, no build or CI configuration is touched.
