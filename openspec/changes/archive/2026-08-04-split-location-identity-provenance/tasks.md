## 1. Create the leaf module

- [x] 1.1 Create `src/domain/file/site.rs` and register `file` in `src/domain/mod.rs`
- [x] 1.2 Move `WorkflowPath`, `JobId`, `StepIndex` from `workflow_actions.rs:99-200` into `site.rs` unchanged; update importers to `domain::file::site` directly (a `pub use` shim is rejected by `clippy::pub_use`)
- [x] 1.3 Verify `site.rs` imports nothing from `crate::domain` — the leaf property the follow-up work depends on
- [x] 1.4 Confirm `mise run test` passes with only the move applied
- [x] 1.5 Group the managed-file modules under `src/domain/file/`: `workflow_parsed/` → `parsed/`, `workflow_actions.rs` → `actions.rs`, `workflow.rs` → `scan.rs`, plus `site.rs`. Required — `src/domain/` was at the 8-file budget enforced by `tests/code_health.rs:626`

## 2. Define the new types

- [x] 2.1 Add `Slot` with variants `WorkflowStep { job, step }`, `CompositeStep { step }`, `WorkflowJob { job }`
- [x] 2.2 Add `SiteId { file: WorkflowPath, slot: Slot }` deriving `Hash + PartialEq + Eq + Clone + Debug`
- [x] 2.3 Add `Origin { line: Option<u32> }`
- [x] 2.4 Unit test: two `SiteId`s equal under differing `Origin.line` compare AND hash equal — the invariant `Location` could not state
- [x] 2.5 Unit test: `Slot` construction from each schema — workflow step yields `WorkflowStep`, composite step yields `CompositeStep`

## 3. Switch `Located` over

- [x] 3.1 Change `Located` (`file/actions.rs`) to `{ action, site: SiteId, origin: Origin }`
- [x] 3.2 Update the sole production constructor at `scanner.rs` (`extract_steps` now takes a `slot_for` closure, chosen by the existing `match kind`) to build `SiteId` + `Origin`, choosing the `Slot` variant from the file kind already in scope
- [x] 3.3 Delete `Location`
- [x] 3.4 Confirm the compiler surfaces every consumer; the split is complete only when `mise run build` is clean

## 4. Migrate identity consumers

- [x] 4.1 `src/domain/manifest/overrides.rs:38-73` — match on `Slot` instead of inferring composite from `job.is_none() && step.is_some()` at `:59-61`
- [x] 4.2 Delete the prose-only collision argument at `overrides.rs:30-31`; the `Slot` variants now make it hold by construction
- [x] 4.3 `overrides.rs:121-125` (`sync`) and `:168-172` (`prune_stale`) — key on `SiteId`
- [x] 4.4 `src/domain/manifest/mod.rs:57` — update to `SiteId`
- [x] 4.5 Confirm existing tests in `overrides.rs` and `overrides_composite_tests.rs` pass with assertions unchanged — this is the behavior-preservation net

## 5. Migrate provenance consumers

- [x] 5.1 `src/lint/sha_mismatch.rs:36-37`, `stale_comment.rs:42-43`, `unpinned.rs:25-26` — read `Origin.line` and `SiteId.file`
- [x] 5.2 `src/lint/rule.rs:294` — read `SiteId.file`
- [x] 5.3 Confirm diagnostic output is byte-identical: no `file:line` rendering changes

## 6. Manifest boundary

- [x] 6.1 Add `Scope { File | Job | JobStep | CompositeStep }` to `domain/file/site.rs`; replace `ActionOverride`'s `(Option<JobId>, Option<StepIndex>)` pair with it
- [x] 6.2 Rewrite `addresses`, `override_for`, and `resolve_version` in `overrides.rs` to match `Scope` against `Slot` directly, dropping the `scope_of` Option-pair conversion
- [x] 6.3 `src/infra/manifest/convert.rs` — build `Scope` from the TOML `(job, step)` pair; reject `step`-without-`job` there as a parse error about the user's input, preserving the exact message text
- [x] 6.4 `of_path` stays at `convert.rs:146`, narrowed to the one branch that needs it. Deciding whether a job-less step is a composite step or nonsense genuinely requires the file kind; `Scope` removes the *representable* invalid state, not the need to classify. The design's claim that this call disappears was wrong
- [x] 6.5 `src/infra/manifest/patch.rs` — destructure `Scope` on write; TOML keys stay `workflow`/`job`/`step`
- [x] 6.6 Test: `step` without `job` on a workflow file still errors with the same message (no existing coverage — this is new)
- [x] 6.7 Test: a `gx.toml` with file-, job-, and step-scoped overrides round-trips byte-identically

## 7. Fix #161

- [x] 7.1 Two tests, because the end-to-end one cannot prove the fix on its own: `gx_tidy_pins_each_file_from_its_own_references` (integ_tidy) asserts each file keeps its own pins, and `repo_rel_distinguishes_paths_that_share_a_suffix` (unit) pins the seam and asserts the paths genuinely collide under suffix matching. The integration test passes under the old code too — the mispair direction depends on `HashMap` order, so only the seam test is a real regression guard
- [x] 7.2 Thread the relative path from `FileScanner::rel_path` (`scanner.rs:115-123`) through to `compute_workflow_patches`
- [x] 7.3 Replace the suffix match at `src/tidy/patches.rs:39-42` with an exact `SiteId.file` lookup; delete the `.find()` over the `HashMap`
- [x] 7.4 Test: a discovered file with no managed references is left unchanged and not counted in the summary
- [x] 7.5 Test: repeated runs pair identically

## 8. Verify

- [x] 8.1 `mise run test` — full suite green
- [x] 8.2 `mise run lint` — clippy pedantic clean, including private-item and field docs on the new types
- [x] 8.3 Confirmed against a binary built from the base commit (d6af70b) in a scratch worktree, not just self-consistency: a fixture with file-, job-, and composite-step-scoped overrides produces byte-identical `gx.toml`, `gx.lock`, `ci.yml`, and `action.yml` under both binaries
- [x] 8.4 Update `README.md` / `docs/demo.tape` only if user-facing output changed — confirmed a no-op: `README.md` and `docs/` reference no internal type names, and #161's pairing fix produces no new output
