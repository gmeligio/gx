## Context

`gx upgrade --json` emits a stable document (`src/upgrade/report.rs`) with `upgrades[]` — each carrying
`action`, `from`, `to`, `in_range`, and an optional `compare` URL — plus top-level `workflows_updated` and
`up_to_date`. Everything a scheduled PR job needs is already there; what is missing is the workflow that
consumes it.

The artifact being designed is a **copy-paste file a stranger pastes into their repo once**. That inverts
the usual priority: it is read far more often than it is run, it gets no code review in the user's repo,
and a mistake surfaces as a broken cron job days later. So the design optimizes for correct-on-first-paste
and legible-while-reading, and treats every configuration knob as a maintenance liability (the issue itself
warns the template "will drift with GitHub Actions / third-party action versions").

## Goals / Non-Goals

**Goals:**

- One file a user copies to `.github/workflows/` unmodified and it works.
- The PR body is built from `gx upgrade --json`, never from re-parsing human output.
- No PR is opened when there is nothing to upgrade.
- The template practices gx's own thesis: every action it references is SHA-pinned with a version comment.

**Non-Goals:**

- Configurability (schedule overrides, label/reviewer inputs, `--latest` mode, matrix over directories).
  Nobody asked; each knob is permanent drift surface. The user edits the file — that is what a template is.
- Making the template a reusable workflow or composite action. That would be a maintained gx product
  surface with a version contract; the issue asks for a reference file.
- Running this workflow in the gx repository itself.

## Decisions

### 1. The template lives at `docs/gx-upgrade.yml`, not `.github/workflows/`

**Chosen:** ship it as a non-executing file under `docs/`, with `docs/upgrade-workflow.md` as its prose
companion.

A real file under this repo's `.github/workflows/` would *run here* — opening scheduled PRs against gx
itself, which nobody asked for — and it would be linted by this repo's own `gx lint` and CI expectations,
coupling a user-facing artifact to this repo's internal policy. Under `docs/` it is inert.

**Alternatives considered:**

- *`.github/workflows/gx-upgrade.yml`* — rejected: it executes. Neutering it (`on: workflow_dispatch` only)
  would ship a template whose most important line, the schedule, is wrong in the copy the user takes.
- *Fenced code block inside `docs/upgrade-workflow.md`* — rejected: the user copy-pastes out of a rendered
  code block, which is exactly where indentation errors enter. A real `.yml` file can be downloaded, and
  YAML indentation is preserved verbatim.
- *New top-level `templates/`* — rejected: one file does not earn a directory, and `docs/` is already where
  `renovate.md` and `lint-rules.md` send readers.

Discovery is handled by links rather than location: README → `docs/upgrade-workflow.md` → the `.yml`, and
`docs/renovate.md` replaces its "tracked in gx#121" sentence with the same link.

### 2. Render the PR body with a single `jq` program over the whole document

`gx upgrade --json` is captured to a file, then one `jq -r` program produces the Markdown body. Structuring
it as one program (rather than a shell loop over `jq` calls) keeps the mapping from JSON shape to Markdown
readable in one glance, which matters more than anything for a file people read to understand.

Each row renders as a Markdown list item `action: from → to` where the version pair is a link to `compare`
when present, and plain text when absent. `compare` is `skip_serializing_if = "Option::is_none"`, so it is
*absent*, not `null` — `jq`'s `//` fallback and `has("compare")` both handle that; the template uses an
explicit `if has("compare")` because a reader should not have to know `//` semantics on missing keys.

`in_range` is deliberately **not** surfaced in the body: the default `gx upgrade` is safe mode, so in the
template's configuration every entry is in-range and a column that is always `true` is noise.

**Alternative considered:** a Markdown table. Rejected — a table with an embedded link per row is harder to
read in the `jq` source than a list, and gains nothing at the two-to-five rows a weekly run typically
produces.

### 3. Gate on `up_to_date` with a step output, checked by the PR step's `if:`

The `jq` step emits `up_to_date` as a step output; the create-pull-request step carries
`if: steps.<id>.outputs.up_to_date == 'false'`. This is one boolean read straight off the contract.

**Alternatives considered:**

- *Test whether `upgrades` is empty* — rejected: it re-derives a fact the contract already states, and the
  two can differ (an upgrade run with only skips is `up_to_date: false` with an empty `upgrades`).
- *Let create-pull-request no-op on a clean tree* — it does, but the job would still run the action and
  report a confusing "no changes" success. The explicit gate is what the issue asks for and reads clearly.

### 4. Default `GITHUB_TOKEN`, with the PAT/App caveat documented rather than templated

The workflow uses the built-in `GITHUB_TOKEN` and declares `permissions: contents: write,
pull-requests: write`. The well-known consequence — PRs opened with `GITHUB_TOKEN` do not trigger further
workflow runs, so CI will not run on the upgrade PR — is stated in the companion doc with the standard fix
(a PAT or a GitHub App token), rather than being baked into the template.

Templating the App-token path would add a second action dependency and two secrets to a file most users
would then have to strip back down. The caveat is real but it is a *documentation* problem.

### 5. Install gx via Homebrew, matching the README

The template installs gx the way `README.md` documents first (`brew install gmeligio/tap/gx`), so the
template and the install docs cannot disagree. `GITHUB_TOKEN` is exported for the `gx upgrade` step because
gx falls back to unauthenticated GitHub API calls (60 req/hour) without it — a rate limit a repo with a
dozen actions can hit.

## Automated Test Strategy

There is no Rust change, so no new unit or integration test is warranted, and `mise run test` should be
unaffected — confirming that is itself the check.

The template's correctness has two parts, verified differently:

- **YAML validity and workflow-level correctness** — verified by running this repo's own tooling against the
  file: `gx lint` reads `.github/workflows` and `.github/actions` only, so `docs/gx-upgrade.yml` is outside
  its scope by construction; validity is instead confirmed with a YAML parse and by eye against the schema.
- **The `jq` program** — the one piece with real logic. Verified by running it against fixture JSON matching
  the shapes the contract can actually produce: an upgrade with `compare`, an upgrade *without* `compare`
  (the `skip_serializing_if` case), and `up_to_date: true`. This is a one-off verification during
  implementation, not a committed test: committing a fixture harness for a docs file would create the
  maintained surface the issue explicitly warns against.

The field names consumed are pinned to `src/upgrade/report.rs` and its tests (`to_json_uses_resolved_versions_and_compare`,
`to_json_omits_compare_when_absent`, `to_json_up_to_date_has_empty_upgrades`), which are the contract's
existing regression guard.

## Observability

Failure modes in the shipped artifact, and how each surfaces:

- **`gx upgrade` fails** (rate limit, network, malformed manifest) — the step exits non-zero and the job
  fails loudly in the Actions UI. No PR is opened. Not silent.
- **`jq` fails or the contract drifts** — because the body is generated by a `jq` program with explicit
  field access, a renamed field yields empty output rather than a crash. This is the one path that could go
  quiet: a PR would open with an empty body. Mitigated by keeping the `up_to_date` gate reading the same
  document, so a contract break that renames `up_to_date` fails the `if:` comparison and suppresses the PR
  instead of opening an empty one. A full rename of `upgrades[]` is caught by the human reading the PR.
- **Nothing to upgrade** — the intended quiet path: job succeeds, no PR, visible in the run log.
- **A `run:` step in the template mishandles quoting** — the template writes the body to a file and passes it
  via `body-path`, so no PR content ever transits a shell interpolation.

Diagnosis for a user is the Actions run log: the raw `gx upgrade --json` output is written to a file and the
job echoes it, so the exact document that produced (or suppressed) a PR is always recoverable.

## Risks / Trade-offs

- **The template drifts as `peter-evans/create-pull-request` and `actions/checkout` release** → Accepted and
  inherent; the issue names it. Mitigated by keeping the dependency count at two actions, both SHA-pinned
  with a version comment so a reader can see exactly how stale their copy is.
- **A SHA-pinned template goes stale silently, and a user copies an old pin** → Mitigated by the version
  comment beside each SHA: the user can compare against the action's latest release at a glance. Not
  mitigated further; pinning to a floating tag would contradict gx's entire thesis.
- **`GITHUB_TOKEN`-opened PRs do not trigger CI** → Documented in the companion doc with the PAT/App fix.
  Not templated, per decision 4.
- **The workflow will not work in a repo whose Actions settings forbid PR creation** → Surfaced as a clear
  API error on the create-pull-request step; the companion doc names the setting.
