## Context

The `publish-homebrew-formula` job in `.github/workflows/release.yml` pushes formula updates to `gmeligio/homebrew-tap`. It already creates a GitHub App token via `actions/create-github-app-token` but uses a hardcoded "axo bot" identity for commits and `persist-credentials: true` on checkout. This results in unverified commits and unnecessarily persisted credentials.

GitHub marks commits as "Verified" when the committer identity matches the authenticated GitHub App's bot account and the push is authenticated with the app's token.

## Goals / Non-Goals

**Goals:**
- Commits to `homebrew-tap` show as "Verified" on GitHub
- Derive the app's bot identity dynamically (no hardcoded app name)
- Remove `persist-credentials: true` from the checkout step
- Authenticate `git push` explicitly via remote URL token injection

**Non-Goals:**
- GPG key signing — not needed; GitHub verifies app-identity commits natively
- Changing any other job in release.yml
- Modifying the app-token generation step itself

## Decisions

### 1. Derive app identity via `GET /app`

**Decision**: Call `gh api /app --jq '.slug'` using the app token to get the app's slug at runtime.

**Rationale**: Avoids hardcoding the app name. If the app is renamed or a different app is used, the workflow adapts automatically. The `GET /app` endpoint requires no special permissions beyond the token itself.

**Alternatives considered**:
- Hardcode `verified-commit[bot]` — simpler but brittle; breaks if the app changes
- Add app slug as a separate secret — unnecessary indirection

### 2. Authenticate push via `git remote set-url`

**Decision**: Set the remote origin URL to `https://x-access-token:<token>@github.com/gmeligio/homebrew-tap.git` before pushing.

**Rationale**: Explicit, self-contained, no credential persistence. The token is only used for the push and not stored in the git credential helper.

**Alternatives considered**:
- `persist-credentials: true` on checkout — persists token in credential store for the rest of the job (security concern)
- `gh auth setup-git` — adds dependency on gh CLI credential helper integration
- `git -c http.extraHeader="Authorization: Bearer ..."` — verbose, less common pattern

### 3. Git user identity format

**Decision**: Use GitHub's bot identity format:
- `user.name`: `<slug>[bot]`
- `user.email`: `<app-id>+<slug>[bot]@users.noreply.github.com`

Where `<app-id>` comes from `secrets.VERIFIED_COMMIT_ID` and `<slug>` from `GET /app`.

**Rationale**: This is the standard format GitHub uses to identify app bot accounts. When combined with an app-token-authenticated push, GitHub recognizes the commit as originating from the app and marks it as "Verified".

## Risks / Trade-offs

- **[API rate limit on `GET /app`]** → Negligible; this is a single API call per release, well within limits.
- **[`gh` CLI availability]** → GitHub-hosted runners include `gh` by default. No risk for `ubuntu-24.04`.
- **[Token exposed in remote URL]** → The URL is set in-memory for the step's shell process. It is not persisted to disk or logs (GitHub masks secrets in logs automatically).
