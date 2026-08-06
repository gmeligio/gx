## 1. Ship the reference workflow

- [x] 1.1 Create `docs/gx-upgrade.yml`: weekly `schedule` + `workflow_dispatch`, `permissions: contents: write` and `pull-requests: write`, checkout, install gx via Homebrew, run `gx upgrade --json` to a file with `GITHUB_TOKEN` set.
- [x] 1.2 Add the `jq` step that renders `upgrades[]` into a Markdown body file — `action: from → to`, linked to `compare` when the key is present, plain when absent — and emits `up_to_date` as a step output.
- [x] 1.3 Add the `peter-evans/create-pull-request` step gated on `up_to_date == 'false'`, passing the rendered body via `body-path`.
- [x] 1.4 SHA-pin every action referenced in the template with a trailing `# vX.Y.Z` version comment.
- [x] 1.5 Add a brief comment in the template noting that the body omits `in_range` because the default `gx upgrade` is safe mode — a user who switches to `--latest` loses that signal.

## 2. Verify the template against the real contract

- [x] 2.1 Parse `docs/gx-upgrade.yml` as YAML to confirm it is syntactically valid.
- [x] 2.2 Run the `jq` program against fixture JSON for every contract shape: multiple upgrades in one run (the normal weekly case — checks row joining), an upgrade with `compare`, an upgrade without `compare`, and `up_to_date: true`. Confirm the rendered Markdown and the `up_to_date` output are correct in each.
- [x] 2.3 Confirm every field the template reads (`upgrades[].action`, `.from`, `.to`, `.compare`, `up_to_date`) matches `src/upgrade/report.rs`.

## 3. Documentation and discovery

- [x] 3.1 Write `docs/upgrade-workflow.md`: what the template does, how to install it, prerequisites (including that the repo must already have a manifest and lock from `gx init`), and the `GITHUB_TOKEN`-does-not-trigger-CI caveat with the PAT/App fix.
- [x] 3.2 Update `docs/renovate.md` to link the shipped template instead of issue #121.
- [x] 3.3 Add a README pointer to `docs/upgrade-workflow.md`.

## 4. Gate

- [ ] 4.1 Run `mise run test` and confirm it passes (no Rust change; expected unaffected).
- [ ] 4.2 Run `git diff --stat main` and confirm the changed-file list contains no path under `src/` or `tests/` — this change is docs-only, so any such path means scope leaked.
