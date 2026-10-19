## 1. Upgrade action

- [x] 1.1 Run `gx upgrade --latest actions/create-github-app-token`; confirm `.github/gx.toml` has `"^3"`, `.github/gx.lock` resolves `^3` to the latest v3 tag, no `^2`/`v2.2.1` entries remain, and all three workflow `uses:` lines carry the locked SHA

## 2. Switch to client ID

- [x] 2.1 Replace `app-id: ${{ secrets.VERIFIED_COMMIT_ID }}` with `client-id: ${{ secrets.VERIFIED_COMMIT_CLIENT_ID }}` at `.github/workflows/release-plz.yml` (both jobs) and `.github/workflows/release.yml` (publish-homebrew-formula)
- [x] 2.2 `grep -rn VERIFIED_COMMIT_ID .github/` returns nothing

## 3. Verify

- [x] 3.1 `gx lint` passes; pre-commit hooks pass (no new findings; 2 pre-existing errors tracked in #226)
- [ ] 3.2 Open PR `ci(release): authenticate app token with client ID`

## Post-merge (not tracked here)

- Release-plz run on `main` passes the token step.
- Next tag: `publish-homebrew-formula` passes the token step.
