# Proposal: Lazy Pipeline Architecture

## Problem

The current tidy/init/lint pipeline is eager throughout — all workflows are scanned and collected into `Vec<LocatedAction>` before any processing, domain types expose `Vec` return types that force callers to collect, manifest and lock files are always fully rewritten regardless of what changed, and commands mutate domain objects in-place then return full state.

This causes three problems:

1. **Large testing surface for writes.** `format_manifest_toml()` and `serialize_lock()` serialize every entry on every write. A bug in serialization can corrupt entries that were never intended to be modified. Every possible entry state must be tested because every entry passes through the write path.

2. **Unnecessary work.** Most `gx tidy` runs change 0-2 entries, but the pipeline scans all files, builds multiple intermediate `Vec`/`HashMap` collections, serializes all entries, and writes all files. There are 17+ eager collection points where `Vec` is allocated just to be iterated.

3. **Poor composability.** The `WorkflowScanner` trait returns `Vec<LocatedAction>`, forcing all consumers to pay the cost of a full scan. Lint can't work file-by-file. Future commands (e.g. `gx check ci.yml`) can't scan a single file. The pipeline can't short-circuit on no-ops.

## Solution

Restructure the pipeline around three principles:

1. **Iterator-based sources.** Domain accessors and the workflow scanner return iterators. Consumers collect only what they need into the shape they need.

2. **Plan-based commands.** Commands borrow state read-only and produce a plan (changeset) describing what should change. They never mutate domain objects.

3. **Surgical writes via toml_edit.** The apply layer patches manifest/lock files using `toml_edit`, touching only the entries described in the plan. Untouched entries never pass through gx serialization code.

## Non-goals

- Changing CLI interface or user-facing behavior
- Lazy-loading manifest/lock at startup (they're small files — not worth the complexity)
- Making `VersionRegistry` return iterators (paginated API results must buffer)
- Rewriting the workflow updater (already only writes changed files)

## Outcome

After this change:
- `gx tidy` with nothing to change: zero file writes, zero network calls
- `gx tidy` with one new action: one manifest insert, one lock insert, affected workflows updated
- `gx lint` can work file-by-file for file-local rules
- Commands are pure functions from `(&Manifest, &Lock, scanner) → Plan`
- Plans are inspectable, testable, and enable `--dry-run` for free
- `format_manifest_toml()` and `serialize_lock()` are only used for `init` (creating files from scratch)
