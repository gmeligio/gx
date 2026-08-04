## Why

`Location` (`src/domain/workflow_actions.rs:203-214`) carries identity
(`workflow`, `job`, `step` — exact-matched to resolve overrides) and provenance
(`line` — diagnostics only) in one struct. It derives `PartialEq, Eq` but not
`Hash`, because `line` would poison it, so every override lookup is a linear
field-by-field scan (`src/domain/manifest/overrides.rs:38-73`) rather than a map
hit.

#124 (follow local `uses:` edges into composite actions) makes a reference
reachable from several files. Issue #126 frames this as a choice between the two
roles — but both options it presents are correctness regressions. Identity is
single-valued (where a reference is written); reachability is a relation
(many-valued, cannot be a key field). Splitting the struct now means #124 adds a
set to a correct model instead of overloading a field further.

## What Changes

- New leaf domain module `src/domain/site.rs` holding `SiteId`, `Slot`,
  `StepKey`, and `Origin`.
- `SiteId { file, slot }` becomes the identity — `Hash + Eq`, usable as a map
  key.
- `Slot` replaces the `(Option<JobId>, Option<StepIndex>)` encoding with a sum
  type: `WorkflowStep { job, step }`, `CompositeStep { step }`,
  `WorkflowJob { job }`. This makes the invariant currently stated in prose at
  `overrides.rs:30-31` ("job-bearing tiers and file-step cannot collide")
  type-enforced, and removes the kind re-derivation at
  `src/infra/manifest/convert.rs:118-125`.
- Addressing is unchanged: `Slot` carries `StepIndex`, the same zero-based
  position gx uses today.
- `Scope` replaces `ActionOverride`'s `(Option<JobId>, Option<StepIndex>)` pair
  with `File | Job | JobStep | CompositeStep`. An override is a *selector* — a
  job-scoped one matches every step in that job — so it is set-valued where
  `Slot` is a point. But the `Option` pair can represent four combinations when
  only three are meaningful: `(None, Some)` means "a composite step" on an
  action file and nothing coherent on a workflow file. Resolving that ambiguity
  is what forces `convert.rs:118-125` to call `FileKind::of_path` — a
  path-classification rule — to validate a *scope*. With `Scope`, the invalid
  combination is unrepresentable and the rejection becomes a parse error about
  the user's input.
- `Origin { line }` carries provenance, separate from identity.
- `WorkflowPath`, `JobId`, `StepIndex` (`workflow_actions.rs:99-200`) move to
  `site.rs` unchanged — they are self-contained newtypes with no `action::`
  dependency.
- Fixes #161: `compute_workflow_patches` (`src/tidy/patches.rs:39-42`) currently
  pairs files to pins by suffix-matching an absolute path against a relative one
  inside `.find()` over a `HashMap`, so pairing is nondeterministic when two keys
  match. `SiteId.file` gives the exact key, deleting the suffix match.

Not in scope, deliberately:

- `ReachedBy` — the reachability relation. Lands with #124, since traversal is
  what introduces it.
- `SiteSelector` unifying override addressing with lint ignores — blocked on
  moving policy types out of `src/config.rs`, and is user-facing config
  semantics deserving its own change.
- Any change to how steps are addressed. Addressing steps by `Step.id` instead
  of by index was considered and rejected: `id` is optional and absent on most
  steps, so it could only be a second address form carried permanently alongside
  the index, and renaming an `id` breaks a name-addressed override exactly as
  inserting a step breaks an index-addressed one. The underlying fragility —
  an override silently retargeting to a different action — is #163, fixed by
  cross-checking the override's `ActionId` against the action at its address,
  which needs no new address form.
- #153 (write path discards per-location resolution), #162 (ignore targets
  cannot address a step), #163 (overrides not validated against their address).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `file-discovery`: adds a requirement that each file is rewritten with the pins
  from its own references, matched by exact path rather than by suffix over an
  unordered map. This is the #161 fix.

Per the relevance gate, the type refactor itself is internal — override
resolution, diagnostics, and `gx.toml` read/write semantics are all preserved,
and the requirement that composite steps are addressed "by its zero-based index
within `runs.steps`" (`openspec/specs/file-discovery/spec.md:55-58`) holds
unchanged, including its scenario
`{ workflow = ".github/actions/setup/action.yml", step = 0, version = "^3" }`.

The #161 fix does change user-visible behavior and so does need a spec: a file
could previously be rewritten with a different file's pins, writing a version
the user never referenced there, with nothing reporting it and a repeat run
potentially differing. It sits alongside the existing "Every discovered file is
a candidate for rewriting" requirement (`:150`) — that one guarantees every file
gets written, the new one guarantees it gets written with the right content.

The one behavior change is #161: file-to-pins pairing becomes deterministic.
That is a bug fix with an obvious solution, which the gate also places outside
spec scope.

## Impact

**Code.** One production constructor of `Location`
(`src/infra/workflow_scan/scanner.rs:90-95`) and five consumer sites:
`src/domain/manifest/mod.rs:57`, `src/domain/manifest/overrides.rs:33-76`
/`:121-125`/`:168-172`, `src/tidy/patches.rs:29`/`:59`, three lint rules
(`sha_mismatch.rs:36-37`, `stale_comment.rs:42-43`, `unpinned.rs:25-26`), and
`src/lint/rule.rs:294`. No consumer reads both halves, so the split follows an
existing cleavage plane.

`src/infra/manifest/convert.rs` gains `Slot` construction on read and
destructuring on write; the `FileKind::of_path` validation at `:118-125` is
deleted.

**Module graph.** `site.rs` is a leaf — it imports nothing from `domain`. This is
what lets `manifest/overrides.rs` and, later, a relocated lint-policy module both
depend on it without importing each other or `infra`.

**Ports.** None change. `Scanner::scan` (`src/domain/workflow.rs:36-38`) returns
`Iterator<Item = Result<Located, Error>>`; `Located` changes shape, the signature
does not.

**Unblocks.** #124 (adds `ReachedBy` to a correct model), #154 (kind on domain
types), #155 (renames the residue).

**Users.** No migration. Existing `gx.toml` overrides and lint `ignore` entries
keep matching exactly what they match today.
