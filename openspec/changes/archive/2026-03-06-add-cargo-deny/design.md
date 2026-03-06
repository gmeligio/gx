## Context

The project uses `warnings = "deny"` and `clippy pedantic = "deny"` but has no supply chain security tooling. Vulnerabilities are only caught by GitHub Dependabot after code is pushed. `cargo deny` fills this gap with four check categories: advisories, licenses, bans, and sources.

## Goals / Non-Goals

**Goals:**
- Catch known vulnerabilities locally before push
- Enforce an explicit license allowlist
- Deny duplicate dependency versions and wildcard version specs
- Deny dependencies from unknown registries or git sources
- Block PRs with supply chain issues via CI

**Non-Goals:**
- SBOM generation
- Container or infrastructure scanning
- Runtime dependency analysis

## Decisions

### 1. Use strictest defaults for all four check categories

**Advisories:**
- Default behavior already denies known vulnerabilities (unmaintained crates produce warnings by default — we keep that).
- No ignore list to start. If a vulnerability needs temporary exception, it's added with an expiration date.

**Licenses:**
- Explicit allowlist of licenses present in the current dependency tree. Start with the set actually used (MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, etc.) rather than a broad permissive list.
- `confidence-threshold = 0.8` (default) for license detection.
- `unlicensed = "deny"`.

**Bans:**
- `multiple-versions = "deny"` — no duplicate versions of the same crate.
- `wildcards = "deny"` — no `*` version specs.

**Sources:**
- `unknown-registry = "deny"` — all crates must come from crates.io.
- `unknown-git = "deny"` — no git dependencies from unknown sources.

### 2. Config at project root (`deny.toml`)

Explored moving to `.config/deny.toml` but decided against it:
- Requires `--config` flag on every invocation
- All other tool configs (`release-plz.toml`, `dist-workspace.toml`, `rust-toolchain.toml`, `.pre-commit-config.yaml`) are at root — none support `.config/` natively
- Consistency wins over tidiness

### 3. CI integration as a separate job in `build.yml`

Add a `deny` job alongside existing `check`, `fmt`, `clippy`, `test`, and `semver` jobs. Use the `EmbarkStudios/cargo-deny-action` GitHub Action for caching and advisory DB fetching.

### 4. Fix the triggering vulnerability in the same change

`cargo update aws-lc-rs aws-lc-sys` bumps:
- `aws-lc-rs`: 1.16.0 → 1.16.1
- `aws-lc-sys`: 0.37.1 → 0.38.0 (patched)

This is a Cargo.lock-only change — no Cargo.toml modifications needed.

## Risks / Trade-offs

- **`multiple-versions = "deny"` may be too strict**: If any transitive deps pull different versions of the same crate, `cargo deny check` will fail. May need to add specific `skip` entries. We'll see on first run.
- **License allowlist maintenance**: New dependencies with novel licenses will be caught. This is the point — but it requires updating `deny.toml` when adding deps.
- **Advisory DB freshness**: The DB is fetched on each run. CI uses the action's built-in fetch. Locally, `cargo deny check` fetches automatically.
