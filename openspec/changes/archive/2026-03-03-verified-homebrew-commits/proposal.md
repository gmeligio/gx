## Why

The `publish-homebrew-formula` job in `release.yml` currently commits with a hardcoded "axo bot" identity and uses `persist-credentials: true` on checkout. Commits pushed this way do not show as "Verified" on GitHub, and persisting credentials is a security concern. Since the job already generates a GitHub App token (`app-token` step), we can leverage the app's bot identity to produce verified commits and authenticate the push explicitly without persisting credentials.

## What Changes

- Add a new workflow step to derive the GitHub App's bot name and email from `GET /app`
- Set `persist-credentials: false` on the homebrew-tap checkout
- Remove hardcoded `GITHUB_USER` and `GITHUB_EMAIL` job-level env vars
- Update the "Commit formula files" step to:
  - Configure git user from the derived app identity
  - Set the remote URL with `x-access-token` for authenticated push

## Capabilities

### New Capabilities
- `verified-homebrew-commits`: Produce GitHub-verified commits in the homebrew-tap repo using the GitHub App's bot identity and token-authenticated push

### Modified Capabilities

## Impact

- `.github/workflows/release.yml` — `publish-homebrew-formula` job only
- Requires the GitHub App (referenced by `VERIFIED_COMMIT_ID` / `VERIFIED_COMMIT_KEY` secrets) to have read access to its own metadata (`GET /app`)
- No changes to other jobs or workflows
