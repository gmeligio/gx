## Why

The `publish-homebrew-formula` job fails because the `homebrew-tap` repository has branch protection rules requiring (1) changes through pull requests and (2) verified commit signatures. The current workflow does a direct `git push` to `main` with unsigned commits, violating both rules.

## What Changes

- Replace local `git commit` + `git push` with GitHub's GraphQL `createCommitOnBranch` mutation, which produces GitHub-verified signatures automatically when using an App token
- Create commits on a temporary branch instead of directly on `main`
- Open a pull request from the temporary branch and merge it via the GitHub API
- Clean up the temporary branch after merge

## Capabilities

### New Capabilities

- `pr-based-homebrew-publish`: Publish Homebrew formula changes via PR + API-signed commits instead of direct push

### Modified Capabilities

- `verified-homebrew-commits`: Requirements change from direct push with token-injected URL to API-based commit creation and PR-based merge flow

## Impact

- **Workflow**: `.github/workflows/release.yml` — the `publish-homebrew-formula` job steps change significantly
- **GitHub App permissions**: The App token must have permission to create branches, commits (via GraphQL), and pull requests on `homebrew-tap`
- **Dependencies**: No new dependencies; uses `gh` CLI (already available on runners) for GraphQL and PR operations
