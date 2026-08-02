## Context

Discovery today is a single hardcoded directory. `FileScanner::new`
(`src/infra/workflow_scan/scanner.rs:113`) sets `workflows_dir =
repo_root/.github/workflows`, and `find_workflow_files` (`scanner.rs:80`) globs
`*.yml`/`*.yaml` non-recursively inside it. Extraction reads only
`parsed.jobs[].steps[].uses` (`scanner.rs:165`). `WireWorkflow`
(`src/domain/workflow_parsed/mod.rs:365`) has no `runs:` field, so a composite
`action.yml` parses successfully into an empty `Parsed` — every field is
`#[serde(default)]` — and its steps are silently dropped. That silence is the
bug: nothing errors, the actions simply vanish.

Four facts make this change narrower than it first appears:

- `WorkflowPath` (`src/domain/workflow_actions.rs:101`) is an unvalidated
  newtype over `String`. `.github/actions/foo/action.yml` is already a legal
  value.
- `Location.job` and `Location.step` are already `Option`
  (`workflow_actions.rs:208`, `:210`). A composite step needs no synthetic job
  id.
- `ActionSet` is location-free — pure `(id, version)` aggregation.
- `src/infra/workflow_update.rs` rewrites by regex over file text
  (`workflow_update.rs:128`), never by YAML schema. Handed a composite file
  path, it already writes correctly.

The blockers are: the missing `runs:` parse shape, the `jobs`-only extraction
loop, two hardcoded `.github/workflows` roots (`scanner.rs:113`,
`workflow_update.rs:31`), the workflow-schema lint rules that would
false-positive on a file with no `on:`/`permissions:`, and
`src/infra/manifest/convert.rs:115` which rejects a `step` override without a
`job`.

## Goals / Non-Goals

**Goals:**

- Composite-action `uses:` references are discovered, pinned, upgraded, and
  linted exactly like workflow ones.
- Recursive discovery, so a composite action nested under
  `.github/actions/group/sub/action.yml` is found. "Nested composite actions"
  in #150 means composites referencing composites; since discovery is
  file-based and reference-agnostic, recursive globbing covers it without any
  reference-following graph walk.
- Workflow-schema-only rules stay silent on composite files — no new false
  positives.
- Discovery becomes a stated spec contract, so #135 (configurable workflow
  paths) has something concrete to widen.

**Non-Goals:**

- Making the `.github/actions` root configurable. That is #135.
- Extending `run-shellcheck` to composite `runs.steps[].run` bodies. It is the
  one step-level `Parsed` consumer that would work, but it needs the `defaults`
  precedence story rethought for a schema with no `defaults:`; deferred.
- Following `uses: ./.github/actions/foo` edges to build a reference graph.
  Local references stay skipped.
- Reusable workflows (`uses: ./.github/workflows/release.yml` at job level).
- Any `gx.toml` / `gx.lock` format change.

## Decisions

### D1: A managed file has a kind; extraction dispatches on it

**Decision:** introduce a file-kind discriminant on the parsed model —
`Workflow` vs `CompositeAction` — carried alongside `Parsed`, and add a `runs:`
shape to the wire struct. Extraction picks the step list from
`jobs[].steps[]` or `runs.steps[]` accordingly; the `uses:` handling after that
is byte-identical (same `USES_RE`, same local/`docker://` skip, same
`UsesRef::new`).

**Alternative considered — synthesize a fake job named `runs` so composite
steps flow through the existing `jobs` loop unchanged:** rejected. It would
leak a fabricated job id into `Location.job`, into diagnostic sort order
(`src/lint/command.rs:163`), and into `gx.toml` override/ignore vocabulary,
where users would then be able to write `job = "runs"` for a schema that has no
jobs. `Location.job` being `Option` already models "no job" correctly; a
composite step SHALL carry `job: None`.

**Alternative considered — a separate `CompositeParsed` type and a second
scanner:** rejected as duplication. Discovery, read, YAML parse, regex
extraction, and error mapping are all shared; only the step-list lookup
differs. One type with a kind keeps the single-parse-pass property that
`scan_all_with_parsed` exists to provide.

### D2: Workflow-schema rules are fed only workflow files

**Decision:** `Context.workflows_full` (`src/lint/rule.rs:153`) continues to
carry only workflow-kind parses. Composite files contribute to
`Context.workflows` (the `Located` list) and `Context.action_set`, which is
what the four action-hygiene rules and `unsynced-manifest` read.

This is filtering at the context boundary rather than adding a guard clause to
each of the eight schema-only rules. Rationale: `missing-permissions`
(`src/lint/workflow_security/missing_permissions.rs:15`) flags an *absent*
`permissions:` block, so it fails open — every composite file would produce a
diagnostic. The other seven happen to fail closed today (they gate on `on:` or
on `jobs`, both empty), but that is incidental, not designed. A per-rule guard
would leave seven rules correct by accident and invite the next rule author to
get it wrong. One filter at the boundary makes the invariant structural.

**Alternative considered — pass everything and let each rule check the kind:**
rejected per above. Also worse for `run-shellcheck`, which is the rule we
*want* to opt in later: with a boundary filter, opting in is a deliberate
second field on `Context`, not an accidental inclusion.

### D3: Discovery is one recursive glob per root, errors are per-file

**Decision:** `FileScanner` holds a list of discovery roots with their
patterns: `.github/workflows/*.{yml,yaml}` (non-recursive, unchanged) and
`.github/actions/**/action.{yml,yaml}` (recursive). Discovery order is
workflows first, then composite actions, each internally sorted, so output is
deterministic.

Error classification follows the existing contract in
`openspec/specs/action-resolution/spec.md:243`: a composite file that cannot be
read or parsed is a per-file error surfaced the same way a malformed workflow
is today (`scanner.rs:254` yields `Err` without aborting the scan). A file with
`runs.using` other than `composite` is **not** an error — it contributes zero
actions silently, because `node20`/`docker` actions legitimately have no
`uses:` steps.

**Alternative considered — glob `.github/actions/**/*.{yml,yaml}`:** rejected.
GitHub only recognizes `action.yml`/`action.yaml` as an action definition; a
sibling `config.yml` in the same directory is not one, and parsing it as one
would produce spurious errors.

### D4: `update_all_with_pins` gets its file list from the scanner

**Decision:** `WorkflowWriter::update_all_with_pins`
(`src/infra/workflow_update.rs:85`) currently globs its own hardcoded
`.github/workflows` list, which would leave composite files unpinned on `gx
upgrade` even after discovery works. It SHALL take the file list from the same
discovery source the scanner uses rather than re-deriving it. `apply_patches`
already takes explicit paths and needs no change.

Two discovery implementations that can disagree is the exact failure mode this
change exists to fix — an action gx knows about but does not rewrite.

### D5: A composite step override is `{ workflow = <path>, step = N }`

**Decision:** relax `src/infra/manifest/convert.rs:115`, which currently errors
on `step` without `job`, to permit it when the target is a composite action
file. The `workflow` key is kept as the path key rather than introducing a
parallel `action_file` key: it is already a free-form path string
(`src/config.rs:48`), matching is suffix-based (`src/lint/rule.rs:207`), and a
second key would double the ignore/override vocabulary for no user benefit.
Docs will describe the key as "the file path", not "the workflow path".

**Trade-off accepted:** `workflow = ".github/actions/setup/action.yml"` reads
slightly off. Renaming the key is a breaking `gx.toml` change and is not worth
it here; if #135 lands a broader path story, renaming can be considered then
with a migration.

## Automated Test Strategy

Per `AGENTS.md`, the regression tests land first and MUST fail against current
`main`.

- **Unit — discovery** (`src/infra/workflow_scan/tests.rs`): composite file
  found under `.github/actions/foo/action.yml`; found when nested
  (`.github/actions/a/b/action.yml`); `action.yaml` extension; a
  non-`action.yml` sibling is not discovered; `runs.using: node20` yields zero
  actions and no error; malformed composite YAML yields `Err` without aborting
  the scan (mirrors `tests.rs:448`).
- **Unit — extraction**: composite step yields `Location { workflow: <path>,
  job: None, step: Some(i), line: Some(n) }`; local (`./...`) and `docker://`
  references inside a composite are skipped (extends the existing
  `scan_skips_local`, `tests.rs:103`); per-step version comments stay distinct.
- **Unit — parse** (`src/domain/workflow_parsed/tests.rs`): `runs.using:
  composite` parses to the composite kind with its steps; a workflow file still
  parses to the workflow kind.
- **Integration — tidy** (`tests/integ_tidy.rs`): an action used *only* in a
  composite action is added to `gx.toml`/`gx.lock` and pinned in the
  `action.yml` file, not pruned. Re-scope
  `gx_tidy_skips_local_actions` (`integ_tidy.rs:345`) — its assertion that the
  manifest omits `.github/actions` conflates "local reference" with "composite
  file" and must assert only that the local `./` *reference* is absent.
- **Integration — upgrade** (`tests/integ_upgrade.rs`): a pin inside a
  composite action is advanced and the file rewritten (guards D4).
- **Integration — lint** (`tests/integ_lint.rs`): `unpinned` fires on a
  composite step and the diagnostic renders `.github/actions/foo/action.yml:N`;
  `unsynced-manifest` does not report an action that appears only in a
  composite; `missing-permissions` does **not** fire on any composite file
  (guards D2); an ignore entry keyed on the composite path suppresses the
  diagnostic.

Harness note: `tests/common/setup.rs` needs a `write_composite_action(root,
name, content)` helper beside `write_workflow` (`setup.rs:29`).

## Observability

Failure must not be silent — silence is the defect being fixed.

- A composite file that cannot be read or parsed produces a per-file error
  through the existing `WorkflowError::ParseFailed`/`ScanFailed` path, carrying
  the composite file's path so the user can see *which* file gx gave up on. It
  does not abort the scan of other files.
- A `runs.using` value other than `composite` is deliberately silent (zero
  actions, no error) — `node20` and `docker` actions are normal and have no
  `uses:` steps to manage. This is the one intentional silence, and it cannot
  hide a managed reference, because those schemas have no step list at all.
- `gx tidy`/`gx upgrade` counters: the summary today reads `N workflows`
  (`src/tidy/report.rs:8` `workflows_updated`, surfaced in `--json` per
  `docs/renovate.md:120`). Composite files rewritten MUST be included in that
  count so a user watching the summary sees the writes happen; the JSON field
  name is retained to avoid a breaking output change, and the human-facing
  noun becomes "files".
- Existing diagnostic rendering (`src/output/lines.rs:111`) already prints
  `{path}:{line}`, which works unmodified for a composite path.

## Risks / Trade-offs

- **A user upgrades gx and `gx tidy` suddenly rewrites files it never touched
  before** → This is the intended fix, but it is a behavior change on an
  existing repo. Mitigated by it being additive and reversible (the pins are
  correct SHAs), by `gx tidy` already being an explicit user-invoked write
  command, and by calling it out in the proposal as a behavioral break for the
  changelog.
- **New `unpinned` errors break someone's CI on upgrade** → Real: a repo whose
  composite actions were never pinned will now fail `gx lint`. Mitigated by the
  existing per-rule `ignore` mechanism, which accepts the composite path.
  Documented in the release notes rather than gated behind a flag; a flag would
  make the default state the insecure one.
- **Two discovery sources drift** (scanner finds a file the writer does not)
  → D4 makes them one source. Guarded by the upgrade integration test.
- **A schema-only rule added later forgets composite files exist** → D2's
  boundary filter makes the default safe: a new rule reading `workflows_full`
  cannot see composite files at all.
- **`runs.using: composite` with steps that use `with:`/`env:` referencing
  expressions gx does not understand** → Not a concern; extraction only reads
  `uses:`, and the regex path is unchanged.
