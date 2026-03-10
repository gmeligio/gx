# Design: Semver Specifiers

## Three-Concept Model

The system separates three concerns previously conflated in `Version`:

| Concept | Where it lives | Example | Purpose |
|---------|---------------|---------|---------|
| **Specifier** | manifest, lock key | `"^6"`, `"~1.15.2"` | Range constraint |
| **Comment** | lock entry, workflow `#` | `"v6"`, `"v1.15.2"` | Human-readable display |
| **Version** | lock entry | `"v6.0.2"` | Resolved tag from GitHub |

## Domain Type: `Specifier`

New enum replacing `Version` in manifest/lock-key contexts:

```rust
pub enum Specifier {
    /// Semver range: "^6", "~1.15.2"
    Range {
        req: semver::VersionReq,  // for matching
        raw: String,              // "^6" — serialization roundtrip
        comment: String,          // "v6" — for workflow comments
    },
    /// Non-semver ref: "main", "develop"
    Ref(String),
    /// Direct SHA
    Sha(String),
}
```

Parsing: `"^6"` → `Range { req: VersionReq::parse("^6"), raw: "^6", comment: "v6" }`

The `comment` is derived at parse time by stripping the operator and adding `v` prefix. The `Specifier` type provides `matches(&semver::Version) -> bool` delegating to `VersionReq`.

## `Version` Scope Reduction

`Version` becomes lock-only — it represents a concrete resolved tag (e.g., `"v6.0.2"`). It no longer appears in manifests, lock keys, or upgrade outputs.

## File Formats

### Manifest v2 (`gx.toml`)

```toml
[gx]
min_version = "0.6.0"

[actions]
"actions/checkout" = "^6"
"actions-rust-lang/setup-rust-toolchain" = "~1.15.2"
"release-plz/action" = "^0.5"
"some/action" = "main"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "^3" },
]
```

- `[gx]` section: optional, absent = v1 format (old)
- `min_version`: the gx version that wrote this file; older binaries hard-error
- Override `version` field: keeps the name for user familiarity, value is a specifier

### Lock v1.4 (`gx.lock`)

```toml
version = "1.4"

[actions]
"actions/checkout@^6" = { sha = "de0fac2...", version = "v6.0.2", comment = "v6", repository = "actions/checkout", ref_type = "tag", date = "2026-01-09T..." }
```

- Key uses specifier (`@^6`) instead of v-prefix (`@v6`)
- `comment` field replaces `specifier` field — stores what to write as `# comment` in workflows
- `version` field unchanged — most specific resolved tag from GitHub

## Migration: Write-Time (Cargo-Style)

```
┌────────────┐     ┌────────────┐     ┌────────────┐
│   Parser   │     │   Domain   │     │ Serializer │
│ reads both │────►│   Model    │────►│ writes     │
│ v1 and v2  │     │ (unified)  │     │ v2 only    │
└────────────┘     └────────────┘     └────────────┘
```

- **Parsers** are pure functions — no side effects, no file rewrites
- **Serializers** always write current format
- **Read-only commands** (lint, check) work with old formats without modifying files
- **Write commands** (tidy, init, upgrade) output current format — migration is implicit
- Parser returns `Parsed<T> { value: T, migrated: bool }` for signaling

### Manifest migration (v1 → v2)

Detection: absence of `[gx]` section.

Conversion rules:
- `"v6"` (Major) → `"^6"`
- `"v4.2"` (Minor) → `"^4.2"`
- `"v1.15.2"` (Patch) → `"~1.15.2"`
- `"main"` → `"main"` (unchanged)
- `"abc123..."` → `"abc123..."` (SHA, unchanged)

### Lock migration (v1.0/v1.3 → v1.4)

Detection: `version` field value.

- v1.0: plain string SHA values → full entries with defaults
- v1.3: rekey `action@v6` → `action@^6`, rename `specifier` → drop, derive `comment` from old key version
- v1.4: current format, no migration
- Unknown future version: hard error

### Migration UX

```
$ gx tidy
  migrated gx.toml → semver specifiers
  migrated gx.lock → v1.4
  Tidying 3 workflows...
```

One line per file, only when migration actually occurs. Read-only commands show nothing.

## Upgrade Rearchitecture

### `UpgradeAction` changes

```rust
enum UpgradeAction {
    InRange {
        candidate: Version,           // "v6.2.0" — tag to resolve
    },
    CrossRange {
        candidate: Version,           // "v6.1.0" — tag to resolve
        new_specifier: Specifier,     // "^6" — for manifest
        new_comment: String,          // "v6" — for lock
    },
}
```

### `find_upgrade_candidate` changes

```rust
fn find_upgrade_candidate(
    specifier: &Specifier,          // was: manifest_version: &Version
    lock_version: Option<&Version>,
    candidates: &[Version],
    allow_major: bool,
) -> Option<UpgradeAction>
```

In-range detection uses `specifier.matches(&candidate_semver)` via `semver::VersionReq` instead of hand-rolled major/minor comparison.

Cross-range output preserves the operator from the original specifier:
- `"^4"` upgrading to v6.1.0 → `new_specifier = "^6"` (caret preserved, major precision)
- `"~1.15.2"` upgrading to v1.16.0 → `new_specifier = "~1.16.0"` (tilde preserved, patch precision)

## Data Flow Summary

### Init/Tidy (workflow → manifest + lock)

```
Workflow # v6 → comment="v6" → specifier="^6" (manifest)
                             → comment="v6" (lock)
             → describe_sha() → version="v6.0.2" (lock)
```

### Upgrade — In-Range

```
Manifest "^6" unchanged, lock comment "v6" unchanged
Only lock.version updates (e.g., "v6.0.2" → "v6.2.0")
```

### Upgrade — Cross-Range

```
Manifest "^4" → "^6" (operator preserved, precision preserved)
Lock comment "v4" → "v6"
Lock version "v4.2.1" → "v6.1.0"
```

### Workflow output

```
format!("{sha} # {comment}")  →  "de0fac2... # v6"
```

Comment comes from lock entry, not manifest specifier.

## Type Migration Map

| Current | Current usage | New type |
|---------|--------------|----------|
| `ActionSpec.version` | manifest entry | `Specifier` |
| `ActionOverride.version` | override entry | `Specifier` |
| `LockKey.version` | lock key part | `Specifier` |
| `LockEntry.specifier` | derived range | **dropped** |
| `LockEntry.comment` | **(new)** | `String` |
| `LockEntry.version` | resolved tag | `Version` (unchanged) |
| `Manifest.get()` | returns `&Version` | returns `&Specifier` |
| `find_upgrade_candidate` | takes `&Version` | takes `&Specifier` |
| `UpgradeAction::CrossRange` | `new_manifest_version` | `new_specifier` + `new_comment` |
| `ResolvedAction.version` | manifest version for lock key | `Specifier` |

## Version Guard

When gx binary version < manifest `min_version`:

```
error: gx.toml requires gx >= 0.6.0 (you have 0.5.10)
```
