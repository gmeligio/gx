## 1. Fix the Vulnerability

- [x] 1.1 Run `cargo update aws-lc-rs aws-lc-sys` to bump `aws-lc-sys` from 0.37.1 to 0.38.0
- [x] 1.2 Verify the fix with `cargo build` and `cargo test`

## 2. Set Up cargo-deny

- [x] 2.1 Add `cargo-deny` to `.config/mise.toml` as a tool and add a `deny` task (`cargo deny check`)
- [x] 2.2 Install via `mise install` and run `cargo deny init` to generate `deny.toml` at project root
- [x] 2.3 Configure strictest settings: `multiple-versions = "deny"`, `wildcards = "deny"`, `unknown-registry = "deny"`, `unknown-git = "deny"`, `unlicensed = "deny"`
- [x] 2.4 Build the license allowlist from the current dependency tree (`cargo deny list`)
- [x] 2.5 Run `cargo deny check` and resolve any failures (add `skip` entries for legitimate duplicate versions if needed)

## 3. CI Integration

- [x] 3.1 Add `deny` job to `.github/workflows/build.yml` using `jdx/mise-action` (same pattern as the `semver` job) to run `mise run deny`
- [x] 3.2 Verify the action works by pushing to a test branch or running locally

## 4. Verify

- [x] 4.1 Run full `cargo deny check` locally — all four categories pass
- [x] 4.2 Run `cargo test` and `cargo clippy` — no regressions
