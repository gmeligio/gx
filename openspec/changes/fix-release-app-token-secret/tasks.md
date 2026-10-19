## 1. Upgrade action

- [ ] 1.1 Set `"actions/create-github-app-token" = "^3"` in `.github/gx.toml`
- [ ] 1.2 Run gx to re-resolve and re-pin; confirm `.github/gx.lock` and all three workflow uses point at v3.3.0 (`64ab729a6294000f5aa4a4bc0b73d1f32bc45510`)

## 2. Switch to client ID

- [ ] 2.1 Replace `app-id: ${{ secrets.VERIFIED_COMMIT_ID }}` with `client-id: ${{ secrets.VERIFIED_COMMIT_CLIENT_ID }}` at `.github/workflows/release-plz.yml` (both jobs) and `.github/workflows/release.yml` (publish-homebrew-formula)
- [ ] 2.2 `grep -rn VERIFIED_COMMIT_ID .github/` returns nothing

## 3. Verify

- [ ] 3.1 `gx lint` passes; pre-commit hooks pass
- [ ] 3.2 Open PR `ci(release): authenticate app token with client ID`
- [ ] 3.3 After merge, Release-plz run on `main` passes the token step
