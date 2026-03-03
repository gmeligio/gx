### Requirement: Derive GitHub App bot identity dynamically
The workflow SHALL query the GitHub API (`GET /app`) using the app token to obtain the app's slug and derive the bot identity (`<slug>[bot]` / `<app-id>+<slug>[bot]@users.noreply.github.com`).

#### Scenario: App identity is resolved
- **WHEN** the `app-token` step completes successfully
- **THEN** a subsequent step SHALL call `gh api /app` with the app token and output the bot `name` and `email`

#### Scenario: App slug is used for git config
- **WHEN** the app identity is resolved
- **THEN** `git config user.name` SHALL be set to `<slug>[bot]` and `git config user.email` SHALL be set to `<app-id>+<slug>[bot]@users.noreply.github.com`

### Requirement: Authenticate git push via remote URL token injection
The workflow SHALL set the remote origin URL to `https://x-access-token:<token>@github.com/gmeligio/homebrew-tap.git` before pushing, where `<token>` is the app token.

#### Scenario: Push succeeds without persist-credentials
- **WHEN** the commit step runs with `persist-credentials: false` on the checkout
- **THEN** `git push` SHALL authenticate using the token embedded in the remote URL

### Requirement: Checkout SHALL NOT persist credentials
The `actions/checkout` step for `homebrew-tap` SHALL use `persist-credentials: false`.

#### Scenario: Credentials are not persisted
- **WHEN** the checkout step completes
- **THEN** no credentials SHALL remain in the git credential store

### Requirement: Remove hardcoded bot identity env vars
The job-level `GITHUB_USER` and `GITHUB_EMAIL` env vars SHALL be removed.

#### Scenario: No hardcoded identity
- **WHEN** the `publish-homebrew-formula` job is defined
- **THEN** there SHALL be no `GITHUB_USER` or `GITHUB_EMAIL` env vars at the job level
