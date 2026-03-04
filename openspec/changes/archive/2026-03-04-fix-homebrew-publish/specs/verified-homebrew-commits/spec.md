## MODIFIED Requirements

### Requirement: Authenticate git push via remote URL token injection
The workflow SHALL NOT use `git push` with a token-injected remote URL. Instead, all commits SHALL be created via the GitHub GraphQL API and delivered through a pull request.

#### Scenario: No direct git push
- **WHEN** the formula commit step runs
- **THEN** the workflow SHALL NOT execute `git push` or set `git remote set-url` with an embedded token

## REMOVED Requirements

### Requirement: Checkout SHALL NOT persist credentials
**Reason**: The workflow no longer needs to checkout `homebrew-tap` as a git repository since commits are created via the GraphQL API. File content is read from downloaded artifacts on disk and sent directly through the API.
**Migration**: Remove the `actions/checkout` step for `homebrew-tap`. The formula files are already available from `actions/download-artifact`.
