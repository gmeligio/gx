## Why

Actions pinned inside a composite action are invisible to `gx`
([#150](https://github.com/gmeligio/gx/issues/150)). Factoring repeated setup
steps into `.github/actions/<name>/action.yml` is the standard way to remove
duplication across jobs, but every `uses:` moved there silently leaves gx's
management: `gx tidy` prunes those actions from `gx.toml` and `gx.lock` as
"unreferenced", `gx upgrade` stops advancing them, `gx lint` stops flagging them
as unpinned, and nothing warns. Deduplicating workflows quietly shrinks
supply-chain coverage, which inverts the tool's purpose.

This passes the relevance gate: it changes user-facing `gx tidy`, `gx upgrade`,
and `gx lint` behavior (actions that today disappear from the manifest are
retained, pinned, and linted), and it introduces a new domain concept — the set
of files gx manages is no longer "workflows" but "workflow files and composite
action files". Discovery has never been stated as a requirement in any existing
spec, so this is an ADDED requirement, not only a modification.

## What Changes

- Discover `.github/actions/**/action.yml` (and `action.yaml`) alongside
  `.github/workflows/*.yml`, recursively, so nested composite action
  directories are covered.
- Parse `runs.steps[].uses` when `runs.using` is `composite`. Files with any
  other `runs.using` (`node20`, `docker`) contribute no actions and are not an
  error.
- Composite-action `uses:` references participate in every existing pipeline:
  `gx init`/`gx tidy` add them to `gx.toml` and `gx.lock`, `gx tidy` pins them
  in place, `gx upgrade` advances them, and the action-hygiene lint rules
  (`unpinned`, `sha-mismatch`, `stale-comment`, `unsynced-manifest`) evaluate
  them.
- **BREAKING (behavioral):** `gx tidy` in a repository with composite actions
  will now *add* entries it previously pruned, and `gx tidy`/`gx upgrade` will
  now *rewrite* `.github/actions/**/action.yml` files. Existing `gx lint` runs
  may report new `unpinned` diagnostics. No file-format change.
- Local references (`uses: ./.github/actions/foo`) and `docker://` references
  continue to be skipped wherever they appear — including inside a composite
  action referencing another composite action.
- Workflow-schema-only lint rules (`missing-permissions`,
  `excessive-permissions`, `dangerous-trigger`, `missing-concurrency`,
  `pr-head-checkout`, `unprotected-secrets`, `dangling-reference`,
  `invalid-expression`) MUST NOT fire on composite action files, which have no
  `on:`, no top-level `permissions:`, and no jobs.
- Diagnostics and `gx.toml` `ignore`/override targets address a composite step
  by its file path and step index, with no job component.
- **Out of scope (follow-up):** extending `run-shellcheck` to composite
  `runs.steps[].run` bodies; making `.github/actions` configurable (that is
  [#135](https://github.com/gmeligio/gx/issues/135)'s concern); reusable
  workflows called via `uses:` at job level.

## Capabilities

### New Capabilities

- `file-discovery`: which files gx reads to find managed `uses:` references —
  workflow files and composite action files — and what it skips. Today this is
  an unspecified implicit; this change makes it a stated contract so that
  widening it later (e.g. #135) has something to modify.

### Modified Capabilities

- `lint-command`: workflow-schema-only rules are scoped to workflow files;
  action-hygiene rules extend to composite action files; ignore-target
  vocabulary admits a composite step (path + step, no job).
- `manifest-and-lock`: "unreferenced" for the purposes of tidy pruning counts
  composite-action references; the resolved-version annotation contract applies
  to composite action files.

## Impact

- **Spec**: new `openspec/specs/file-discovery/spec.md`; deltas to
  `lint-command` and `manifest-and-lock`.
- **Domain**: `src/domain/workflow_parsed/mod.rs` gains a `runs:` shape and a
  file-kind discriminant; `src/domain/workflow.rs` `Scanner` trait doc/contract
  widens from "workflows" to "managed files".
- **Infra**: `src/infra/workflow_scan/scanner.rs` (recursive discovery of
  `.github/actions`, composite step extraction);
  `src/infra/workflow_update.rs` (its `find_workflows`/`update_all_with_pins`
  hardcodes `.github/workflows`).
- **Lint**: `src/lint/command.rs` must feed workflow-schema rules only workflow
  files; `src/lint/rule.rs` ignore matching.
- **Manifest**: `src/infra/manifest/convert.rs` validates `step` requires
  `job`, which a composite step cannot satisfy.
- **Tests**: new discovery/extraction unit tests; integration tests for tidy,
  upgrade, and lint over composite actions;
  `tests/integ_tidy.rs::gx_tidy_skips_local_actions` asserts the manifest omits
  `.github/actions` and must be re-scoped to local *references*.
- **Docs**: `README.md`, `docs/lint-rules.md` (ignore-target vocabulary),
  `docs/demo.tape` if the demo grows a composite action.
- No file-format change to `gx.toml` or `gx.lock`.
