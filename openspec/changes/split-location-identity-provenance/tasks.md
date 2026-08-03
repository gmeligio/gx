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

- [ ] 3.1 Change `Located` (`workflow_actions.rs:217-222`) to `{ action, site: SiteId, origin: Origin }`
- [ ] 3.2 Update the sole production constructor at `src/infra/workflow_scan/scanner.rs:90-95` to build `SiteId` + `Origin`, choosing the `Slot` variant from the file kind already in scope
- [ ] 3.3 Delete `Location`
- [ ] 3.4 Confirm the compiler surfaces every consumer; the split is complete only when `mise run build` is clean

## 4. Migrate identity consumers

- [ ] 4.1 `src/domain/manifest/overrides.rs:38-73` — match on `Slot` instead of inferring composite from `job.is_none() && step.is_some()` at `:59-61`
- [ ] 4.2 Delete the prose-only collision argument at `overrides.rs:30-31`; the `Slot` variants now make it hold by construction
- [ ] 4.3 `overrides.rs:121-125` (`sync`) and `:168-172` (`prune_stale`) — key on `SiteId`
- [ ] 4.4 `src/domain/manifest/mod.rs:57` — update to `SiteId`
- [ ] 4.5 Confirm existing tests in `overrides.rs` and `overrides_composite_tests.rs` pass with assertions unchanged — this is the behavior-preservation net

## 5. Migrate provenance consumers

- [ ] 5.1 `src/lint/sha_mismatch.rs:36-37`, `stale_comment.rs:42-43`, `unpinned.rs:25-26` — read `Origin.line` and `SiteId.file`
- [ ] 5.2 `src/lint/rule.rs:294` — read `SiteId.file`
- [ ] 5.3 Confirm diagnostic output is byte-identical: no `file:line` rendering changes

## 6. Manifest boundary

- [ ] 6.1 `src/infra/manifest/convert.rs` — construct `Slot` on read, destructure on write
- [ ] 6.2 Delete the `FileKind::of_path` validation at `convert.rs:118-125`; the invalid combination is now unrepresentable
- [ ] 6.3 Move the rejection of `step` without `job` on a workflow file into parsing, preserving the exact user-facing error text
- [ ] 6.4 Test: that rejection still errors with the same message as today
- [ ] 6.5 Test: a `gx.toml` with file-, job-, and step-scoped overrides round-trips byte-identically

## 7. Fix #161

- [ ] 7.1 Write the failing test first: fixture repo with `.github/actions/build/action.yml` and nested `.github/actions/x/.github/actions/build/action.yml`, asserting each file is paired with its own pins. Assert the exact pairing rather than looping — the bug depends on `HashMap` order and is not reliably reproducible
- [ ] 7.2 Thread the relative path from `FileScanner::rel_path` (`scanner.rs:115-123`) through to `compute_workflow_patches`
- [ ] 7.3 Replace the suffix match at `src/tidy/patches.rs:39-42` with an exact `SiteId.file` lookup; delete the `.find()` over the `HashMap`
- [ ] 7.4 Test: a discovered file with no managed references is left unchanged and not counted in the summary
- [ ] 7.5 Test: repeated runs pair identically

## 8. Verify

- [ ] 8.1 `mise run test` — full suite green
- [ ] 8.2 `mise run lint` — clippy pedantic clean, including private-item and field docs on the new types
- [ ] 8.3 Confirm no `gx.toml` or `gx.lock` format change: run `gx tidy` on a fixture repo before and after, diff the outputs
- [ ] 8.4 Update `README.md` / `docs/demo.tape` only if user-facing output changed — expected to be no-ops here, since only #161's pairing changes and it produces no new output
