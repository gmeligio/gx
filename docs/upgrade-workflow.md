# Scheduled upgrade PRs

gx ships a reference workflow, [`gx-upgrade.yml`](gx-upgrade.yml), that keeps `.github/gx.lock` current on a
schedule and opens a pull request when something moves. Copy it into your repository:

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

## The pull request body

The body is generated from `gx upgrade --json`, never by scraping human output. Each upgraded action becomes
one line linking to its GitHub compare view:

```markdown
- **actions/checkout** [`v6.0.1` → `v6.0.3`](https://github.com/actions/checkout/compare/v6.0.1...v6.0.3)
```

The `compare` field is omitted when either side is not a real version tag (a branch pin, for example), so
those lines render without a link rather than with a broken one.

The raw JSON is echoed into the run log, so the document that produced a PR — or that decided not to open
one — is always recoverable from the Actions run.

## Customizing it

It is a template, not a product — edit the copy in your repo.

- **Schedule.** The `cron` is Monday 06:00 UTC. GitHub cron is always UTC and has no timezone option.
- **Reviewers, labels, PR title.** See the
  [`create-pull-request` inputs](https://github.com/peter-evans/create-pull-request#action-inputs).
- **Crossing majors.** `gx upgrade --latest` also edits ranges in `gx.toml`. If you switch to it, consider
  surfacing the `in_range` field in the body — in safe mode it is always `true`, which is why the template
  leaves it out.

## The `GITHUB_TOKEN` caveat

PRs opened with the default `GITHUB_TOKEN` don't trigger other workflows, so CI won't run on the upgrade PR.
This is [GitHub's deliberate guard against recursive runs](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request).

If you need CI to run on these PRs, authenticate the `create-pull-request` step with a
[GitHub App token](https://github.com/peter-evans/create-pull-request/blob/main/docs/concepts-guidelines.md#authenticating-with-github-app-generated-tokens)
or a personal access token instead. The template stays on `GITHUB_TOKEN` because it is the zero-setup default;
switching is a two-line change.

## Keeping the pins current

The template SHA-pins the actions it uses, with the version in a trailing comment — the same practice gx exists
to enforce. Those pins age. If you use gx on your own repository, `gx tidy` and `gx upgrade` will manage them
for you once the file lives in `.github/workflows/`, exactly like any other workflow.
