## Context

`gx upgrade` uses an `UpgradeMode` enum with three variants:

```rust
pub enum UpgradeMode {
    Safe,                        // all actions, within-major
    Latest,                      // all actions, cross-major
    Targeted(ActionId, Version), // one action, exact version
}
```

`Targeted` conflates two orthogonal concerns: scope (which actions) and mode (how to upgrade). This makes it impossible to express "upgrade this one action using safe mode" or "upgrade this one action to latest" — both of which are now desired.

The CLI currently forbids `--latest` combined with `action` via `conflicts_with`. That constraint will be relaxed.

## Goals / Non-Goals

**Goals:**
- Allow `gx upgrade actions/checkout` (safe mode, single action)
- Allow `gx upgrade --latest actions/checkout` (latest mode, single action)
- Reject `gx upgrade --latest actions/checkout@v5` (incoherent combination)
- Separate mode and scope into distinct, composable types
- Only upgrade the global manifest entry for the targeted action; overrides are unaffected

**Non-Goals:**
- Wildcard/pattern matching for scope (e.g., `--only actions/*`)
- Changing upgrade behavior for existing modes (Safe, Latest, Pinned)
- Modifying how overrides are resolved during upgrade

## Decisions

### Decision 1: Separate `UpgradeMode` and `UpgradeScope` into an `UpgradeRequest`

Replace `UpgradeMode` with three types:

```rust
pub enum UpgradeScope {
    All,
    Single(ActionId),
}

pub enum UpgradeMode {
    Safe,             // stay within current major
    Latest,           // cross majors freely
    Pinned(Version),  // exact version — only valid with Single scope
}

pub struct UpgradeRequest {
    pub mode: UpgradeMode,
    pub scope: UpgradeScope,
}
```

**Why**: Mode (how to upgrade) and scope (which actions) are genuinely orthogonal. The current `Targeted` variant encodes both, preventing the new combinations. Splitting them into `UpgradeRequest` with a validation constructor makes all valid combinations expressible and the one invalid combination (`Pinned + All`) rejected in one place.

**Alternatives considered**:
- *Flat enum with all combinations* (`SafeSingle`, `LatestSingle`, etc.): avoids the struct but loses the orthogonal structure and grows quadratically with new modes/scopes.
- *Mode carries optional scope* (`Safe(Option<ActionId>)`): conflates concerns in the other direction.

### Decision 2: Validate `Pinned + All` in `UpgradeRequest::new`

```rust
impl UpgradeRequest {
    pub fn new(mode: UpgradeMode, scope: UpgradeScope) -> Result<Self> {
        if matches!((&mode, &scope), (UpgradeMode::Pinned(_), UpgradeScope::All)) {
            bail!("Pinned mode requires a single action target (e.g., actions/checkout@v5)");
        }
        Ok(Self { mode, scope })
    }
}
```

**Why**: A single `match` in the constructor exhaustively documents valid combinations and catches the invalid one early, before it reaches `determine_upgrades`.

### Decision 3: Reject `--latest` with `action@version` at CLI parse time

In `resolve_upgrade_mode`, add an explicit check:

```rust
if latest && action_str contains '@' {
    bail!("--latest cannot be combined with an exact version pin (ACTION@VERSION). \
           Use --latest ACTION to upgrade to latest, or ACTION@VERSION to pin.");
}
```

**Why**: This combination is semantically incoherent — "upgrade to latest" and "pin to v5" contradict each other. Failing early with a clear message is better than silently ignoring one of the flags.

### Decision 4: `determine_upgrades` filters by scope

For `Single(id)` scope, `determine_upgrades` filters `manifest.specs()` to only the matching action before processing. This keeps the upgrade logic itself unchanged — it still operates on a list of specs, just a list of one.

## Risks / Trade-offs

- **`Pinned + All` is a type-system non-guarantee**: The invalid combination is caught at runtime in `UpgradeRequest::new`, not at compile time. A future refactor could encode validity in types (e.g., `PinnedRequest(ActionId, Version)` as a separate type), but that adds complexity for one edge case.
  → Mitigation: the constructor validation is in one place and well-documented.

- **Breaking change to `UpgradeMode`**: Any downstream code using `UpgradeMode::Targeted` or pattern-matching on `UpgradeMode` directly will need updating. Within this codebase, that's `main.rs`, `upgrade.rs`, and `upgrade_test.rs`.
  → Mitigation: compiler will catch all sites exhaustively.

## Open Questions

<!-- none -->
