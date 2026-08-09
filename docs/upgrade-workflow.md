# Scheduled upgrade PRs

gx ships a ready-made workflow, [`gx-upgrade.yml`](gx-upgrade.yml), that checks for newer versions of your
pinned actions on a schedule and opens a pull request when it finds any.

## Setup

1. Make sure your repo has `.github/gx.toml` and `.github/gx.lock`. If it doesn't, run `gx init` once and
   commit the result.
2. Copy the workflow in:

   ```bash
   curl -o .github/workflows/gx-upgrade.yml \
     https://raw.githubusercontent.com/gmeligio/gx/main/docs/gx-upgrade.yml
   ```

3. Let Actions open pull requests: in *Settings → Actions → General → Workflow permissions*, turn on
   "Allow GitHub Actions to create and approve pull requests". Without it, the final step fails with a
   permissions error.

## What it does

Every Monday at 06:00 UTC it runs `gx upgrade`, which updates `gx.lock` to the newest versions your `gx.toml`
ranges already allow. It never widens those ranges, so a new major version won't land on you unasked. That
in-range advancement is the piece Renovate can't do — [renovate.md](renovate.md) explains why.

If nothing moved, the run ends quietly. If something did, you get a pull request.

## The pull request body

The body comes from `gx upgrade --json`, so it stays accurate as gx's console output changes. Each upgraded
action gets one line, linking to the compare view on GitHub:

> - **actions/checkout** [`v6.0.1` → `v6.0.3`](https://github.com/actions/checkout/compare/v6.0.1...v6.0.3)

When one side isn't a real version tag — a branch pin, say — gx leaves the link out, and the line renders as
plain text rather than a dead link.

The full JSON also goes to the run log, so you can always see what a run decided, including runs that opened
no PR.

## CI won't run on the PR

Pull requests opened with the default `GITHUB_TOKEN` don't trigger other workflows, so your usual checks stay
idle. That's [GitHub's guard against workflows triggering themselves in a loop](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request).

To get CI running, authenticate the `create-pull-request` step with a
[GitHub App token](https://github.com/peter-evans/create-pull-request/blob/main/docs/concepts-guidelines.md#authenticating-with-github-app-generated-tokens)
or a personal access token. It's a two-line change. The template ships with `GITHUB_TOKEN` because it needs no
setup at all.

## Changing it

The file is yours once you copy it — edit away.

- **Schedule.** Change the `cron`. GitHub runs it in UTC and offers no timezone setting.
- **Reviewers, labels, title.** See the
  [`create-pull-request` inputs](https://github.com/peter-evans/create-pull-request#action-inputs).
- **Crossing majors.** `gx upgrade --latest` will also widen a range in `gx.toml` when the newest version sits
  outside it. If you switch to it, consider adding each upgrade's `in_range` field to the body: `false` means
  that action's range was rewritten, which is the change worth a closer look. The template leaves the field out
  because plain `gx upgrade` never rewrites a range, so it would always read `true`.

## Keeping the pins current

The workflow SHA-pins the actions it uses, with the version in a trailing comment — the practice gx exists to
enforce. Those pins age like any other. Once the file sits in `.github/workflows/`, `gx tidy` and `gx upgrade`
maintain it alongside your other workflows.
