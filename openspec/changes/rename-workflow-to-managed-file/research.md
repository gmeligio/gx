# Research: #155 — rename workflow-specific names now that gx manages several file kinds

Source: https://github.com/gmeligio/gx/issues/155

## Context

gx manages two file kinds today (workflows, composite actions) and plans a third
(`.gitlab-ci.yml`, #144–#149). `FileKind` already exists — introduced by #151 —
but only in the scan/parse layer. It never reaches location, manifest, config, or
the lint context.

```
                       ┌─────────────────────────────────────────┐
                       │  FileKind lives here                    │
                       │  domain/workflow_parsed/mod.rs:283      │
                       └────────────────┬────────────────────────┘
                                        │ used by
        ┌───────────────┬───────────────┼────────────────┐
        ▼               ▼               ▼                ▼
  discovery.rs:17   scanner.rs:200   Parsed.kind    convert.rs:119
  (glob→kind)       (re-derive)      (:321)         (re-derive from
                                                     path string)

        ╳ CANNOT reach ──▶  Location (workflow_actions.rs:203)
                            ActionOverride (manifest/overrides.rs:11)
                            IgnoreTarget (config.rs:45)
                            Context (lint/rule.rs:143)
```

`workflow_actions` is imported *by* `workflow_parsed`
(`workflow_parsed/mod.rs:6` pulls `WorkflowPath`). So `Location` cannot carry a
`FileKind` without creating a cycle between sibling domain modules. **That is why
kind is encoded as an `Option` shape instead** — it was the only thing the module
graph permitted.

Layering elsewhere is genuinely clean: `grep -rn "crate::infra" src/domain/`
returns nothing. `Scanner` (`domain/workflow.rs:31`) is a real port.

## Findings

### 1. `FileKind`'s placement is load-bearing in the wrong direction

Because `Location` (`workflow_actions.rs:203-214`) has no kind field, the same
question — *which schema does this file follow* — is answered in four unrelated
places by three different mechanisms:

```
  Q: "which schema is this file?"
  ├─ discovery::ROOTS glob table        infra/workflow_scan/discovery.rs:35-46
  ├─ FileKind::of_path ancestor walk    domain/workflow_parsed/mod.rs:295-309
  ├─ job.is_none() && step.is_some()    domain/manifest/overrides.rs:59-61
  └─ of_path on a manifest path string  infra/manifest/convert.rs:118-125
```

The first two are known to be able to drift — there is a test whose whole purpose
is to pin them together (`tests/composite_tests.rs:299-323`,
`discovery_kind_agrees_with_of_path`). A test guarding two copies of one fact is
the signature of a misplaced boundary.

They already disagree in one case. `FileKind::of_path(".github/actions/setup/steps.yml")`
returns `ActionDefinition` (asserted at `workflow_parsed/tests.rs:458-461`), but
`discovery::ROOTS` globs only `**/action.{yml,yaml}` (`discovery.rs:43`), so that
file is never scanned. A `gx.toml` override naming it with `step` and no `job`
passes validation at `convert.rs:120` and then matches nothing. Silent dead config.

`FileKind::of_path` is also GitHub-filesystem logic sitting in domain, and its
shape ("walk ancestors for a directory named X, else `Workflow`") cannot express
`.gitlab-ci.yml` — a filename at repo root. Its `else → Workflow` fallback
(`:307`) would silently misclassify it.

### 2. `Context.workflows` / `workflows_full` differ on two axes at once

```
                      representation          file set
  workflows        │ located refs      │ ALL managed kinds
  workflows_full   │ structural parse  │ Workflow kind ONLY
                   └───────────────────┴──────────────────────
                     ^ differs           ^ also differs
                     ...distinguished only by the token "_full"
```

The filter is at `lint/command.rs:70-73`. The workflow-only invariant is enforced
**only by a doc comment** (`rule.rs:150-160`). `ParsedWorkflow` is a type alias
for `Parsed`, so deleting the filter compiles clean — #154 makes exactly this
point.

Rule census (13 rules):

| Reads | Rules | Correct? |
|---|---|---|
| `ctx.workflows` | sha_mismatch:52, stale_comment:58, unpinned:41 | yes |
| `ctx.action_set` | unsynced_manifest:20 | yes |
| `ctx.workflows_full` | dangerous_trigger:57, excessive_permissions:43, missing_concurrency:48, missing_permissions:37, pr_head_checkout:103, unprotected_secrets:143, dangling_reference:47, invalid_expression:161 | yes — all read `on`/`permissions`/`concurrency`/`needs` |
| `ctx.workflows_full` | **run_shellcheck:51** | **no** |

**Live gap:** `run-shellcheck` iterates `wf.jobs` (`run_shellcheck/mod.rs:73`)
purely as the route to steps. Composite `runs.steps[].run` bodies are never
shellchecked. This was a deliberate deferral in the #151 design
(`openspec/changes/archive/2026-08-02-scan-composite-actions/`), but the *reason*
it's awkward to undefer is the conflation: there are only two field names, so a
rule wanting "steps from every kind" must pick one and be wrong.

The `Rule` trait (`rule.rs:166-176`) does **not** force this — it hands every
rule the whole `Context`. The `Context` *shape* causes it. Fix is a third
accessor, not a trait change.

### 3. `Parsed` is a sum type wearing a product type's clothes

`workflow_parsed/mod.rs:317-333` — for `ActionDefinition`, `on`/`permissions`/
`concurrency`/`defaults`/`jobs` are always empty; for `Workflow`, `steps` is
always empty. The doc comment states this in prose (`:315-316`) because the type
cannot. This is #154 verbatim; #154 also notes `scanner.rs` hand-writes
`match kind { Workflow => …jobs, ActionDefinition => …steps }` — manually
reconstructing the enum the struct refuses to be.

### 4. Module names encode a file kind, and two modules are genuinely mixed

| Module | Contents | Verdict |
|---|---|---|
| `domain/workflow_actions.rs` | `WorkflowPath`, `Location`, `Located`, `ActionSet` | kind-agnostic → pure rename |
| `domain/workflow.rs` | `Error`, `UpdateResult`, `Scanner` | kind-agnostic → pure rename |
| `infra/workflow_scan/` | discovery + extraction, both kinds | kind-agnostic → pure rename |
| `domain/workflow_parsed/` | `Step` (all kinds) **+** `Trigger`/`Permissions`/`Job`/`Concurrency`/`Defaults` (workflow-only) | **mixed — renaming makes it *less* accurate** |
| `infra/workflow_update.rs` | `uses:` regex rewriter | **genuinely GitHub-specific** |

`trigger.rs:1` and `permissions.rs:1` describe the workflow schema's `on:` and
`permissions:` blocks. Renaming their parent to `managed_file` would be a
regression in accuracy, not an improvement.

### 5. `WorkflowWriter` is not a seam, and it works on composites by coincidence

No trait (`infra/workflow_update.rs:22`). Imported concretely by
`init/command.rs:9`, `tidy/command.rs:18`, `upgrade/command.rs:14`,
`upgrade/plan.rs:12`. It is pure regex substitution
(`workflow_update.rs:111-135`): `(uses:\s*{escaped})@[^\s#]+(\s*#[^\n]*)?`. It
never asks what schema the file follows — it works for composite actions only
because both schemas spell it `uses:`. **GitLab has no `uses:` at all**, and
there is no dispatch point to put a second strategy. `find_managed_paths` is
duplicated as an inherent method on both `FileScanner` (`scanner.rs:132`) and
`WorkflowWriter` (`workflow_update.rs:43`).

### 6. `WorkflowPath` is a clean rename; two issue entries are stale

`WorkflowPath` (`workflow_actions.rs:99-112`) is an unvalidated newtype over
`String` whose only behavior is `\` → `/` normalization. Zero workflow-specific
invariants. 110 occurrences across `src/` + `tests/`.

Stale in #155: `FileScanner::find_workflows` and `WorkflowWriter::find_workflows`
were already renamed to `find_managed_paths`/`find_managed` in #151. Only the
*trait* method `Scanner::find_workflow_paths` (`domain/workflow.rs:59`) still has
the old name — and its doc comment already reads "Enumerate all managed file
paths", so the name contradicts its own documentation. One production caller:
`tidy/patches.rs:34`. `WorkflowActionSet` is really `ActionSet`
(`workflow_actions.rs:21`) aliased at each import site — an import-alias issue.

### 7. Issue graph

```
  #123 ──▶ #151 (landed) ──▶ introduces FileKind at scan layer only
                                   │
                                   ├──▶ #154  Parsed → sum type
                                   ├──▶ #126  Location conflates provenance/scope
                                   ├──▶ #153  per-job/step overrides not written
                                   └──▶ #155  the names  ◀── you are here
                                          │
  #144–#149 GitLab ──── forces ───────────┘
```

#154 already reaches this report's conclusion independently, including "root
cause: `FileKind` was introduced at the discovery boundary and never propagated
into the domain types", and says it should land before GitLab work starts. #126
covers the `Location` half. #155 is the naming residue of both.

## Options

| Approach | Pros | Cons |
|---|---|---|
| **A. Rename-only sweep now** | Mechanical; lands the churn once; no behavior risk | Freezes `workflows_full`'s prose invariant and the shellcheck gap into a *new* name; renames `Location.job`/`step` and `Context.workflows_full`, which #154/#126 then delete; `workflow_parsed` → `managed_file` makes `Trigger`/`Permissions` less accurate |
| **B. Move `FileKind` first, then #154/#126, then sweep** | Deletes names before renaming them; unblocks D1 (structurally unreachable today); closes shellcheck gap cheaply; gives GitLab somewhere to land | Three issues instead of one; the sweep waits |
| **C. Do nothing until GitLab lands** | Zero cost now | Pays the modeling cost three times; #154's stated argument against |

```
  OPTION A                            OPTION B
  ┌──────────────────┐                ┌──────────────────┐
  │ rename ~15 names │                │ move FileKind    │  small, ~1 file
  └────────┬─────────┘                └────────┬─────────┘
           │                                   ▼
           ▼                          ┌──────────────────┐
  ┌──────────────────┐                │ #154 + #126      │  deletes names
  │ #154/#126 later  │                └────────┬─────────┘
  │ DELETE names you │                         ▼
  │ just renamed     │                ┌──────────────────┐
  └──────────────────┘                │ rename residue   │  smaller sweep
                                      └──────────────────┘
```

## Recommendation

**Option B.** Split #155 into three pieces and sequence them.

The enabling move is small and specific: **lift `FileKind` out of
`domain/workflow_parsed/` into its own domain module** that both
`workflow_actions` and `workflow_parsed` may depend on. One file created, ~6
import sites. Everything else follows from it.

```
  BEFORE                                AFTER
  ┌──────────────────┐                  ┌──────────────────────┐
  │ workflow_actions │◀────┐            │ domain/managed_file  │
  │  Location{       │     │            │  FileKind            │
  │   workflow,      │     │ imports    │  StepScope           │
  │   job:  Option,  │     │ WorkflowPath│  ManagedFilePath    │
  │   step: Option}  │     │            └──────┬───────┬───────┘
  └──────────────────┘     │                   │       │
  ┌──────────────────┐     │            ┌──────▼───┐ ┌─▼──────────┐
  │ workflow_parsed  │─────┘            │file_     │ │ file_body/ │
  │  FileKind  ◀─ can't reach Location  │actions   │ │  workflow  │
  │  Parsed{7 fields,│                  │ Location{│ │  composite │
  │   half always    │                  │  path,   │ └────────────┘
  │   empty}         │                  │  scope}  │
  └──────────────────┘                  └──────────┘
```

Concretely:

```rust
pub enum StepScope {
    Job { job: JobId, step: StepIndex },  // workflow
    Step { step: StepIndex },             // composite
    File,                                 // manifest-derived
}
```

What this buys, in order:

1. `overrides.rs:59-61` matches on `StepScope::Composite` instead of inferring
   from `Option` shape.
2. `convert.rs:118-125` validation disappears — "step without job in a workflow"
   becomes unrepresentable, so there is no runtime check to write.
3. `FileKind::of_path` is **deleted**, not moved. Classification collapses into
   `discovery`, which already pairs glob↔kind (`discovery.rs:24-31`). The
   drift-guard test at `composite_tests.rs:299-323` goes with it.
4. `Context` gains a third accessor — `workflows` (all, refs), `workflow_bodies`
   (workflow-kind, structural), `all_steps` (all kinds). `run-shellcheck` moves
   to the third and its composite blind spot closes. The other 8 rules change a
   field name only. `Rule` trait unchanged.
5. GitLab gets a place to land: `discovery` classifies by filename as easily as
   by directory, and a third `StepScope` variant is additive.

Then sweep the residue — which is genuinely mechanical: `WorkflowPath` →
`ManagedFilePath` (110 refs, zero invariants), `Scanner::find_workflow_paths` →
`managed_paths` (1 caller), `Diagnostic.workflow`, `ActionOverride.workflow`,
`TomlOverride.workflow`, `IgnoreTarget.workflow`, `WorkflowError`,
`UpdateResult`, `WorkflowPatch`, and modules `workflow_actions`/`workflow.rs`/
`workflow_scan`.

Two items should be **carved out of the sweep**, because a rename is the wrong
operation on them:

- `domain/workflow_parsed/` — split, don't rename. `Trigger`/`Permissions`/
  `Job`/`Concurrency`/`Defaults` are a real workflow-only subset that deserves
  its own submodule once GitLab lands.
- `infra/workflow_update.rs` — needs a `Writer` trait before a rename means
  anything. Its `uses:` regex is GitHub-specific and has no dispatch point.

**Config keys** (`workflow =` in `ignore` and `[actions.overrides]`,
`config.rs:49` / `convert.rs:29`): rename to `file =` with both keys accepted
during a deprecation window. Decide against the GitLab timeline — once
`.gitlab-ci.yml` lands, `workflow =` is indefensible. This is the only genuinely
breaking piece and is separable from everything above.

## Open questions

None blocking. One product decision the code cannot answer: whether the
`workflow =` → `file =` migration accepts both keys indefinitely or deprecates on
a schedule. That is a compatibility-policy call, not a research finding.

## Next steps

1. File the `FileKind` relocation as its own small change (prerequisite for #154
   and #126) — or fold it into #154, which already names it as the root cause.
2. Land #154 + #126.
3. Then `/opsx:propose` the rename sweep on this change name, scoped to the
   residue and excluding `workflow_parsed` and `workflow_update.rs`.
4. Decide the config-key migration separately, before #144.

Worth also filing: `run-shellcheck` never inspects composite `run:` bodies
(`run_shellcheck/mod.rs:51,73`) — a real coverage gap, cheap once step 2 lands.
