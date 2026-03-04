## Context

The `publish-homebrew-formula` job in `release.yml` currently creates local git commits and pushes directly to the `main` branch of `gmeligio/homebrew-tap`. The `homebrew-tap` repository has branch protection rules requiring:
1. Changes must be made through a pull request
2. Commits must have verified signatures

The current approach (local `git commit` + `git push`) produces unsigned commits and bypasses the PR requirement, causing the job to fail.

## Goals / Non-Goals

**Goals:**
- Produce GitHub-verified commits on `homebrew-tap` using the existing GitHub App token
- Deliver formula changes via pull request to satisfy branch protection rules
- Preserve the existing formula download, lint, and multi-release loop logic

**Non-Goals:**
- Changing the GitHub App or its credentials
- Modifying how formula `.rb` files are generated or downloaded
- Adding PR review requirements (the PR will be auto-merged)

## Decisions

### Use GitHub GraphQL `createCommitOnBranch` for verified commits

**Choice**: Use the GraphQL `createCommitOnBranch` mutation via `gh api graphql` to create commits.

**Why**: Commits created through the GitHub API using a GitHub App installation token are automatically signed by GitHub, producing verified signatures. This is the simplest path to signed commits without managing GPG keys.

**Alternative considered**: Import GPG keys into the runner and sign with `git commit -S`. Rejected because it adds key management complexity and the App token already provides a signing mechanism via the API.

### Use a temporary branch + PR + auto-merge

**Choice**: Create a temporary branch (e.g., `formula/gx-0.5.8`), commit to it via GraphQL, open a PR with `gh pr create`, then merge with `gh pr merge --merge`.

**Why**: Branch protection requires PRs. Auto-merging immediately keeps the workflow simple and avoids needing human review for automated formula updates.

**Alternative considered**: Using `--admin` flag to bypass branch protection. Rejected because it defeats the purpose of the rules and the App may not have admin privileges.

### Read file content and compute base tree from the API

**Choice**: Before creating the commit, read the current `main` HEAD OID via GraphQL and prepare the file content (base64-encoded) from the downloaded/linted formula file on disk.

**Why**: `createCommitOnBranch` requires the branch's expected HEAD OID (for optimistic concurrency) and file changes as `FileAddition` objects with base64 content.

### Single commit per PR even with multiple formulae

**Choice**: Accumulate all formula file changes and create a single commit + single PR, rather than one PR per formula.

**Why**: Simpler, fewer API calls, and the current workflow already batches all formulae into sequential commits before a single `git push`. A single PR is the natural equivalent.

## Risks / Trade-offs

- **[Concurrency]** If another process pushes to `homebrew-tap` between reading HEAD OID and creating the commit, `createCommitOnBranch` will fail with a conflict. → Acceptable for now since only this workflow writes to the repo.
- **[PR merge delay]** If the repo has required status checks, the auto-merge may be blocked. → The `homebrew-tap` repo should not have required checks for formula updates, but this should be verified.
- **[Complexity]** The GraphQL approach is more verbose than `git commit` + `git push`. → Acceptable trade-off for compliance with branch protection rules.
