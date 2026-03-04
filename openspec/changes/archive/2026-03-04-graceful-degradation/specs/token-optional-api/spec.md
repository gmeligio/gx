## ADDED Requirements

### Requirement: GitHub API requests work without authentication token
`GithubRegistry` SHALL make API requests regardless of whether a token is configured. When a token is present, requests SHALL include the `Authorization: Bearer {token}` header. When no token is present, requests SHALL be sent without authentication.

#### Scenario: API request with token
- **GIVEN** a `GithubRegistry` with `token = Some("ghp_abc123")`
- **WHEN** any API method is called (e.g., `lookup_sha`, `tags_for_sha`)
- **THEN** the HTTP request includes `Authorization: Bearer ghp_abc123` header

#### Scenario: API request without token
- **GIVEN** a `GithubRegistry` with `token = None`
- **WHEN** any API method is called
- **THEN** the HTTP request is sent without an `Authorization` header
- **AND** the request succeeds for public repositories

#### Scenario: No token does not produce TokenRequired error
- **GIVEN** a `GithubRegistry` with `token = None`
- **WHEN** any `VersionRegistry` trait method is called
- **THEN** the result is NOT `Err(TokenRequired)`
- **AND** the method attempts the actual HTTP request

### Requirement: One-time warning when no token is configured
The system SHALL emit a single warning at command entry when `GITHUB_TOKEN` is not set, informing the user of the lower unauthenticated rate limit.

#### Scenario: No token warning on init
- **GIVEN** `GITHUB_TOKEN` is not set
- **WHEN** `gx init` runs
- **THEN** a warning is logged: "No GITHUB_TOKEN set — using unauthenticated GitHub API (60 requests/hour limit)."
- **AND** the command continues execution (does not abort)

#### Scenario: No warning when token is set
- **GIVEN** `GITHUB_TOKEN` is set to a valid token
- **WHEN** `gx init` runs
- **THEN** no rate-limit warning is emitted

### Requirement: HTTP status codes are classified into specific error variants
`GithubError` SHALL distinguish rate limiting, unauthorized access, not-found, and other API errors based on HTTP status codes. The `TokenRequired` variant SHALL be removed.

#### Scenario: Rate limited by primary rate limit (429)
- **WHEN** the GitHub API returns HTTP 429
- **THEN** `GithubError::RateLimited` is produced

#### Scenario: Rate limited by secondary rate limit (403 with exhausted quota)
- **WHEN** the GitHub API returns HTTP 403
- **AND** the `x-ratelimit-remaining` header is `0`
- **THEN** `GithubError::RateLimited` is produced

#### Scenario: Unauthorized access (401)
- **WHEN** the GitHub API returns HTTP 401
- **THEN** `GithubError::Unauthorized` is produced

#### Scenario: Forbidden non-rate-limit (403 without exhausted quota)
- **WHEN** the GitHub API returns HTTP 403
- **AND** the `x-ratelimit-remaining` header is NOT `0` (or absent)
- **THEN** `GithubError::Unauthorized` is produced

#### Scenario: Not found (404)
- **WHEN** the GitHub API returns HTTP 404
- **THEN** `GithubError::NotFound` is produced

#### Scenario: Other error status
- **WHEN** the GitHub API returns HTTP 500
- **THEN** `GithubError::ApiError` is produced with the status code
