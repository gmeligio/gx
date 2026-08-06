# Scheduled upgrade PRs

gx ships a reference workflow, [`gx-upgrade.yml`](gx-upgrade.yml), that keeps `.github/gx.lock` current on a
schedule and opens a pull request when something moves. Copy it into your repository and you are done:

```bash
curl -o .github/workflows/gx-upgrade.yml \
  https://raw.githubusercontent.com/gmeligio/gx/main/docs/gx-upgrade.yml
```

It runs `gx upgrade` in **safe mode**: the lock advances to the newest versions your `gx.toml` ranges already
allow, and the ranges themselves are never edited. That is the half of the update problem Renovate structurally
cannot cover — see [renovate.md](renovate.md) for why.

## Prerequisites

- **A manifest and lock.** The workflow upgrades `.github/gx.toml` + `.github/gx.lock`. If your repo has
  neither, run `gx init` once and commit the result first.
- **Actions may open pull requests.** In *Settings → Actions → General → Workflow permissions*, enable
  "Allow GitHub Actions to create and approve pull requests". Without it the last step fails with a
  permissions error from the API.

## What the workflow does

| Step | Why |
|---|---|
| `gx upgrade --json \| tee upgrade.json` | Advances the lock and emits the machine-readable report. `tee` keeps it in the run log, so you can always see the document that produced the PR. |
| `jq` over `upgrade.json` | Builds the PR body, and reads `up_to_date` into a step output. |
| `peter-evans/create-pull-request` | Opens or updates the PR — skipped entirely when `up_to_date` is `true`. |

The PR body is generated from the JSON contract, never by scraping human output. Each upgraded action becomes
one line linking to its GitHub compare view:

```markdown
- **actions/checkout** [`v6.0.1` → `v6.0.3`](https://github.com/actions/checkout/compare/v6.0.1...v6.0.3)
```

The `compare` field is omitted when either side is not a real version tag (a branch pin, for example), so
those lines render without a link rather than with a broken one.

## Customizing it

It is a template, not a product — edit the copy in your repo.

- **Schedule.** The `cron` is Monday 06:00 UTC. GitHub cron is always UTC and has no timezone option.
- **Reviewers, labels, PR title.** See the
  [`create-pull-request` inputs](https://github.com/peter-evans/create-pull-request#action-inputs).
- **Crossing majors.** `gx upgrade --latest` also edits ranges in `gx.toml`. If you switch to it, consider
  surfacing the `in_range` field in the body — in safe mode it is always `true`, which is why the template
  leaves it out.

## The `GITHUB_TOKEN` caveat

By design, **a PR opened using the default `GITHUB_TOKEN` does not trigger your other workflows** — so your CI
will not run on the upgrade PR. GitHub documents this as a deliberate restriction against recursive workflow
runs ("other `GITHUB_TOKEN`-triggered events do not create workflow runs at all" —
[events that trigger workflows](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request)),
not a bug in the template.

If you need CI to run on these PRs, authenticate the `create-pull-request` step with a
[GitHub App token](https://github.com/peter-evans/create-pull-request/blob/main/docs/concepts-guidelines.md#authenticating-with-github-app-generated-tokens)
or a personal access token instead. The template stays on `GITHUB_TOKEN` because it is the zero-setup default;
switching is a two-line change.

## Keeping the pins current

The template SHA-pins the actions it uses, with the version in a trailing comment — the same practice gx exists
to enforce. Those pins age. If you use gx on your own repository, `gx tidy` and `gx upgrade` will manage them
for you once the file lives in `.github/workflows/`, exactly like any other workflow.
