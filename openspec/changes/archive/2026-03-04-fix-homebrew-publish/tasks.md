## 1. Remove direct push approach

- [x] 1.1 Remove the `actions/checkout` step for `homebrew-tap` (no longer needed since commits are API-based)
- [x] 1.2 Remove `git config`, `git add`, `git commit`, `git remote set-url`, and `git push` commands from the "Commit formula files" step

## 2. Build formula file content for GraphQL

- [x] 2.1 After downloading and linting formula files, collect each `.rb` file's content as base64 and its path (`Formula/<filename>`) into shell variables for the GraphQL mutation
- [x] 2.2 Build the commit message from the release names and versions (e.g., "gx 0.5.8")

## 3. Create commit via GraphQL API

- [x] 3.1 Query the `main` branch HEAD OID of `homebrew-tap` using a GraphQL query (`repository.ref.target.oid`)
- [x] 3.2 Create a temporary branch name (e.g., `formula/gx-0.5.8`) for the commit
- [x] 3.3 Call `createCommitOnBranch` mutation with the formula `FileAddition` objects, the expected HEAD OID, and the temporary branch name (creating the branch if it doesn't exist via `branch.create: true`)

## 4. Open and merge pull request

- [x] 4.1 Create a PR from the temporary branch to `main` using `gh pr create` on the `homebrew-tap` repo
- [x] 4.2 Merge the PR using `gh pr merge --merge --delete-branch` to auto-delete the temporary branch

## 5. Verify

- [x] 5.1 Trigger a release (or re-run the job) and confirm the `publish-homebrew-formula` job succeeds
- [x] 5.2 Verify the resulting commit on `homebrew-tap` main has a verified signature badge
