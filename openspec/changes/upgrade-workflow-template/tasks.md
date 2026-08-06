## 1. Ship the reference workflow

- [ ] 1.1 Create `docs/gx-upgrade.yml`: weekly `schedule` + `workflow_dispatch`, `permissions: contents: write` and `pull-requests: write`, checkout, install gx via Homebrew, run `gx upgrade --json` to a file with `GITHUB_TOKEN` set.
- [ ] 1.2 Add the `jq` step that renders `upgrades[]` into a Markdown body file — `action: from → to`, linked to `compare` when the key is present, plain when absent — and emits `up_to_date` as a step output.
- [ ] 1.3 Add the `peter-evans/create-pull-request` step gated on `up_to_date == 'false'`, passing the rendered body via `body-path`.
- [ ] 1.4 SHA-pin every action referenced in the template with a trailing `# vX.Y.Z` version comment.

## 2. Verify the template against the real contract

- [ ] 2.1 Parse `docs/gx-upgrade.yml` as YAML to confirm it is syntactically valid.
- [ ] 2.2 Run the `jq` program against fixture JSON for all three contract shapes: an upgrade with `compare`, an upgrade without `compare`, and `up_to_date: true`. Confirm the rendered Markdown and the `up_to_date` output are correct in each.
- [ ] 2.3 Confirm every field the template reads (`upgrades[].action`, `.from`, `.to`, `.compare`, `up_to_date`) matches `src/upgrade/report.rs`.

## 3. Documentation and discovery

- [ ] 3.1 Write `docs/upgrade-workflow.md`: what the template does, how to install it, prerequisites, and the `GITHUB_TOKEN`-does-not-trigger-CI caveat with the PAT/App fix.
- [ ] 3.2 Update `docs/renovate.md` to link the shipped template instead of issue #121.
- [ ] 3.3 Add a README pointer to `docs/upgrade-workflow.md`.

## 4. Gate

- [ ] 4.1 Run `mise run test` and confirm it passes (no Rust change; expected unaffected).
- [ ] 4.2 Confirm no file under `src/` and no budget in `tests/code_health.rs` was modified.
