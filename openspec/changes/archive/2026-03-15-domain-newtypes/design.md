## Context

The codebase already has well-designed newtypes (`ActionId`, `Version`, `CommitSha`, `StepIndex`) that demonstrate the pattern. Several other domain concepts remain as bare `String`: repository identifiers, workflow paths, job IDs, version comments, commit dates, lint rule names, and the GitHub token. This design covers how to introduce newtypes for these concepts incrementally.

## Goals / Non-Goals

**Goals:**
- Make domain structs self-documenting through types
- Prevent mixing up strings that mean different things (e.g., a workflow path vs a job ID)
- Follow the existing newtype conventions already in the codebase
- Keep each step as an independent, reviewable PR

**Non-Goals:**
- Validation at construction time (except `WorkflowPath` normalization and `RuleName` exhaustiveness)
- Performance optimization (no `Arc<str>`, no interning)
- Changing serialization formats (TOML files remain identical)
- Introducing new public API behavior

## Decisions

### 1. Inner field visibility: private

Existing newtypes use `pub` inner fields (e.g., `ActionId(pub String)`). New newtypes will use **private** fields with `as_str()` accessors. This prevents arbitrary construction — callers go through `From` impls or named constructors.

This is a forward-looking decision: if we later add validation (e.g., `Repository` must contain exactly one `/`), the constructor is already the single entry point. Existing newtypes can be migrated to private fields separately if desired, but that's out of scope.

**Alternative considered:** Match existing `pub` convention for consistency. Rejected because the whole point of this change is construction control — `pub` inner fields defeat that.

### 2. Standard trait surface

Every `String`-wrapping newtype derives and implements the same baseline:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Foo(String);

impl Foo {
    pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for Foo { ... }
impl From<String> for Foo { ... }
impl From<&str> for Foo { ... }
```

Exceptions to this baseline:
- `WorkflowPath`: No `From` impls — uses `WorkflowPath::new()` only (normalization must be explicit).
- `GitHubToken`: No `From<&str>` and no `Display` — tokens come from `String` env vars, never from `&str` literals or user-facing output.

Additional traits only when needed:
- `Serialize`/`Deserialize` — only on types that appear in serde-derived structs (not all do)
- `Ord` — only if used as `BTreeMap` key
- `Hash` — included by default since it's cheap and useful

### 3. `RuleName` as enum, not newtype

The four lint rules are a closed set. An enum with `Display`/`FromStr`/serde support is the right representation:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleName {
    ShaMismatch,
    Unpinned,
    StaleComment,
    UnsyncedManifest,
}
```

Serde strategy: `#[serde(rename_all = "kebab-case")]` on the enum. This produces the correct TOML keys for all variants: `ShaMismatch` → `sha-mismatch`, `Unpinned` → `unpinned`, `StaleComment` → `stale-comment`, `UnsyncedManifest` → `unsynced-manifest`.

The `Rule` trait's `fn name(&self) -> &str` becomes `fn name(&self) -> RuleName`. Config changes from `BTreeMap<String, Rule>` to `BTreeMap<RuleName, Rule>`. `Diagnostic::new` takes `RuleName` directly instead of `impl Into<String>`.

**Default config construction**: `Lint::get_rule()` changes signature from `get_rule(&self, name: &str, default_level: Level)` to `get_rule(&self, name: RuleName, default_level: Level)`. The lookup uses `self.rules.get(&name)` on the `BTreeMap<RuleName, Rule>`. When no `[lint.rules]` section exists, `self.rules` is empty and the default `Rule { level: default_level, ignore: vec![] }` is returned — this logic is unchanged since it already constructs a fallback regardless of key type.

**Unknown TOML keys**: With `BTreeMap<RuleName, Rule>`, serde will reject unrecognized rule names during deserialization (e.g., a typo like `sha-missmatch` or a key from a future version). This is intentional — it catches config typos at parse time rather than silently ignoring misconfigured rules. The file-format spec's "forward-compatible reads" requirement (ignore unknown keys) applies to `[resolutions]` and `[actions]` sections, not to `[lint.rules]` where silent ignoring is harmful. If a softer approach is needed later, a custom deserializer that collects unknown keys into warnings can be added without changing the `RuleName` enum.

**Alternative considered:** Keep `String` for forward-compatibility with user-defined rules. Rejected because there's no planned extension point, and the closed set is a feature — exhaustive matching catches missing rule handling at compile time.

### 4. `WorkflowPath` enforces forward-slash normalization

The scanner already does `replace('\\', "/")` when constructing workflow paths. `WorkflowPath::new()` will apply the same normalization, making it a guaranteed invariant rather than a convention.

```rust
impl WorkflowPath {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into().replace('\\', "/"))
    }
}
```

No `From<String>` for `WorkflowPath` — only the named constructor. This makes the normalization explicit at every construction site.

**`IgnoreTarget.workflow` stays `String`**: The `IgnoreTarget` struct in config deserialization keeps `workflow: Option<String>` because ignore targets come from user TOML input and are matched via `ends_with`. The comparison in `matches_ignore_action` uses `diag_workflow.as_str().ends_with(target_str)`, so both sides are `&str` at the comparison point. Since `WorkflowPath` normalizes backslashes to forward slashes, and TOML config values use forward slashes (TOML is a text format, not a filesystem path), this is safe. If a user writes backslashes in an ignore target, the match will fail — this is acceptable because TOML conventionally uses forward slashes for paths.

**Alternative considered:** `From<String>` with silent normalization. Rejected because implicit normalization in a `From` impl is surprising behavior.

### 5. `GitHubToken` masks Debug output

```rust
impl fmt::Debug for GitHubToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GitHubToken(***)")
    }
}
```

No `Display` impl — tokens should never be formatted for output. The only consumption path is `token.as_str()` in the Authorization header construction.

`Clone` is derived despite this being a security-sensitive type. This is acceptable because the token already exists as a plain string in environment variables and process memory — `Clone` does not increase the attack surface. The masked `Debug` impl is the meaningful security boundary (preventing accidental logging).

### 6. Type placement

Each type lives closest to where it's semantically defined:

| Type | Module | Rationale |
|------|--------|-----------|
| `Repository` | `domain::action::identity` | Derived from `ActionId`, same module |
| `WorkflowPath` | `domain::workflow_actions` | Lives alongside `Location` and `StepIndex` |
| `JobId` | `domain::workflow_actions` | Same — part of `Location` |
| `VersionComment` | `domain::action::identity` | Used by both `Specifier::Range` (in `domain::action::specifier`) and `Resolution` (in `domain::lock::resolution`). Placing it in `domain::action::identity` avoids a reverse dependency from `domain::action` → `domain::lock`. |
| `CommitDate` | `domain::action::identity` | Part of commit metadata, alongside `CommitSha`. `identity.rs` is ~445 lines; adding `Repository` and `CommitDate` keeps it within the 500-line budget. |
| `RuleName` | `lint` | Owned by the lint system |
| `GitHubToken` | `config` | Loaded from env, passed to infra (config.rs is ~314 lines, well within the 500-line budget) |

### 7. Serialization boundary strategy

Newtypes don't change the TOML format. The infra layer converts at the boundary:

- **Lock format** (`infra::lock::format`): `ActionCommitData` keeps `String` fields (it's a serde struct for TOML). Conversion to/from domain types happens in the existing `From`/`Into` impls between format structs and domain structs.
- **Manifest format** (`infra::manifest::convert`): Same approach — `TomlOverride` keeps `String` fields, conversion happens at the boundary.
- **Config** (`config.rs`): `Lint` TOML deserialization uses `BTreeMap<RuleName, Rule>` directly via serde rename.
- **Manifest infra** (`infra::manifest::convert`): `LintData.rules` also changes from `BTreeMap<String, Rule>` to `BTreeMap<RuleName, Rule>` since this is the serde struct that deserializes `[lint.rules]` from TOML — the unknown-key rejection happens here at parse time.

## Risks / Trade-offs

**[Verbosity at construction sites]** → Every place that creates a `Repository` or `WorkflowPath` needs an explicit conversion. This is intentional — it marks the boundary between untyped and typed worlds. The `From` impls keep it to `.into()` in most cases.

**[RuleName serde compatibility]** → Changing config keys from `String` to `RuleName` enum means unrecognized rule names in TOML will fail deserialization instead of being silently ignored. This is the desired behavior — typos in config should be caught early, and the file-format spec's "forward-compatible reads" requirement applies to lock/manifest sections, not `[lint.rules]`. A custom deserializer with warnings can be added later if needed without changing the enum.

**[Private inner fields diverge from existing convention]** → Existing newtypes use `pub` fields. New ones won't. This creates inconsistency. → Accept the inconsistency for now. Migrating existing types to private fields can be a follow-up if desired, but it's not required for this change to be valuable.
