---
name: gx-architecture
description: Use when working on gx source code structure, understanding layer boundaries, data flow, tidy modes, or the init command
---

# gx Architecture

## Layers

Presentation → Application Dispatcher → Command Handlers → Domain/Infrastructure

- **Presentation**: CLI parsing only, no logic
- **Application Dispatcher**: constructs stores and dispatches to commands
- **Command Handlers**: orchestrate business operations via traits (generic over store implementations)
- **Domain**: pure business types and resolution logic, no I/O
- **Infrastructure**: all I/O (filesystem, GitHub API); implements domain traits

## Key Paradigms

- **Trait-based stores**: commands are generic over `ManifestStore` and `LockStore`, enabling file-backed (persistent) vs. memory-only (transient) modes without branching in business logic
- **Two tidy modes**: if a manifest exists, use file-backed stores (persist to disk); otherwise use memory-only stores (update workflows only, no manifest/lock written)
- **Interpretation pipeline**: raw workflow `uses:` strings are parsed → interpreted into structured refs → resolved to pinned SHAs via a registry trait

## Tidy Data Flow

1. Discover repo root (anchored by `.github/` presence)
2. Parse all workflows → extract and interpret action refs → aggregate into a set
3. Sync manifest: add new actions, remove stale ones (highest semver wins on conflict)
4. Resolve each action version to a commit SHA via the version registry
5. Write updated lock and workflows (skipped in memory-only mode)

## Init vs Tidy

- **init**: bootstraps manifest + lock from current workflow state; refuses to overwrite an existing manifest
- **tidy**: idempotent reconciliation — brings manifest, lock, and workflows into sync
