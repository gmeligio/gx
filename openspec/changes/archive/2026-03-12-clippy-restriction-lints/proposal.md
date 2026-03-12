## Why

The codebase uses `pedantic`, `perf`, and `nursery` clippy groups at `deny` level but has no restriction lints enabled. Restriction lints catch real safety issues (unwrap in production, silent Result discard, panicking indexing) and enforce style consistency (`.to_owned()` over `.to_string()` on `&str`, no `test_` prefix). Adopting them now prevents technical debt while the codebase is still small (~3k lines).

## What Changes

- Replace blanket `restriction = "deny"` with individually selected restriction lints in `Cargo.toml`
- Deny 35+ restriction lints globally, with targeted `#[allow]` in test modules for safety lints (`unwrap_used`, `expect_used`, `indexing_slicing`, etc.)
- Bulk-fix all mechanical violations:
  - `.to_string()` on `&str` to `.to_owned()` (~341 hits)
  - Remove `test_` prefix from `#[test]` functions (~203 hits)
  - Separate literal suffixes: `0u8` to `0_u8` (~3 hits)
  - `use Trait as _` for anonymous trait imports (~21 hits)
  - Add terminal punctuation to doc paragraphs (~140 hits)
  - Add missing doc comments on private items (~64+ hits)
  - Replace `.ok()` with explicit `let _ =` or proper error handling (~7 hits)
  - Replace `if cond { Some(x) } else { None }` with `.then_some()` (~2 hits)
  - Eliminate variable shadowing (~7 hits)
  - Replace `as` conversions with `From`/`Into` (~3 hits)
  - Add `reason = ".."` to all `#[allow]` attributes (~5 hits)
  - Replace `#[allow]` with `#[expect]` (~4 hits)
  - Use `.get()` instead of indexing in production code (~15-20 prod hits)
  - Replace `&str[n..]` with `.get(n..)` or `.strip_prefix()` (~2 hits)

## Capabilities

### New Capabilities
- `clippy-restriction-config`: Defines which clippy restriction lints are denied globally, which are denied in production only (allowed in tests), and which are explicitly allowed. Establishes the lint configuration contract in `Cargo.toml`.

### Modified Capabilities
- `architecture-guardrails`: Adds clippy restriction lint rules to the project's quality guardrails.

## Impact

- **Cargo.toml**: New `[lints.clippy]` entries for ~35 individual restriction lints
- **All `src/**/*.rs` files**: Mechanical code fixes (string methods, doc comments, test renames, indexing, shadowing, etc.)
- **All test modules** (`#[cfg(test)]` blocks and `tests/**/*.rs`): `#[allow(...)]` annotations for safety lints
- **CI**: No changes needed — `mise run clippy` already runs `cargo clippy --tests`
- **No breaking API changes**: All changes are internal code quality improvements
