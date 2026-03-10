# Semver Specifiers

## Problem

The manifest (`gx.toml`) uses v-prefixed version strings like `"v6"` that implicitly map to semver ranges (`^6`). Users must know these implicit rules. The specifier is derived at runtime and stored redundantly in the lock file.

## Solution

Replace v-prefixed versions in the manifest with explicit semver specifiers (`"^6"`, `"~1.15.2"`). Add a `[gx]` section with `min_version`. Store the human-readable version comment in the lock file. Drop the redundant `specifier` field from the lock. Use `semver::VersionReq` for range matching instead of hand-rolled logic.

## Scope

**Breaking change** — manifest format v2, lock format v1.4. Auto-migration on first write command.

### In scope

- New `Specifier` domain type wrapping `semver::VersionReq`
- Manifest format: `[gx] min_version`, specifier values (`"^6"`, `"~1.15.2"`, `"main"`)
- Lock format v1.4: replace `specifier` field with `comment` field, rekey entries with specifier
- Overrides: keep `version` field name, value becomes specifier string
- Write-time auto-migration (Cargo-style): parser reads any version, serializer writes current
- `Parsed<T>` wrapper for migration signaling
- Upgrade rearchitecture: `UpgradeAction::CrossRange` produces `new_specifier` + `new_comment`
- Replace hand-rolled range logic with `semver::VersionReq::matches()`
- Update existing specs (`lock-format`, `manifest-authority`)

### Out of scope

- Advanced semver operators (`>=`, `<`, `*`, ranges) — future work
- Explicit `gx migrate` CLI command — migration happens transparently on write
- Lock file `revision` field (UV-style two-tier versioning) — future if needed
