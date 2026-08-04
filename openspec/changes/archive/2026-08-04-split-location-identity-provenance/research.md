# Research: #126 — Location.workflow conflates provenance with override scope

Source: https://github.com/gmeligio/gx/issues/126

## Context

`Location` (`src/domain/workflow_actions.rs:203-214`):

```rust
pub struct Location {
    pub workflow: WorkflowPath,
    pub job: Option<JobId>,
    pub step: Option<StepIndex>,
    pub line: Option<u32>,
}
```

Constructed in exactly **one** production site (`infra/workflow_scan/scanner.rs:90-95`);
consumed in five (`manifest/mod.rs:57`, `manifest/overrides.rs:33-76`+`121-125`+
`168-172`, `tidy/patches.rs:29`+`59`, three lint rules, `lint/rule.rs:294`).

```
  scanner.rs:90 ──▶ Location ──┬──▶ overrides.rs   identity (exact match)
   (sole ctor)                  ├──▶ patches.rs     identity
                                └──▶ lint rules     provenance (workflow + line)
```

## Findings

### 1. `Location` derives `Eq` but not `Hash` — and that is the tell

`workflow_actions.rs:203` derives `PartialEq, Eq` and **not** `Hash`, because
`line` would poison it. The type is compared field-by-field in four sequential
linear scans (`overrides.rs:38-73`) but is never a map key.

The cleavage is clean and behavioral: **no call site reads both halves.**

| Consumer | Reads | Role |
|---|---|---|
| `overrides.rs:39-41,50-52,63,70` | `workflow`,`job`,`step` | identity |
| `overrides.rs:122-124`, `:169-171` | `workflow`,`job`,`step` | identity |
| `sha_mismatch.rs:36-37` | `workflow`,`line` | provenance |
| `stale_comment.rs:42-43` | `workflow`,`line` | provenance |
| `unpinned.rs:25-26` | `workflow`,`line` | provenance |

So the split is a **pure refactor, not a redesign**. One constructor, disjoint
readers.

### 2. The dichotomy dissolves — one side is single-valued, the other is a relation

The issue frames this as provenance *vs* scope identity, implying a choice. Both
options it presents are correctness regressions, which is the signal the framing
is off.

The resolution: **identity is where a reference is written** (single-valued,
belongs in the key). **Reachability is a relation** (many-valued, cannot be a key
field at all). #124 doesn't force a choice between them — it adds a set.

```
  BEFORE (one field, two jobs)      AFTER
  Location {                        SiteId  { file, slot }    identity, Hash+Eq
    workflow ◀── identity           Origin  { line }          provenance
             ◀── provenance         ReachedBy(BTreeSet<Path>) relation (#124)
    job, step, line
  }
```

`Located` (`workflow_actions.rs:217-222`) holds exactly one `Location`, no set —
so reachability is currently unrepresentable, not merely unrecorded.

### 3. `StepIndex` is positional but persisted as a name — the real gate on #124

Assigned by `enumerate()` (`scanner.rs:72`), written into the user's `gx.toml`
(`convert.rs:199-201`), read back (`:143-147`), used as the pruning key
(`overrides.rs:171`).

Insert one step above an override and every override below it silently
retargets to a different action. `prune_stale` (`overrides.rs:168-172`) won't
catch it — it only checks that *some* location has that triple, and after the
insertion one does. The override's `ActionId` is never validated against the
action actually at that address.

Meanwhile `Step.id: Option<String>` (`workflow_parsed/mod.rs:145`) — a stable,
author-given name — is parsed and read **only** by `invalid_expression.rs:48`,
never by the addressing path.

This is the strongest argument for sequencing before #124: once nested composite
addresses are persisted as bare indices, every later fix becomes a migration of
user files.

### 4. `config.rs` is architecturally inverted, and it blocks the unification

`src/config.rs:3-4` imports `crate::infra::lock::Store` and
`crate::infra::manifest::parse`; `:69,76` import `crate::lint::RuleName`.
`Config::load` (`:138-153`) does file I/O.

So a top-level module depends on **infra** *and* a **command** module, while
holding `IgnoreTarget` (`:45-52`) — a pure domain value object with no I/O.
It is two things fused: an application composition root (`Config`, `Settings`,
`load`) and domain policy types (`Level`, `IgnoreTarget`, `Rule`, `Lint`).

```
   src/domain/  ──▶ (nothing outside domain)      ✓ clean
   src/config.rs ──▶ crate::infra::lock           ✗ inverted
                 ──▶ crate::infra::manifest       ✗
                 ──▶ crate::lint::RuleName        ✗
```

**This is why `SiteSelector` can't be built today.** `domain/manifest/overrides.rs`
would have to import `crate::config`, which imports `crate::infra` — inverting
the arrow the rest of domain holds perfectly. The obstruction is one misplaced
file, not anything intrinsic.

### 5. Three types approximate "which sites does this apply to", with three rules

| | `Location` `workflow_actions.rs:204` | `ActionOverride` `overrides.rs:12` | `IgnoreTarget` `config.rs:45` |
|---|---|---|---|
| file | `WorkflowPath` | `WorkflowPath` | `Option<String>` |
| job | `Option<JobId>` | `Option<JobId>` | `Option<String>` |
| step | `Option<StepIndex>` | `Option<StepIndex>` | **absent** |
| line | `Option<u32>` | — | — |
| match | exact | exact (`:39`) | **suffix** (`rule.rs:214-219`) |

`ActionOverride` *is* `Location` minus `line`, plus a version — the identity half,
extracted by hand.

The exact-vs-suffix divergence is **accidental**: neither doc comment
acknowledges the other. Two live consequences:

- **`IgnoreTarget.job` is a silent no-op for per-action rules.**
  `matches_ignore_action` (`rule.rs:273-275`) and `matches_ignore` (`:311-313`)
  hard-`return false` when the target names a job — and the action-hygiene rules
  never populate `job` anyway (`sha_mismatch.rs:36-37`, `stale_comment.rs:42-43`,
  `unpinned.rs:25-26` call only `.with_workflow().with_line()`).
- **`IgnoreTarget` has no `step` axis**, so it cannot address a composite step
  that `ActionOverride` can. The #151 proposal promised both surfaces would
  address composite steps; only overrides got it.

### 6. `tidy/patches.rs:41` — confirmed latent mis-patch

```rust
.find(|(loc, _)| abs_str.ends_with(loc.as_str()))
```

`by_location` is keyed by relative `WorkflowPath` (`scanner.rs:115-123` strips
`repo_root`); `workflows` from `find_workflow_paths()` are absolute
(`discovery.rs:55-66`). Line 41 bridges them by suffix match inside `.find()`
over a **`HashMap`** — nondeterministic iteration order.

For a repo containing both `.github/actions/build/action.yml` and
`.github/actions/x/.github/actions/build/action.yml`, the longer absolute path
matches both keys and `find` returns whichever hash order yields first. #124's
nesting makes collisions likelier.

Needs no new type: `FileScanner::rel_path` (`scanner.rs:115-123`) already
produces exactly the key being looked up. The seam discards it.

Distinct from #153 — that is the `HashMap<ActionId,_>` collapse at `patches.rs:57`
and the unanchored regex at `workflow_update.rs:115`. This one is the *file*-level
lookup, one layer up.

### 7. Corrections to the issue and to prior reports

- **#123 has shipped.** Its scope landed in #151: `Runs{using,steps}`
  (`workflow_parsed/mod.rs:360-369`), gated on `using=="composite"` (`:406-410`),
  `Parsed.steps` (`:332`), scanner consumption (`:181-183`). It is still marked
  OPEN. Close it referencing #151.
- **The issue's stated collapse mechanism is wrong.** It says nested references
  carry `job: None, step: None`. They carry `job: None, step: Some(idx)`
  (`scanner.rs:182`→`:90-95`). So composite steps do *not* collapse today. The
  collapse it describes arises only under the "reaching workflow" option for
  #124. Conclusion right, mechanism wrong.
- **My earlier claim of a `workflow_parsed`↔`workflow_actions` cycle was wrong.**
  That edge is one-way (`workflow_parsed/mod.rs:6` → `workflow_actions`, nothing
  back). Adding `FileKind` to `Location` would *create* the cycle, not collide
  with an existing one. The conclusion — don't put `FileKind` on `Location` —
  survives on better grounds (below).
  A real cycle does exist elsewhere: `action/uses_ref.rs:2` ↔
  `workflow_actions.rs:2`. Legal in Rust; worth noting as one mutually-recursive
  unit.
- **`FileKind` and `Slot` should not merge.** `FileKind` is a *parser* concern —
  which YAML schema, where the steps are (`workflow_parsed/mod.rs:278-288`,
  dispatch at `scanner.rs:174-184`). `Slot` is an *addressing* concern — which
  reference site an override names. They correlate today only because each schema
  has one step-shape; #125 (job-level `uses:`) and GitLab break that. They
  coexist in different modules; `Slot` carries the job/step discriminant natively
  so no `FileKind` import is needed, and the cycle never arises.

## Options

| Approach | Pros | Cons |
|---|---|---|
| **A. Split `Location` only** | One ctor, disjoint readers → pure refactor; ships alone; no user-visible change | Leaves `StepKey` positional, so #124 still persists fragile addresses |
| **B. Split + `StepKey::Named`** | Also closes the persisted-position trap before nesting multiplies it; `Step.id` already parsed | Wider; touches manifest read/write and needs a `gx tidy` rewrite path |
| **C. Full unification (`SiteSelector`)** | Fixes exact-vs-suffix, dead `IgnoreTarget.job`, missing `step` axis | Requires moving `config.rs` policy types first; user-facing config semantics deserve their own change + docs |

## Recommendation

**B, with A's structure and C deferred.**

Smallest structurally coherent change, in a new leaf module:

```
  src/domain/site.rs        NEW — SiteId{file, slot}, Slot, StepKey, Origin
                                  leaf: imports nothing from domain
  workflow_actions.rs             Located{action, site, origin}
                                  WorkflowPath/JobId/StepIndex move here unchanged
```

```rust
pub enum Slot {
    WorkflowStep { job: JobId, step: StepKey },
    CompositeStep { step: StepKey },
    WorkflowJob { job: JobId },      // #125 fits with no other change
}
pub enum StepKey { Named(StepId), Index(StepIndex) }
```

`site.rs` as a **leaf** is the enabling fact — `WorkflowPath`, `JobId`,
`StepIndex` (`workflow_actions.rs:99-200`) are self-contained newtypes with no
`action::` dependency, so both `manifest/overrides.rs` and a later relocated
lint-policy module can depend on it without importing each other or infra.

`Slot` also makes the invariant at `overrides.rs:30-31` ("job-bearing tiers and
file-step cannot collide") type-enforced instead of prose, and removes the
re-derivation at `convert.rs:118-125`.

Fix `patches.rs:41` in the same change — `SiteId.file` gives the exact key, so
the suffix match is deleted rather than patched.

**Must precede #124:** the split, `StepKey::Named`, and moving `Level`/
`IgnoreTarget`/`Rule`/`Lint` out of `config.rs` into a domain policy module
(mechanical; `Level` is imported by 14 files but is a plain enum).

**Follows #124:** `SiteSelector` unification, exact-vs-suffix reconciliation, the
dead `IgnoreTarget.job` fix — user-facing config semantics, own change, own docs.
`ReachedBy` lands *with* #124, since it is the relation traversal introduces.

**On the `Scanner` trait:** its shape does not force a choice. `scan()`
(`workflow.rs:36-38`) returns `Iterator<Item = Result<Located, Error>>`, and
`Located` can gain a field without touching the signature. `extract_steps`
(`scanner.rs:66-98`) already identifies #124's edges — `:82` skips
`action_name.starts_with('.')`. Traversal belongs there, behind the existing
port: currently-discarded information becomes traversal input.

## Open questions

None blocking. One product decision: whether `gx tidy` should auto-rewrite
index-addressed overrides to name-addressed where a `Step.id` exists, or leave
them and warn. That is a migration-policy call.

## Next steps

1. Close #123 referencing #151.
2. Correct #126's stated mechanism (`step: None` → `step: Some(idx)`) and record
   that identity = written-location, reachability = relation.
3. File the `patches.rs:41` nondeterministic lookup and the `IgnoreTarget.job`
   no-op / missing `step` axis as their own bugs.
4. Propose this change (`split-location-identity-provenance`), scoped to
   `SiteId`/`Slot`/`StepKey`/`Origin` + the `patches.rs:41` fix.
