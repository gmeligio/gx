## Context

The project is a single-crate Rust CLI (`gx`) with a layered architecture: `domain/` (pure types), `infra/` (I/O, GitHub API, serialization), and command modules (`tidy/`, `upgrade/`, `lint/`, `init/`). The domain layer has become anemic — types like `Manifest` and `Lock` are data holders with CRUD methods, while business logic lives as private helper functions in command modules. This has caused function duplication (`diff_manifests` and `diff_locks` exist in both tidy and upgrade with divergent behavior) and testing friction (domain logic requires filesystem fixtures because it's tested through command orchestrators).

The `on_progress: &mut dyn FnMut(&str)` callback is threaded through domain-adjacent functions for real-time spinner updates in the terminal. This couples domain logic to presentation concerns.

The project uses `mise` for task running and already has a `tests/code_health.rs` pattern for structural assertions.

## Goals / Non-Goals

**Goals:**
- Domain types own their business logic — testable without I/O or fixtures
- No duplicated logic between command modules
- Architecture guardrails that catch regression automatically
- Smaller files with clear single responsibilities
- Command modules are thin orchestrators that format domain events for display
- Work is scoped for parallel subagent execution where possible

**Non-Goals:**
- Changing CLI behavior, manifest format, or lock file format
- Introducing new crates or workspace restructuring
- Rewriting tests from scratch (migrate existing tests, adjust as needed)
- Adding a full ArchUnit-style framework (use simple source-level tests)

## Decisions

### 1. Domain methods return `Vec<SyncEvent>` instead of accepting `on_progress`

**Decision**: Domain methods like `Manifest::sync_with_workflows()` return a `Vec<SyncEvent>` enum. The command orchestrator iterates events and calls `on_progress` per-phase.

**Alternatives considered**:
- **Keep `on_progress` callback** (status quo): Zero cost, but leaks presentation into domain. Tests must capture or ignore callbacks.
- **Observer trait**: Strongly typed but over-engineered for string messages. More boilerplate than the callback.
- **Derive from diff only** (no events): Simplest domain API but loses context — can't express "SHA upgraded to tag" vs "action added", or recoverable warnings.
- **Iterator-based streaming**: Real-time and pure, but Rust lifetime constraints with `&mut self` + iterator make it impractical.

**Rationale**: Events are testable data (`assert!(events.contains(...))`), keep domain pure, enable future structured output (JSON logs), and the per-phase granularity (not per-action) is acceptable for spinner UX.

### 2. `Manifest::diff()` and `Lock::diff()` as domain methods

**Decision**: Unify the two divergent `diff_manifests` implementations into `Manifest::diff(&self, other: &Manifest) -> ManifestDiff`. Same for `Lock::diff()`. The upgrade variant that detects SHA changes (same key, different SHA → replace) becomes the canonical behavior.

**Rationale**: This eliminates duplication and the divergence bug. Both tidy and upgrade call the same method. The richer diff (detecting SHA replacements) is correct for both use cases.

### 3. Architecture guardrails in `code_health.rs`, not a custom lint

**Decision**: Add architecture tests as Rust integration tests in the existing `tests/code_health.rs` file, scanning source files for `use` statements and function signatures. Add a `mise` task for file/folder size budgets.

**Alternatives considered**:
- **cargo-dylint custom lint**: Proper Clippy-level lint with IDE integration, but enormous setup cost for simple checks.
- **cargo-modules --acyclic**: Detects cycles but not layer violations or anemic domain patterns.
- **Separate tool/script**: Possible, but the `code_health.rs` pattern already exists and runs with `cargo test`.

**Rationale**: Source-level scanning of `use` statements is crude but effective for the three rules we need: layer direction, duplicate functions, and size budgets. No new dependencies. Runs as part of the existing test suite.

### 4. Tidy tests move to `tidy/tests.rs`, not alongside split submodules

**Decision**: The ~1230 lines of integration tests that exercise `plan()` move to `tidy/tests.rs` as a sibling test module. Unit tests for individual functions stay colocated in their new submodule files.

**Rationale**: Most tidy tests call `plan()` — they test the public API, not individual helpers. Scattering them across submodules would be artificial. The `tests.rs` sibling pattern keeps tests near the API they exercise while reducing `mod.rs` to ~250 lines of logic.

### 5. `SyncEvent` is a single enum shared via domain, not per-command enums

**Decision**: One `SyncEvent` enum in `domain/` covers events from manifest sync, lock sync, and version correction. Each command's orchestrator formats events independently using `Display` or pattern matching.

**Rationale**: The events are domain concepts (action added, version corrected, SHA upgraded, resolution skipped). Tidy and upgrade both produce and consume these events. Separate enums would duplicate variants. Each command's orchestrator can ignore irrelevant variants.

### 6. Phase ordering for subagent parallelism

**Decision**: Four implementation phases with defined dependencies:

```
Phase 1: Guardrails ──────────────────── (independent)
Phase 2: Domain enrichment ──────────── (independent of Phase 1)
Phase 3: Split tidy ─────────────────── (depends on Phase 2)
Phase 4: Split infra ────────────────── (independent of Phases 2-3)
```

Within Phase 2, individual domain method additions (Manifest::diff, Lock::diff, sync_overrides, etc.) can be developed in parallel worktree agents if merged sequentially.

## Risks / Trade-offs

- **Per-phase vs per-action spinner updates**: Domain methods return events after completing a full phase. The spinner won't update per-action within a phase (e.g., during `sync_manifest_actions` iterating 50 actions). Phases typically take 1-5 seconds, so this is acceptable. → If UX degrades noticeably, the orchestrator can split calls into smaller batches.

- **Test migration effort**: Moving tidy tests to `tests.rs` and adjusting after domain enrichment is mechanical but tedious. → Subagent parallelism helps. Tests that already exercise `plan()` won't change structurally — they'll just have simpler setup once domain methods handle logic directly.

- **Upgrade's `diff_locks` is richer than tidy's**: Upgrade detects SHA replacements (same key, different SHA), tidy doesn't. Unifying to the richer version means tidy's diff may include "replaced" entries it previously ignored. → This is correct behavior. The apply phase already handles replacements via remove+add.

- **`SyncEvent` enum growth**: As domain methods grow, the event enum could bloat. → Keep it to observable domain transitions. If it exceeds ~10 variants, consider sub-enums per domain entity.
