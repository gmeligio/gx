## Context

`gx` manages GitHub Actions dependencies through a manifest (`gx.toml`), lock file (`gx.lock`), and workflow files (`.github/workflows/*.yml`). The `tidy` command synchronizes these three, but always mutates files. There is no read-only validation path.

The config file (`gx.toml`) currently has a single top-level section `[actions]`. The `[lint.rules]` section will be added as a sibling.

## Goals / Non-Goals

**Goals:**
- `gx lint` checks workflows against manifest and lock without writing anything
- Each rule validates exactly one thing and is independently configurable
- Zero-config runs all rules at hardcoded default levels (the implicit recommended preset)
- Rules can be set to `error`, `warn`, or `off` via `[lint.rules]` in `gx.toml`
- Rules support typed ignore targets using the same domain terms as action overrides
- Exit code 0 for clean or warnings-only, exit code 1 if any errors

**Non-Goals:**
- Network calls (no GitHub API, no CVE databases — those are future rules)
- Named presets or preset selection (single implicit preset for now)
- Inline YAML ignore comments (ignores are in `gx.toml` only)
- CLI flags for promoting warnings to errors (`--strict` or similar)
- Auto-fix mode (that's what `gx tidy` is for)

## Decisions

### Decision 1: Rule trait with shared read-only context

Each rule implements a trait and receives a `LintContext` containing the manifest, lock, and scanned workflow data:

```rust
pub trait LintRule {
    fn name(&self) -> &str;
    fn default_level(&self) -> Level;
    fn check(&self, ctx: &LintContext) -> Vec<Diagnostic>;
}

pub struct LintContext<'a> {
    pub manifest: &'a Manifest,
    pub lock: &'a Lock,
    pub workflows: &'a [LocatedAction],
    pub action_set: &'a WorkflowActionSet,
}

pub struct Diagnostic {
    pub rule: String,
    pub level: Level,
    pub message: String,
    pub workflow: Option<String>,
}

pub enum Level {
    Error,
    Warn,
    Off,
}
```

**Why**: Rules are pure functions from context to diagnostics. No side effects, no shared mutable state, no ordering dependencies. This makes them trivially testable and composable.

**Alternatives considered**:
- *Rules scan workflows themselves*: Duplicates scanning logic across rules. Better to scan once in the orchestrator and share the result.

### Decision 2: Config as object form in `[lint.rules]`

Each rule is configured as an inline TOML table:

```toml
[lint.rules]
sha-mismatch = { level = "error" }
unpinned = { level = "error", ignore = [
  { action = "actions/internal-tool" },
] }
stale-comment = { level = "off" }
```

The user may also use the expanded `[lint.rules.unpinned]` table form — it's valid TOML and parses identically. But the documented style is inline under `[lint.rules]`.

No shorthand form (`rule = "error"` as a bare string). Every rule config is an object with a `level` property.

**Why**: One form means one parser path, one documentation example, no ambiguity. The object form is future-proof for rule-specific options beyond `ignore`.

### Decision 3: Typed ignore targets with intersection semantics

Ignore entries use typed keys matching the existing domain vocabulary:

```toml
ignore = [
  { action = "actions/checkout" },
  { workflow = ".github/workflows/ci.yml" },
  { action = "actions/checkout", workflow = ".github/workflows/legacy.yml", job = "compat" },
]
```

Keys compose as intersection — each additional key narrows the scope. An entry with only `action` applies everywhere that action appears. Adding `workflow` restricts it to that file. Adding `job` restricts further.

**Why**: This mirrors the override format from `[actions.overrides]`, using the same domain terms (`action`, `workflow`, `job`). Users already understand this vocabulary.

### Decision 4: Four rules in v1

| Rule | Checks | Default |
|------|--------|---------|
| `sha-mismatch` | Workflow SHA differs from lock file SHA | `error` |
| `unpinned` | Action reference uses tag (e.g., `@v4`) instead of SHA pin | `error` |
| `unsynced-manifest` | Action in workflow not in manifest, or manifest entry not used in any workflow | `error` |
| `stale-comment` | Version comment (e.g., `# v4`) doesn't match lock entry for that SHA | `warn` |

**Why**: The first three are the "did you forget to run `gx tidy`?" family — they catch real drift. `stale-comment` is cosmetic but useful as a warning. All four are offline checks requiring no network access.

### Decision 5: Orchestrator collects and reports

The lint orchestrator:
1. Loads config (including `[lint.rules]` with defaults for unconfigured rules)
2. Builds `LintContext` by scanning workflows (reusing `WorkflowScanner` / `WorkflowScannerLocated`)
3. Runs each enabled rule, collecting `Vec<Diagnostic>`
4. Filters diagnostics against ignore targets
5. Prints diagnostics grouped by file
6. Exits 0 if no errors, 1 if any errors

Ignore filtering happens in the orchestrator, not in individual rules. Rules don't know about ignores — they report everything, and the orchestrator filters.

**Why**: Centralizing ignore logic avoids duplicating the matching code across every rule. Rules stay simple and purely about detection.

## Risks / Trade-offs

- **Scanning overhead**: `gx lint` re-scans all workflow files even if nothing changed. For most repos (< 20 workflows) this is negligible. If performance becomes an issue, caching could be added later.

- **Config parsing complexity**: Adding `[lint.rules]` to the manifest TOML means the manifest parser needs to handle (or skip) the new section. Since `ManifestData` uses `serde(default)`, unrecognized sections are ignored by default — but `[lint]` needs to be explicitly modeled.

- **Rule granularity**: `unsynced-manifest` covers two directions (action in workflow but not manifest, and action in manifest but not workflow). These could be separate rules, but they're tightly related and splitting would double the config surface for little benefit.

## Open Questions

<!-- none -->
