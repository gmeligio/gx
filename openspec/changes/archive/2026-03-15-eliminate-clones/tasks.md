# Tasks: Eliminate Clones via Domain Restructuring

## Phase 1: Type restructuring (domain layer)

### Task 1.1: Delete LockKey, unify with Spec
- [x] Add `Hash` and `Eq` derives to `Spec` (requires `Hash`/`Eq` on `Specifier`)
- [x] Add `Spec::parse()` method (moved from `LockKey::parse()`)
- [x] Delete `LockKey` struct, `From<&Spec> for LockKey`, all imports
- [x] Change `Lock` from `HashMap<LockKey, Entry>` to `HashMap<Spec, Entry>`
- [x] Update `From<&Resolved> for LockKey` → `From<&Resolved> for Spec`
- [x] Update all 16 files importing `LockKey` to use `Spec`
- [x] Update tests in `spec.rs` to test `Spec::parse()` instead of `LockKey::parse()`
- [x] **Verify**: `cargo check` passes

### Task 1.2: Extract ResolvedCommit
- [x] Add `ResolvedCommit { sha, repository, ref_type, date }` in `resolved.rs`
- [x] Restructure `Resolved` to `{ id, version, commit: ResolvedCommit }`
- [x] Update `Resolved::new()` to accept `ResolvedCommit` or construct it internally
- [x] Update `Resolved::with_sha(self, sha)` to consume `self` and use struct update
- [x] Update all callers accessing `resolved.sha` → `resolved.commit.sha`, etc.
- [x] Update `Resolved` tests
- [x] **Verify**: `cargo check` passes

### Task 1.3: Restructure Entry to use ResolvedCommit
- [x] Change `Entry` to `{ commit: ResolvedCommit, version, comment }`
- [x] Update `Entry::new()` and `Entry::with_version_and_comment()` constructors
- [x] Update `is_complete()` destructuring
- [x] Update all callers accessing `entry.sha` → `entry.commit.sha`, etc.
- [x] Update lock serialization/deserialization (flatten `ResolvedCommit` fields in TOML)
- [x] Update `Entry` tests
- [x] **Verify**: `cargo check` passes, `cargo test` passes for lock roundtrip

### Task 1.4: Compose Located from InterpretedRef
- [x] Change `Located` to `{ action: InterpretedRef, location: Location }`
- [x] Delete `ActionSet::add_located()`, keep only `add(&mut self, &InterpretedRef)`
- [x] Change `ActionSet::from_located` to delegate to `add()`
- [x] Update all callers: `located.id` → `located.action.id`, etc.
- [x] Update tests in `workflow_actions.rs`, overrides, lint, patches, scan
- [x] **Verify**: `cargo check` passes, `cargo test --lib` passes

## Phase 2: Ownership transfer (cross-cutting)

### Task 2.1: Ownership in resolution and lock building
- [x] Change `Lock::set()` to take `Resolved` by value (ownership transfer)
- [x] Add `Entry::from_commit()` for zero-clone construction from `Commit`
- [x] Remove `From<&Resolved> for Spec` (no longer needed)
- [x] Update all callers: `lock.set(&resolved)` → `lock.set(resolved)`
- [x] **Verify**: `cargo test` passes, `cargo clippy` clean

### Task 2.2: Ownership in report building
- [x] Change `TidyReport` fields: `removed: Vec<ActionId>`, `added: Vec<(ActionId, Specifier)>`, `upgraded: Vec<(ActionId, String, Specifier)>`
- [x] Update `render()` to call `.to_string()` only at output time
- [x] Update report builder to move domain types instead of `.to_string()` at construction
- [x] Update report tests
- [x] **Verify**: `cargo test` passes

## Phase 3: Lint guard

### Task 3.1: Add redundant_clone lint
- [x] Add `redundant_clone = "deny"` to `[lints.clippy]` in `Cargo.toml`
- [x] Fix 2 existing violations (`workflow_update.rs`, `lock/mod.rs` test)
- [x] No false positives found
- [x] **Verify**: `cargo clippy` passes

## Phase 4: Validation

### Task 4.1: Full test suite
- [x] Run `cargo test` — 288 lib tests pass
- [x] Run `cargo clippy` — no warnings
- [x] Verify clone count reduction: 173 → 145 clones (−28, 16% reduction), 30 → 28 files
