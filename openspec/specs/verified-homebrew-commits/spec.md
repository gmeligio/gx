### Requirement: Derive GitHub App bot identity dynamically
The workflow SHALL query the GitHub API (`GET /app`) using the app token to obtain the app's slug and derive the bot identity (`<slug>[bot]` / `<app-id>+<slug>[bot]@users.noreply.github.com`).

#### Scenario: App identity is resolved
- **WHEN** the `app-token` step completes successfully
- **THEN** a subsequent step SHALL call `gh api /app` with the app token and output the bot `name` and `email`

#### Scenario: App slug is used for git config
- **WHEN** the app identity is resolved
- **THEN** `git config user.name` SHALL be set to `<slug>[bot]` and `git config user.email` SHALL be set to `<app-id>+<slug>[bot]@users.noreply.github.com`

### Requirement: Authenticate git push via remote URL token injection
The workflow SHALL NOT use `git push` with a token-injected remote URL. Instead, all commits SHALL be created via the GitHub GraphQL API and delivered through a pull request.

#### Scenario: No direct git push
- **WHEN** the formula commit step runs
- **THEN** the workflow SHALL NOT execute `git push` or set `git remote set-url` with an embedded token

### Requirement: Remove hardcoded bot identity env vars
The job-level `GITHUB_USER` and `GITHUB_EMAIL` env vars SHALL be removed.

#### Scenario: No hardcoded identity
- **WHEN** the `publish-homebrew-formula` job is defined
- **THEN** there SHALL be no `GITHUB_USER` or `GITHUB_EMAIL` env vars at the job level
