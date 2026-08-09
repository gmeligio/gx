## Why

`gx upgrade --json` ships the machine-readable contract a scheduled PR-opening job needs, but every user
who wants scheduled in-range lock advancement still has to invent the workflow that consumes it —
including the `jq` that turns `upgrades` into a readable PR body. `docs/renovate.md` already tells readers
that in-range advancement is `gx upgrade`'s job and then points at [gx#121](https://github.com/gmeligio/gx/issues/121)
for the workflow, so the documented path currently dead-ends at an open issue.

## What Changes

- Add a copy-paste reference workflow, `docs/gx-upgrade.yml`, that runs `gx upgrade --json` on a weekly
  schedule (plus `workflow_dispatch`), renders the `upgrades` array into a Markdown PR body with `jq`, and
  opens/updates a PR via `peter-evans/create-pull-request` — skipping the PR entirely when `up_to_date` is
  true.
- Add `docs/upgrade-workflow.md` explaining how to install the template, what each moving part does, and
  which prerequisites (permissions, `GITHUB_TOKEN`, gx installation) it assumes.
- Update `docs/renovate.md` so the "run `gx upgrade` on a schedule" half points at the shipped template
  instead of at issue #121.
- Add a README pointer so the template is discoverable from the entry point, not only from the Renovate page.

No Rust source changes. `gx upgrade --json` already emits everything the template consumes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None.

**Spec decision — justified against the relevance gate.** The gate requires a spec when a change "adds,
removes, or changes user-facing behavior" or "introduces a new domain concept that changes what users can
do". This change does neither: it adds no command, flag, config key, or output, and `gx` behaves
identically before and after. What ships is documentation plus a YAML file that lives under `docs/` and is
never executed by this repository — a consumer *of* an already-shipped contract, not a change to it. The
gate's "skip spec" list covers exactly this shape ("CI/tooling … chores"), and writing a spec here would
either restate the `--json` contract that `src/upgrade/report.rs` and its tests already own, or specify a
third-party action's behavior that gx does not control.

Related but deliberately out of scope: `gx upgrade --json` itself has no spec in `openspec/specs/`. The
template makes that contract load-bearing for users, which strengthens the case for specifying it — but
that is a change to the *contract's* documentation surface, not to this template, and belongs to whoever
owns `upgrade-operations`.

## Impact

- New: `docs/gx-upgrade.yml`, `docs/upgrade-workflow.md`.
- Modified: `docs/renovate.md` (replace the issue link with the template link), `README.md` (add pointer).
- Depends on the `gx upgrade --json` field names — `upgrades[].action`, `.from`, `.to`, `.compare`, and
  the top-level `up_to_date` — as defined in `src/upgrade/report.rs`.
- Depends on third-party `peter-evans/create-pull-request`; it is SHA-pinned with a version comment, the
  same convention gx itself recommends and this repo's own workflows follow.
- Not affected: `docs/demo.tape` (no CLI behavior or output changes), `tests/` (no Rust change).
