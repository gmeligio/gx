## Context

`Location` (`src/domain/workflow_actions.rs:203-214`) is one struct serving two
roles:

```rust
pub struct Location {
    pub workflow: WorkflowPath,   // identity + provenance
    pub job: Option<JobId>,       // identity
    pub step: Option<StepIndex>,  // identity
    pub line: Option<u32>,        // provenance
}
```

It derives `PartialEq, Eq` but **not** `Hash` — `line` would poison it — so
override resolution is four sequential linear scans
(`src/domain/manifest/overrides.rs:38-73`) rather than map lookups.

The split follows an existing cleavage plane. There is exactly one production
constructor (`src/infra/workflow_scan/scanner.rs:90-95`), and no consumer reads
both halves:

| Consumer | Reads | Role |
|---|---|---|
| `overrides.rs:39-41,50-52,63,70` | `workflow`,`job`,`step` | identity |
| `overrides.rs:122-124`, `:169-171` | `workflow`,`job`,`step` | identity |
| `manifest/mod.rs:57` | `workflow`,`job`,`step` | identity |
| `tidy/patches.rs:29`,`:59` | `workflow`,`job`,`step` | identity |
| `sha_mismatch.rs:36-37` | `workflow`,`line` | provenance |
| `stale_comment.rs:42-43` | `workflow`,`line` | provenance |
| `unpinned.rs:25-26` | `workflow`,`line` | provenance |
| `lint/rule.rs:294` | `workflow` | provenance |

Two facts about kind are relevant. `Location` cannot carry `FileKind` today —
`workflow_parsed/mod.rs:6` imports `WorkflowPath` from `workflow_actions`, so the
reverse edge would create a cycle. And the composite discriminant is currently
encoded as an `Option` shape: `job.is_none() && step.is_some()`, read that way at
`overrides.rs:59-61` and re-derived from a path string at
`src/infra/manifest/convert.rs:118-125`.

## Goals / Non-Goals

**Goals:**

- Identity (`SiteId`) is `Hash + Eq` and usable as a map key.
- Provenance (`Origin`) is separate and never participates in matching.
- `Slot` makes the schema-shape invariant type-enforced rather than prose.
- #161 fixed: file-to-pins pairing becomes an exact key lookup.
- Zero user-visible behavior change apart from #161's determinism.

**Non-Goals:**

- `ReachedBy` / reachability — lands with #124.
- `SiteSelector` unifying overrides with lint ignores — needs the `config.rs`
  policy-type move first.
- Any change to how steps are addressed (see Decision 3).
- #153, #162, #163.

## Decisions

### 1. A new leaf module `src/domain/site.rs`, not a growth of `workflow_actions`

`site.rs` imports nothing from `domain`. `WorkflowPath`, `JobId`, and
`StepIndex` (`workflow_actions.rs:99-200`) move into it unchanged — they are
self-contained newtypes with no `action::` dependency.

Leaf placement is what makes the follow-up work legal: `manifest/overrides.rs`
and, later, a relocated lint-policy module can both depend on `site` without
importing each other and without either importing `infra`.

*Alternative considered:* keep the types in `workflow_actions` and add `SiteId`
there. Rejected — `workflow_actions.rs:2` imports `action::uses_ref::ParsedRef`
while `action/uses_ref.rs:2` imports `workflow_actions::WorkflowAction`. That
pair is already one mutually-recursive unit; adding the addressing types to it
would pull `manifest` into the cycle's blast radius.

### 2. `Slot` as a sum type, and it does not merge with `FileKind`

```rust
pub enum Slot {
    WorkflowStep { job: JobId, step: StepIndex },
    CompositeStep { step: StepIndex },
    WorkflowJob { job: JobId },   // #125, no other change needed
}
```

This replaces `(Option<JobId>, Option<StepIndex>)`, whose four representable
combinations include two the scanner never produces. It makes the invariant
asserted in prose at `overrides.rs:30-31` — "job-bearing tiers and file-step
cannot collide" — hold by construction, and removes the re-derivation at
`convert.rs:118-125`.

`Slot` and `FileKind` stay distinct. `FileKind` is a **parser** concern (which
YAML schema, where the steps are — `workflow_parsed/mod.rs:278-288`, dispatched
at `scanner.rs:174-184`). `Slot` is an **addressing** concern. They correlate
today only because each schema has one step shape; #125 and GitLab break that
correlation. Because `Slot` carries the job/step discriminant natively, it needs
no `FileKind` import, so the module cycle never arises.

*Alternative considered:* put `FileKind` on `Location`. Rejected — creates the
cycle, and conflates two questions that diverge as soon as a third file kind
exists.

### 3. Addressing is untouched: `Slot` carries `StepIndex`

Addressing steps by `Step.id` (`workflow_parsed/mod.rs:145`) was considered and
rejected:

- `id` is optional and absent on most steps, so it could only be a **second**
  address form alongside the index — carried permanently through manifest read,
  write, matching, and pruning.
- gx does not control it. Renaming `id: setup` → `id: install` breaks a
  name-addressed override exactly as inserting a step breaks an index-addressed
  one. It relocates the fragility and adds a failure mode.
- `openspec/specs/file-discovery/spec.md:55-63` deliberately requires index
  addressing so a user addresses "the file and step they can see in their
  editor."

The underlying fragility — an override silently retargeting to a different
action — is #163, fixed by cross-checking the override's `ActionId` against the
action at its address. That needs no new address form and is independent of this
change.

### 4. `Origin` is a struct, not a bare `Option<u32>`

`Origin { line: Option<u32> }` reads no better than `Option<u32>` today, but it
is where a source span goes when line-level reporting grows, and it gives the
provenance half a name in signatures. Cheap now, awkward to retrofit.

### 5. #161 is fixed by deletion, not by patching the match

`compute_workflow_patches` (`src/tidy/patches.rs:39-42`) bridges an absolute path
to a relative key with `abs_str.ends_with(loc.as_str())` inside `.find()` over a
`HashMap` — nondeterministic when two keys match, which nested composites make
reachable.

`FileScanner::rel_path` (`scanner.rs:115-123`) already produces exactly the key
being looked up. Threading it through makes the lookup an exact map hit and the
suffix match goes away entirely.

## Risks / Trade-offs

**Wide mechanical diff (~14 files touch `WorkflowPath`).** → One constructor and
disjoint readers mean each site is a local edit. The compiler finds every one;
there is no silent-fallthrough path.

**`Slot` construction must be exhaustive at the manifest boundary.** A `gx.toml`
override with `step` but no `job` on a workflow file is currently rejected at
`convert.rs:118-125`; after the change that combination is unrepresentable, so
the rejection moves into parsing. → Preserve the existing error message and add a
test asserting the same user-facing text, so the diagnostic does not regress into
a generic parse failure.

**`Hash` on `SiteId` invites premature map-keying.** Override resolution is
tiered and order-dependent (`overrides.rs:37-73`); a naive `HashMap` lookup would
lose the precedence semantics. → This change adds the capability, not the
rewrite. Resolution stays as-is.

**Overlap with #124's eventual `ReachedBy`.** → `Located` gains a field later
without a signature change; `Scanner::scan` (`domain/workflow.rs:36-38`) returns
`Iterator<Item = Result<Located, Error>>` either way.

## Automated Test Strategy

Unit-level, in-crate, following the existing `#[cfg(test)]`-at-bottom convention.

- **`Slot` exhaustiveness** — the two `(job, step)` combinations the scanner
  never produces become unrepresentable. Assert construction from each schema:
  a workflow step yields `WorkflowStep`, a composite step yields `CompositeStep`.
- **Identity/provenance separation** — two `SiteId`s equal under differing
  `Origin.line` must compare and hash equal. This is the invariant `Location`
  could not state; it is the change's core claim.
- **Override resolution parity** — the existing tests in `overrides.rs` and
  `overrides_composite_tests.rs` must pass unchanged in behavior. They are the
  regression net for "no user-visible change"; port them to the new types rather
  than rewriting their assertions.
- **Manifest round-trip** — a `gx.toml` with file-, job-, and step-scoped
  overrides reads and writes byte-identically.
- **Rejected combination** — `step` without `job` on a workflow file still errors
  with the same message as `convert.rs:118-125` produces today.
- **#161 determinism** — a fixture repo with `.github/actions/build/action.yml`
  and a nested `.github/actions/x/.github/actions/build/action.yml` pairs each
  file with its own pins. This test must **fail** before the fix; because the
  current bug depends on `HashMap` iteration order it is not reliably
  reproducible, so assert the exact pairing rather than looping for a flake.

No new test infrastructure. Critical path is override resolution parity — if
those tests pass unchanged, the refactor is behavior-preserving.

## Observability

No new runtime surface: this is a type refactor, and most failures it could
introduce are compile errors rather than silent misbehavior.

Two paths where a failure could be silent, and what makes it loud:

- **Wrong `Slot` variant at the manifest boundary** would misroute override
  resolution and produce no diagnostic — the override would simply not apply.
  The sum type prevents the invalid combinations; the parity tests cover the
  valid ones. There is no logging to add, because there is no ambiguity left at
  runtime to report on.
- **#161's pairing** currently fails silently by design — the wrong file gets the
  wrong pins and nothing warns. After the fix, a lookup miss is a definite
  absence rather than an arbitrary match. A file with no located actions yields
  no patch, which is already the correct and observable outcome.

Existing error classification is preserved: manifest validation failures remain
hard errors surfaced through `ManifestError::Validation`; nothing this change
touches moves between warning and failure.

## Migration Plan

None required. No persisted format changes, no user action. `gx.toml` and
`gx.lock` are read and written exactly as before.

Internally the change is one commit — the compiler cannot type-check a partial
split, so staging it across commits would leave the tree broken. Rollback is a
revert.

## Open Questions

None blocking.
