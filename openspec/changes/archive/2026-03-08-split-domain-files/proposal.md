## Why

Four domain files exceed the 500-line budget: `action/identity.rs` (759), `manifest.rs` (725), `lock.rs` (581), and `resolution.rs` (573). Additionally, `src/domain/` has 10 direct `.rs` files against a target of 8. These files mix distinct concerns (e.g., `Specifier` parsing is bundled with `ActionId`/`Version`/`CommitSha` in identity.rs; override logic is mixed with basic CRUD in manifest.rs).

## What Changes

- **Split `domain/action/identity.rs`**: Extract `Specifier` to its own file.
- **Convert `domain/manifest.rs` → `domain/manifest/`**: Core entity in `mod.rs`, override logic in `overrides.rs`.
- **Convert `domain/lock.rs` → `domain/lock/`**: Core entity in `mod.rs`, `LockEntry` in `entry.rs`.
- **Split `domain/resolution.rs`**: Extract `ShaIndex` and tag selection helpers.
- **Reduce direct file count**: Moving `manifest.rs` and `lock.rs` into subdirectories reduces the direct `.rs` file count in `domain/` to ≤8.

## Capabilities

### New Capabilities

_(None — purely structural.)_

### Modified Capabilities

_(No existing capabilities are changing — these are internal structural improvements.)_

## Impact

- **Domain layer** (`src/domain/`): Internal reorganization. All public types and methods unchanged.
- **Direct file count in `domain/`**: Drops from 10 to 8 (manifest.rs and lock.rs become directories).
- **No changes to infra, command, or test layers** (import paths unchanged via re-exports from `domain/mod.rs`).
- **No user-facing changes**.
