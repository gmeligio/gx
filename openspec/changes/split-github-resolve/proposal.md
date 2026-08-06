## Why

`src/infra/github/resolve.rs` sits at 438 logic lines against the 440-line budget
and 544 total against 550 (both enforced by `tests/code_health.rs`). It is one
file holding three unrelated jobs, and it has no room left. Three queued issues
all need to add code to exactly this file and are hard-blocked until it is split:

- **#137** — retry/backoff, which wraps every HTTP send in the file.
- **#145** — a GitLab registry, which needs the same request/response plumbing.
- **#141** — error redesign, which touches every `map_err` in the file.

Splitting now unblocks all three. Raising the budget is not an option: the budget
is the mechanism that surfaced the problem.

## What Changes

Move code only. No signature, no control flow, and no HTTP call sequence changes.

- Extract the repeated `authenticated_get → send → check_status → json` sequence
  — written out eight times in the file — into a single generic helper on
  `Registry`. This is the one seam already present in the code, not a new
  abstraction.
- Split the remaining methods into three files that each name one job:
  - ref resolution and the tag → release → branch → commit fallback chain,
  - tag enumeration (tags-for-SHA, version tags, pagination, annotated-tag
    dereferencing),
  - date lookups (commit, release, tag dates).
- `src/infra/github/mod.rs` gains the new `mod` declarations. Its public
  reexports (`Error`, `Registry`) are unchanged.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None.

Per the relevance gate in `openspec/config.yaml`, this change is checked against
each gate explicitly:

- *"Requires spec: adds, removes, or changes user-facing behavior"* — **not met.**
  The module's public surface is `Error` and `Registry`; both keep identical
  signatures and identical behavior. The `pub` methods `resolve_ref`,
  `get_tags_for_sha`, and `get_version_tags` are moved between files but not
  altered. The same GitHub API endpoints are called in the same order with the
  same error mapping, so CLI output, exit codes, and `gx.lock` contents are
  byte-identical before and after.
- *"Requires spec: introduces a new domain concept that changes what users can
  do"* — **not met.** No new type, command, flag, or config key.
- *"Skip spec: internal refactoring with no user-visible change"* — **met.** This
  is a file-boundary move within one private module.

The change therefore declares `skip_specs: true`. The relevant behavior is
already specified in `openspec/specs/action-resolution/spec.md`, which this
change must not alter — an unchanged spec passing unchanged tests is the
evidence that the refactor is behavior-preserving.

## Impact

- `src/infra/github/resolve.rs` — split; the name is retired or narrowed.
- `src/infra/github/mod.rs` — module declarations added, reexports unchanged.
- `src/infra/github/registry.rs` — gains the shared request helper alongside the
  existing `authenticated_get` / `check_status` it already owns.
- No change to `src/domain/`, `src/lint/`, `src/audit/`, or any command module.
- No dependency, CI, or packaging change.
- Test evidence: existing unit tests and `tests/e2e_github.rs` must pass
  unmodified, and `mise run test` must pass without any budget in
  `tests/code_health.rs` being raised.
