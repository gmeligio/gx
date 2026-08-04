## Why

`gx` decides which schema a managed file follows **twice**: once at discovery
(`discovery::ROOTS`, `src/infra/workflow_scan/discovery.rs:35-46`) and again by walking a
path's ancestors (`FileKind::of_path`, `src/domain/file/parsed/mod.rs:295-309`). A test
exists solely to pin the two together (`composite_tests.rs:299-323`).

The second derivation classifies **by directory location**, so it is wrong by construction
for any action definition outside `.github/actions`. That makes it a hard blocker for #124
(follow local `uses:` edges), whose entire remaining value is reaching composites outside
that directory: a file gx misclassifies there parses under the wrong schema — finding none
of its `uses:` references — and simultaneously draws a false positive from each of the nine
workflow-schema lint rules.

## What Changes

- **Kind is carried, not re-derived.** A managed file's schema is decided once — where the
  file is *found* — and travels with it. `discovery::ManagedFile`
  (`discovery.rs:11-18`) already pairs path with kind; the pairing currently dies at
  `scanner.rs:265-266` because `site::Id` has no kind field.
- **BREAKING (internal API): `FileKind::of_path` is deleted.** Its two production callers
  (`src/infra/manifest/convert.rs:148`, `src/infra/workflow_scan/scanner.rs:224`) receive
  the carried kind instead. This is the point of the change: while the function exists,
  any new call site silently reintroduces the defect.
- **`FileKind` becomes a sum type that owns its schema's fields**, so "not applicable" stops
  being representable as "empty". `Parsed` (`parsed/mod.rs:318-333`) currently carries
  `on`/`permissions`/`concurrency`/`defaults`/`jobs` **and** `steps`, with one half always
  empty and the invariant stated only in a doc comment.
- **The `workflows_full` invariant becomes structural.** `Context.workflows_full`
  (`src/lint/rule.rs:152-159`) documents "**Invariant: workflow files only**" in prose,
  enforced by one `.filter()` at `src/lint/command.rs:69-72`. Delete that filter today and
  nothing fails to compile; nine rules begin flagging every `action.yml` for missing `on:`,
  `permissions:`, and `concurrency:`. The filter becomes a total function whose removal is
  a compile error.
- **Bare-step override validation becomes shape-only.** `convert.rs:147-158` asks `of_path`
  whether a `step`-without-`job` override names an action definition; with the second
  derivation gone and no discovered file set reachable at parse time, that question moves
  out of validation. No net change for users — such an override on a workflow path was
  rejected before and selects nothing now. See design D4.
- The drift-guard test `discovery_kind_agrees_with_of_path`
  (`src/infra/workflow_scan/composite_tests.rs:299-323`) is deleted along with the second
  derivation it guards.

## Capabilities

### New Capabilities

None. This change introduces no new user-facing capability; it corrects behavior governed
by existing specs.

### Modified Capabilities

- `file-discovery`: A managed file's kind is established at discovery and is not re-derived
  from its path afterwards — which is what makes kind correct for a file outside
  `.github/actions`, and is the prerequisite #124 builds on.
- `lint-command`: The workflow-schema rules apply to workflow files only, as a property of
  the type they receive rather than a filter that can be removed without a compile error.
  No rule's *diagnostic* behavior changes on correctly-classified files — this closes a
  false-positive class rather than adding a check.

**Relevance gate.** This passes despite reading as a refactor. The gate excludes "internal
refactoring with no user-visible change"; the user-visible change is that gx reads an action
definition correctly wherever it lives, rather than only under `.github/actions`. Today that
is unreachable and so unobservable; the moment #124 lands it is the difference between
coverage and a wall of false positives on every traversed file. Establishing it as a
spec'd guarantee **before** the change that depends on it is the point — it is a contract
#124 builds on, not an implementation detail of #124.

## Impact

**Blocks:** #124 (follow local `uses:` edges) — cannot ship correctly before this.
**Related:** #155 (name sweep) should land after; #159/#153 are the independent write-side track.

Affected code:

| Area | Files |
|---|---|
| Kind definition & parse | `src/domain/file/parsed/mod.rs` (`FileKind`, `Parsed`, `Parsed::parse`) |
| Addressing | `src/domain/file/site.rs` (`Id` gains kind, or callers receive it alongside) |
| Discovery | `src/infra/workflow_scan/discovery.rs` (`ManagedFile` becomes the sole source) |
| Scanner | `src/infra/workflow_scan/scanner.rs:224` (drops `of_path` call), `:265-266` (preserves kind) |
| Manifest | `src/infra/manifest/convert.rs:147-158` (validation stops reaching into `domain/file/parsed`) |
| Lint | `src/lint/rule.rs` (`Context.workflows_full` type), `src/lint/command.rs:69-72` (filter → total function), nine workflow-schema rules take the narrower type |
| Tests | `composite_tests.rs:299-323` deleted; `parsed/tests.rs:458-461` updated |

No CLI surface, config format, `gx.lock` format, or network behavior changes. No user
migration required.

Out of scope, same root, tracked on #163: an override naming a file gx does not scan is
accepted and then **silently deleted** by the next `gx tidy` (`prune_stale`,
`src/domain/manifest/overrides.rs:126-167`). Not fixable at parse time — `Config::load`
(`src/config.rs:138-141`) parses the manifest before any scan, so no discovered file set
exists there.
