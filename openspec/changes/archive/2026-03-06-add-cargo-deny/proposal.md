## Why

Dependabot alert #2 (GHSA-vw5v-4f2q-w9xf) revealed a high-severity vulnerability in `aws-lc-sys` — a transitive dependency pulled through `reqwest → rustls → aws-lc-rs`. The project has no local vulnerability scanning, so these issues are only caught after they appear on GitHub. Adding `cargo deny` provides local and CI-level supply chain checks: vulnerabilities, license compliance, dependency bans, and source verification. This matches the project's existing strict posture (`warnings = "deny"`, `clippy pedantic = "deny"`).

## What Changes

- **Add `deny.toml`** at project root with strictest defaults: deny all advisories, deny duplicate versions, deny wildcard dependencies, explicit license allowlist, deny unknown registries/git sources.
- **Fix the vulnerability** by running `cargo update aws-lc-rs aws-lc-sys` to bump `aws-lc-sys` from 0.37.1 to 0.38.0.
- **Add CI job** in `build.yml` to run `cargo deny check` on pull requests.
- **Add `cargo-deny` to mise config** (`.config/mise.toml`) so it's available as a project tool.

## Impact

- **`deny.toml`** (new): cargo-deny configuration with strictest checks.
- **`Cargo.lock`**: Bumps `aws-lc-rs` 1.16.0 → 1.16.1 and `aws-lc-sys` 0.37.1 → 0.38.0.
- **`.config/mise.toml`**: Add `cargo-deny` as a managed tool and a `deny` task.
- **`.github/workflows/build.yml`**: New `deny` job using `jdx/mise-action` (matching the existing `semver` job pattern) to run `mise run deny`.
