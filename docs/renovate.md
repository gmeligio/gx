# gx and Renovate

gx and [Renovate](https://docs.renovatebot.com/) can run on the same repository. This page explains exactly
what that buys you, where the boundary is, and how to set it up honestly — so you don't end up believing your
actions are current when only half of the updates are actually landing.

## The two-layer model

gx splits an action version into two files, the same way every mature package manager does:

| File | Holds | Analogue |
|---|---|---|
| `.github/gx.toml` (manifest) | the **range** you allow, e.g. `actions/checkout = "^6"` | `package.json`, `pyproject.toml`, pnpm `catalog:` |
| `.github/gx.lock` (lock) | the **resolved version + commit SHA**, e.g. `v6.0.2` at a 40-char SHA | `package-lock.json`, `uv.lock`, `pnpm-lock.yaml` |

An update can move either layer:

- **In-range (minor/patch)** — a new tag still satisfies the range (`^6` already permits `6.0.2` *and*
  `6.0.3`). Only the **lock** needs to advance; the range in `gx.toml` does not change.
- **Out-of-range (major)** — a new tag falls outside the range (`^6` does not permit `7.0.0`). The **range**
  in `gx.toml` must be edited, and then the lock re-resolves.

## What Renovate can and cannot do with gx

Renovate has no built-in manager for gx, so you attach it with a
[custom regex manager](https://docs.renovatebot.com/modules/manager/regex/) pointed at `gx.toml`. That
manager reads **only the range in `gx.toml`**. It never reads `gx.lock`.

That single fact sets the boundary:

- ✅ **Majors** — Renovate sees `7.0.0` is outside `^6`, edits the range in `gx.toml`, and opens a PR. This
  works.
- ❌ **In-range minor/patch** — Renovate **cannot advance `gx.lock`**. When the lock is pinned at `v6.0.2`
  and `v6.0.3` ships, the range `^6` already permits both, so from Renovate's point of view **nothing needs
  to change** — and it never looks at the lock to notice the drift.

This is not a `rangeStrategy` you can flip. Renovate's `update-lockfile` strategy only advances a lock when
the manager extracts a **`lockedVersion`**, and a custom regex manager captures only `currentValue` (the
range) — it has no `lockedVersion` (see the strategy matrix in
[renovate#19802](https://github.com/renovatebot/renovate/issues/19802)). For a range like `^6`, `replace`,
`bump`, and `update-lockfile` are all no-ops on an in-range update.

Nor can you close the gap with a post-upgrade hook on the hosted app.
[`postUpgradeTasks`](https://docs.renovatebot.com/configuration-options/#postupgradetasks) *could* run
`gx tidy` to regenerate the lock, but its commands are gated by
[`allowedCommands`](https://docs.renovatebot.com/self-hosted-configuration/#allowedcommands), which is
**self-hosted only** — it is unavailable on the hosted Mend app. So no `renovate.json` gives hosted-Renovate
users turnkey in-range lock advancement.

## The division of labor

Because Renovate cannot advance the lock in-range, gx keeps that job for itself:

> **Renovate catches majors. In-range advancement is `gx upgrade`'s job.**

`gx upgrade` (safe mode — the default) is gx's local equivalent of npm's `update-lockfile` and uv's
`lockFileMaintenance`. It reads the resolved version out of `gx.lock`, treats it as a floor, finds the
highest tag that still satisfies the range, and advances **only the lock** (plus the workflow SHAs) — leaving
the `gx.toml` range untouched:

```console
$ gx upgrade
 ↑ actions/checkout               ^6 → v6.0.3
1 upgraded · 1 workflow
```

`gx upgrade --latest` additionally crosses majors, editing the range — the same territory Renovate covers, so
you typically leave majors to whichever tool you prefer rather than running both.

## Recommended coexistence setup

Two pieces, each doing only what it can do well.

**1. Renovate for majors.** A custom manager over `gx.toml` so out-of-range bumps arrive as PRs:

```json
{
  "$schema": "https://docs.renovatebot.com/renovate-schema.json",
  "customManagers": [
    {
      "customType": "regex",
      "description": "Bump out-of-range (major) GitHub Actions in gx.toml",
      "managerFilePatterns": ["/^\\.github/gx\\.toml$/"],
      "matchStrings": ["\"(?<depName>[^\"]+?)\"\\s*=\\s*\"(?<currentValue>[^\"]+?)\""],
      "datasourceTemplate": "github-tags"
    }
  ]
}
```

Validate it before committing:

```console
npx --package renovate -- renovate-config-validator --strict
```

> **This half only catches majors.** A manifest-only Renovate config does **not** keep `gx.lock` current for
> in-range minor/patch updates — see the boundary above. Do not treat a green Renovate dashboard as "actions
> are up to date."

**2. `gx upgrade` for in-range.** Run `gx upgrade` on a schedule or in CI and open a PR with the result. Two
features make this a turnkey job rather than a text-scraping exercise:

- **`gx upgrade --json`** emits a stable, machine-readable document on stdout — one entry per action with the
  **old and new resolved versions** (`from`/`to`, not the range), whether the bump stayed `in_range`, and a
  `compare` link. Build a PR body straight from it, no diff re-parsing:

  ```console
  $ gx upgrade --json
  {
    "upgrades": [
      {
        "action": "actions/checkout",
        "from": "v6.0.1",
        "to": "v6.0.3",
        "in_range": true,
        "compare": "https://github.com/actions/checkout/compare/v6.0.1...v6.0.3"
      }
    ],
    "workflows_updated": 1,
    "up_to_date": false
  }
  ```

- The same **compare link** is written to the run's log file (and shown in CI verbose output), so anyone can
  see *why* each pin moved without it cluttering the terminal summary.

gx ships that job as a copy-paste reference workflow — [`docs/gx-upgrade.yml`](gx-upgrade.yml) — which runs
`gx upgrade --json` on a schedule, turns the `upgrades` array into a Markdown body with `jq`, and hands it to
[`peter-evans/create-pull-request`](https://github.com/peter-evans/create-pull-request), skipping the PR when
`up_to_date` is true. See [upgrade-workflow.md](upgrade-workflow.md) to set it up.

## Why gx does not ship `gx renovate init`

A generator that emits the `renovate.json` above was considered and rejected. It would not have earned its
maintenance cost:

- It **cannot solve the in-range problem** — whatever it generates is still a manifest-only custom manager,
  blind to the lock, for the structural reasons above.
- It would **couple gx to Renovate's configuration schema**, which actively drifts (the `rangeStrategy`
  refactor in [renovate#19802](https://github.com/renovatebot/renovate/issues/19802) is one example), turning
  every upstream schema change into a gx maintenance chore.

The only way to get true in-range, lock-advancing PRs *driven by Renovate* is a **native gx manager inside
Renovate** — one that detects a gx project by `gx.lock`, reads the resolved version, and invokes gx to
regenerate the lock, exactly as Renovate's native uv and pnpm managers do. That is the real endpoint, tracked
in [gx#111](https://github.com/gmeligio/gx/issues/111). Until it exists, the two-piece setup above is the
honest arrangement.
