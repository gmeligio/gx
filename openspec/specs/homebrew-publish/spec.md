## Bot Identity

### Requirement: Derive GitHub App bot identity dynamically
The workflow SHALL query the GitHub API (`GET /app`) using the app token to obtain the app's slug and derive the bot identity (`<slug>[bot]` / `<app-id>+<slug>[bot]@users.noreply.github.com`).

#### Scenario: App identity is resolved
- **WHEN** the `app-token` step completes successfully
- **THEN** a subsequent step SHALL call `gh api /app` with the app token and output the bot `name` and `email`

#### Scenario: App slug is used for git config
- **WHEN** the app identity is resolved
- **THEN** `git config user.name` SHALL be set to `<slug>[bot]` and `git config user.email` SHALL be set to `<app-id>+<slug>[bot]@users.noreply.github.com`

### Requirement: Remove hardcoded bot identity env vars
The job-level `GITHUB_USER` and `GITHUB_EMAIL` env vars SHALL be removed.

---

## Commit Delivery

### Requirement: Create commits via GitHub GraphQL API
The workflow SHALL use the GitHub GraphQL `createCommitOnBranch` mutation to create commits on `homebrew-tap`, ensuring commits are automatically signed by GitHub.

### Requirement: No direct git push
The workflow SHALL NOT use `git push` with a token-injected remote URL. All commits SHALL be created via the GitHub GraphQL API and delivered through a pull request.

### Requirement: Formula contains SHA256 without sidecar files
The formula SHALL contain SHA256 checksums baked in by cargo-dist at build time. The workflow SHALL NOT depend on `.sha256` sidecar files for checksum data.

---

## PR Workflow

### Requirement: Commit to a temporary branch
The workflow SHALL create commits on a temporary branch (e.g., `formula/<name>-<version>`), not directly on `main`.

### Requirement: Open and merge a pull request
The workflow SHALL open a pull request from the temporary branch to `main` and merge it automatically using `gh pr merge --merge`.

### Requirement: Clean up temporary branch after merge
The workflow SHALL delete the temporary branch after the pull request is merged.

### Requirement: Accumulate all formula changes into a single commit
All formula file changes SHALL be collected into a single `createCommitOnBranch` call and single PR, rather than one per formula.

### Requirement: Resolve main branch HEAD OID before commit
The workflow SHALL query the current HEAD OID of the `main` branch before creating the commit, to satisfy the `expectedHeadOid` parameter of `createCommitOnBranch`.
