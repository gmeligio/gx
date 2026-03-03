## 1. Add app identity resolution step

- [x] 1.1 Add a new step `Get GitHub App user` after the `app-token` step in the `publish-homebrew-formula` job. It should run `gh api /app --jq '.slug'` with `GH_TOKEN` set to `${{ steps.app-token.outputs.token }}`, then output `name` (`<slug>[bot]`) and `email` (`<app-id>+<slug>[bot]@users.noreply.github.com`) via `$GITHUB_OUTPUT`.

## 2. Harden checkout

- [x] 2.1 Change `persist-credentials` from `true` to `false` on the `actions/checkout` step for `homebrew-tap`.

## 3. Clean up hardcoded identity

- [x] 3.1 Remove the `GITHUB_USER` and `GITHUB_EMAIL` env vars from the job-level `env` block.

## 4. Update commit and push step

- [x] 4.1 Replace `git config` lines in the "Commit formula files" step to use `${{ steps.app-user.outputs.name }}` and `${{ steps.app-user.outputs.email }}`.
- [x] 4.2 Add `git remote set-url origin https://x-access-token:${PUSH_TOKEN}@github.com/gmeligio/homebrew-tap.git` before `git push`, with `PUSH_TOKEN` set as a step-level env var from `${{ steps.app-token.outputs.token }}`.
