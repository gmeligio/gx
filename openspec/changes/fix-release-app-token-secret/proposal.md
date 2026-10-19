## Why

Release-plz has failed on every push to `main` since 2026-10-04 (e.g. run 39244136741): workflows read `secrets.VERIFIED_COMMIT_ID`, which was deleted in favor of `VERIFIED_COMMIT_CLIENT_ID`. No release PRs or crates.io publishes happen until fixed. See `research.md`.

## What Changes

- Bump `actions/create-github-app-token` from `^2` (v2.2.1) to `^3` (v3.3.0) in `.github/gx.toml`; re-pin lock and workflows.
- Replace `app-id: ${{ secrets.VERIFIED_COMMIT_ID }}` with `client-id: ${{ secrets.VERIFIED_COMMIT_CLIENT_ID }}` in `.github/workflows/release-plz.yml` (2 sites) and `.github/workflows/release.yml` (Homebrew publish).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. CI/tooling fix with no user-facing behavior change → skip spec (`skip_specs: true`).

## Impact

- `.github/gx.toml`, `.github/gx.lock`, `.github/workflows/release-plz.yml`, `.github/workflows/release.yml`.
- Depends on the existing repo secrets `VERIFIED_COMMIT_CLIENT_ID` and `VERIFIED_COMMIT_KEY`.
