## Context

A managed file's schema is decided in two independent places:

```
  discovery::ROOTS                    FileKind::of_path
  discovery.rs:35-46                  parsed/mod.rs:295-309
  glob table, kind per root           ancestor walk, else → Workflow
        │                                     │
        │  ManagedFile{path, kind}            │  called at scanner.rs:224
        ▼                                     ▼  and convert.rs:148
   scanner.rs:265-266                   ┌──────────────┐
   kind consumed, path continues        │ they already │
        │                               │   disagree   │
        ▼                               └──────────────┘
   site::Id { file, slot }  ← no kind field (site.rs:165-171)
        │
        ▼
   every later layer re-derives
```

`FileKind::of_path`'s own doc comment calls it "the one place that decides a file's
schema" (`parsed/mod.rs:291`). It is not — `discovery::ROOTS` decides it too, and
`discovery.rs:22-23` admits the drift risk in prose. `discovery_kind_agrees_with_of_path`
(`composite_tests.rs:299-323`) exists solely to pin them together.

The consequence this change exists to prevent is the **false-positive cascade on #124**.
`of_path` classifies by directory, falling back to `Workflow` (`parsed/mod.rs:306-308`). A
traversed composite at `tools/build/action.yml` parses under the workflow schema —
`wire.runs` ignored, `steps` empty (`parsed/mod.rs:406-410`) — so gx finds zero actions
**and** the file enters `workflows_full`, where eight workflow-schema rules plus
`run-shellcheck` judge it.

A second defect traces to the same two-derivation split but is **out of scope here**: an
override naming a file gx does not scan (e.g. `.github/actions/setup/steps.yml`, which
`of_path` calls an `ActionDefinition` but `ROOTS` never globs) is accepted at validation and
then **silently deleted from the user's `gx.toml` by the next `gx tidy`** — `prune_stale`
(`src/domain/manifest/overrides.rs:126-167`) drops any override that selects no live site.
Fixing it at validation time is not possible: `manifest::parse`
(`src/infra/manifest/parse.rs:92`) takes only the manifest path, and `Config::load`
(`src/config.rs:138-141`) parses the manifest **before** any scan, so no discovered file set
exists at that point. Post-scan is the only coherent place, which is where `prune_stale`
already sits. Tracked on #163, whose `ActionId` cross-check is the same predicate and whose
prune-vs-report choice governs it.

The `Parsed` type is a sum wearing a product's clothes: it carries `on`, `permissions`,
`concurrency`, `defaults`, `jobs` **and** `steps`, one half always empty
(`parsed/mod.rs:318-333`). `Context.workflows_full` is typed `&[ParsedWorkflow]`, but
`ParsedWorkflow` is a **type alias for `Parsed`** (`src/lint/rule.rs:8`) — the narrowing is
cosmetic. The invariant is held by one `.filter()` at `command.rs:69-72` and a doc comment
at `rule.rs:152`.

## Goals / Non-Goals

**Goals:**

- Kind is established once, where a file is found, and carried thereafter.
- `FileKind::of_path` is deleted — not merely bypassed.
- The `workflows_full` invariant becomes a compile error to violate.
- No behavior change for any user whose repo gx classifies correctly today.
- `gx lint`, `gx tidy`, `gx upgrade` produce byte-identical output on any repo whose files
  are all under `.github/workflows` or `.github/actions` (i.e. every repo today).

**Non-Goals:**

- Traversal of local `uses:` edges (#124). This change makes it *safe*; it does not do it.
- Discovery roots beyond `.github` (#135).
- The `Writer` port and site-addressed writes (#159, #153) — independent track.
- Renaming `WorkflowPath`, `WorkflowError`, `Context.workflows` etc. (#155) — must land
  *after* this, since this changes the types those names are attached to.
- GitLab schema support (#144–#149). This unblocks it by making a third kind cheap, but
  adds no variant.

## Decisions

### D1 — `FileKind` becomes a sum type owning its schema's fields; `Parsed` splits

`Parsed` splits into `ParsedWorkflow { path, on, permissions, concurrency, defaults, jobs }`
and `ParsedAction { path, steps }`, with `Parsed` as the enum over them and `path` reachable
from either.

*Alternative rejected — keep `Parsed` as a product, add a real newtype wrapper for the
workflow case.* Cheaper, and makes `workflows_full` type-safe. But "not applicable" stays
representable as "empty", so `parsed.jobs` on a composite still compiles and still returns
`vec![]`. That is the defect #154 names; a wrapper hides it at one call site rather than
removing it.

*Alternative rejected — parse into two entirely separate types with no sum.* The scanner
needs a single return type for a heterogeneous file list (`scan_all_with_parsed`), so the
sum has to exist somewhere; better in the domain than hand-rolled at the boundary.

### D2 — Kind travels on the file, not on `site::Id`

The kind rides with `ManagedFile` (`discovery.rs:11-18`) and, after parsing, is implied by
the `Parsed` variant. `site::Id` is **not** given a kind field.

*Rationale.* `Id` is an identity used as a `HashMap` key (`site.rs:165-171`, with the
hashable/provenance split from `f550f60`). Adding a derived field to a key invites two `Id`s
that name the same place but differ in kind — reintroducing exactly the drift this change
removes, in the type whose whole job is identity. Consumers that need kind either hold the
`Parsed` variant already or can be passed it.

*Consequence for `convert.rs:147-158`.* Override validation currently asks `of_path` whether
a bare-step override is legal. With kind off `Id` — and no discovered set reachable at parse
time — that question moves out of validation entirely; see D4.

### D3 — `Context.workflows_full` takes the narrow type; the filter becomes a total function

`workflows_full: &'ctx [ParsedWorkflow]` where `ParsedWorkflow` is a **real struct**, not
today's alias (`rule.rs:8`). `command.rs:69-72`'s `.filter(|p| p.kind == FileKind::Workflow)`
becomes a partition returning `Vec<ParsedWorkflow>`; deleting it stops compiling.

Eight rules (`dangerous_trigger`, `excessive_permissions`, `missing_concurrency`,
`missing_permissions`, `pr_head_checkout`, `unprotected_secrets`, `dangling_reference`,
`invalid_expression`) plus `run_shellcheck` read `ctx.workflows_full`. All already want
workflow-only data, so each is a signature change with no body change — with one exception
worth calling out: **`run-shellcheck` currently never inspects composite `run:` bodies**
(#160). This change does not fix that; it makes the gap explicit by giving the rule a type
that says "workflows only" rather than a filtered list that silently lost the composites.
Note that on the tests side `stale_comment.rs` passes `workflows_full: &[]` three times —
those keep compiling unchanged.

### D4 — Bare-step override validation becomes shape-only, preserving today's outcome

`convert.rs:147-158` currently asks `of_path` whether a bare-step override (`step` without
`job`) names an action definition, rejecting it otherwise. With `of_path` deleted, that
question cannot be answered at parse time — there is no discovered file set there (see
Context). So the check becomes shape-only: a bare-step override parses to
`Scope::CompositeStep` on any path.

This is a deliberate **no-net-change** for users. Today a bare-step override on a workflow
path is rejected at parse time; afterwards it parses and then selects nothing, because a
workflow's sites all carry a job. The user-visible difference is an error message moving to
silence in a case that was already broken either way — and the silence is what #163 exists
to fix, for both this case and the stale-address case, in the one place that can see the
scanned set.

*Alternative rejected — thread the scanner into `Config::load`.* Would let validation consult
the discovered set and keep the error. But it inverts the config/scan ordering for every
command (`config.rs:138-141` runs before any scan, in `lint`, `tidy`, `upgrade`, `init`), to
recover one message for a config that `prune_stale` deletes anyway. Far past "carry the
kind".

*Alternative rejected — keep a path-shape heuristic inline.* Checking for an `actions`
ancestor directly in `convert.rs` preserves the current message with a small diff. Rejected
because it is `of_path` under another name, in the layer furthest from discovery — exactly
the re-derivation this change removes. It would also be wrong for the #124 files this change
exists to make safe.

*Risk accepted.* The parse-time error for a bare-step override on a workflow path is lost.
It is recovered, better placed, by #163 — which reports against the real scanned set rather
than a path's spelling. Noted in the changelog rather than the migration plan, since no user
action is possible or needed.

### D5 — Delete `of_path` in the same change, not after

Leaving it deprecated would let any new call site silently reintroduce the defect, and #124
adds call sites in exactly the code path where it is wrong. Deletion is what makes the
guarantee hold.

## Risks / Trade-offs

- **Wide diff across the lint layer (nine files) → mechanical and compiler-guided.** Each is
  a signature change with no body change. Sequenced in `tasks.md` so the type lands first
  and the compiler enumerates the rest.
- **A bare-step override on a workflow path stops erroring at parse time → it now parses
  and selects nothing, which is what it did after the error was removed anyway.** Recovered
  properly by #163; changelog entry so the message's disappearance is not mistaken for a
  regression.
- **`Parsed` splitting touches every construction site → two sites, both already branching
  on kind** (`scanner.rs:266`, `:296`). The scanner's existing
  `match kind { Workflow => …jobs, ActionDefinition => …steps }` (`scanner.rs:185-190`) is
  the enum being hand-written; the split absorbs it rather than adding work.
- **Merge conflict with #124 if both are in flight → sequence, do not parallelize.** This
  change touches the exact lines #124 modifies (`scanner.rs:88`, `:265-266`).
- **`from_yaml` defaults to `FileKind::Workflow`** (`parsed/mod.rs:378-380`) and is used by
  tests; the split must keep an equivalent workflow-only entry point or the test surface
  churns unnecessarily.

## Automated Test Strategy

Level: unit + integration, no new infrastructure. The existing suite already covers this
area (`composite_tests.rs`, `parsed/tests.rs`, `override_scope_tests.rs`).

Critical path — the tests that must exist and fail before the change:

1. **The removed derivation.** `discovery_kind_agrees_with_of_path`
   (`composite_tests.rs:299-323`) is deleted, not ported: with one derivation there is
   nothing to agree with. Its replacement asserts kind is what discovery said, read back
   off the parsed file.
2. **The validation change (D4).** A bare-step override parses to `Scope::CompositeStep` on
   any path, and one on a composite path still resolves and applies. The existing
   `step_without_job_on_a_workflow_is_rejected`
   (`src/infra/manifest/override_scope_tests.rs:16`) inverts: it must assert the override
   parses and then selects no site, rather than asserting a parse error.
3. **The #124 precondition.** A file classified `ActionDefinition` whose path is *outside*
   `.github/actions` parses under the action schema and is absent from `workflows_full`.
   Cannot be produced by discovery today, so it is driven by constructing the kind directly
   — which is the point: kind comes from the caller, not the path.
4. **No-op on existing repos.** An integration assertion that `gx lint` output is unchanged
   for a fixture repo with both workflows and `.github/actions` composites.

The `workflows_full` invariant needs no test — after D3 it is a compile error, which is
strictly stronger. Worth a comment at the definition saying so, replacing the prose
invariant at `rule.rs:152`.

## Observability

Failures here are the silent kind, which is why the change exists. Three paths:

- **Override names an unscanned file** — still silent after this change, and still
  silently deleted by the next `gx tidy` via `prune_stale`
  (`src/domain/manifest/overrides.rs:126-167`). Not fixed here, and deliberately not
  half-fixed: parse time cannot see the scanned set, so any check placed there would be a
  path-spelling heuristic that disagrees with `prune_stale`. Tracked on #163.
- **Parse under the wrong schema** — after this change, unrepresentable: kind comes from
  discovery, and a `ParsedAction` has no `jobs` field to be empty. No runtime signal needed
  because there is no runtime failure mode left.
- **A file discovered but classified by neither root** — cannot occur while `ManagedFile`
  is constructed only in `glob_root` (`discovery.rs:82-85`), where kind comes from the
  `Root` that matched. This is the invariant to preserve when #124 adds traversal as a
  second construction site; a traversed file's kind comes from the *edge*, and that must be
  asserted at construction, not defaulted.

Per-file error isolation is unchanged: a malformed file skips that file and does not abort
the scan (`scanner.rs:271`, `discovery.rs:80-81`).

## Migration Plan

No data or config migration. `gx.toml`, `gx.lock`, and the CLI surface are untouched.

Rollout: single PR, on by default, no flag. The one observable change on an existing repo is
D4 — a bare-step override on a workflow path no longer errors at parse time. It selected
nothing before and selects nothing now, so no user action is needed. Changelog entry under
Changed so the vanished message is not read as a regression.

Rollback is a straight revert; nothing persists state across the change.

## Open Questions

None blocking. One deferred by design: whether `run-shellcheck` should inspect composite
`run:` bodies is tracked in #160 — this change makes the gap visible in the type signature
but deliberately does not close it, to keep the diff mechanical.
