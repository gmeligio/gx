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

Two consequences are live or imminent:

1. **Silently-inert override (today).** `of_path(".github/actions/setup/steps.yml")` returns
   `ActionDefinition` (`parsed/tests.rs:458-461`), but `ROOTS` globs only
   `**/action.{yml,yaml}`, so the file is never scanned. An override naming it passes
   validation at `convert.rs:147-158` and matches nothing at runtime.
2. **False-positive cascade (on #124).** `of_path` classifies by directory, falling back to
   `Workflow` (`parsed/mod.rs:306-308`). A traversed composite at `tools/build/action.yml`
   parses under the workflow schema — `wire.runs` ignored, `steps` empty
   (`parsed/mod.rs:406-410`) — so gx finds zero actions **and** the file enters
   `workflows_full`, where eight workflow-schema rules plus `run-shellcheck` judge it.

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
- An override naming an unscanned file is reported, not silently inert.
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
a bare-step override is legal. With kind off `Id`, it instead resolves the named path against
the discovered file set — which is what fixes the silently-inert override (D4), so the two
changes are one move.

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

### D4 — An override naming an unscanned file is reported

Validation moves from "does this path *look* like an action definition" to "is this path in
the discovered set, and what kind is it there". A path gx does not scan produces a
validation error naming the file.

*Alternative rejected — warn instead of error.* An override that matches nothing is always a
user mistake (typo, renamed directory, stale entry); there is no configuration where an
inert override is intended. Erroring matches how `dangling-reference` treats a reference to
something absent.

*Risk accepted.* A repo with a stale override currently gets silence and will now get an
error. That is the fix, and it is the only behavior change a user can observe on an existing
repo. Called out in the changelog and in the migration plan below.

### D5 — Delete `of_path` in the same change, not after

Leaving it deprecated would let any new call site silently reintroduce the defect, and #124
adds call sites in exactly the code path where it is wrong. Deletion is what makes the
guarantee hold.

## Risks / Trade-offs

- **Wide diff across the lint layer (nine files) → mechanical and compiler-guided.** Each is
  a signature change with no body change. Sequenced in `tasks.md` so the type lands first
  and the compiler enumerates the rest.
- **A user's stale override starts erroring → intended (D4); changelog entry + the error
  names the file and the fact that it is not scanned.** Not silently degraded — the whole
  point is to stop being silent.
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
2. **The live defect (D4).** An override naming `.github/actions/setup/steps.yml` — a file
   `of_path` calls an `ActionDefinition` and `ROOTS` never globs — must produce a
   validation error. This is the regression test for the silently-inert bug; it fails today.
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

- **Override names an unscanned file (D4)** — surfaced as a validation error naming the
  path and stating it is not among the files gx scans. Must name the file; "invalid
  override" alone reproduces the current unhelpfulness in louder form.
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

Rollout: single PR, on by default, no flag. The one observable change on an existing repo
is D4 — a stale override that silently matched nothing now errors. Changelog entry under
Fixed, naming the symptom users would have seen (an override that appeared to do nothing).

Rollback is a straight revert; nothing persists state across the change.

## Open Questions

None blocking. One deferred by design: whether `run-shellcheck` should inspect composite
`run:` bodies is tracked in #160 — this change makes the gap visible in the type signature
but deliberately does not close it, to keep the diff mechanical.
