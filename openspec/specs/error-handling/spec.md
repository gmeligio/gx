## Token-Optional API

### Requirement: GitHub API requests work without authentication token
`GithubRegistry` SHALL make API requests regardless of whether a token is configured. Token present: include `Authorization: Bearer {token}` header. Token absent: send without auth (works for public repos). No `TokenRequired` error variant.

### Requirement: One-time warning when no token is configured
The system SHALL emit a single warning at command entry when `GITHUB_TOKEN` is not set: "No GITHUB_TOKEN set -- using unauthenticated GitHub API (60 requests/hour limit)."

### Requirement: HTTP status codes are classified into specific error variants
`GithubError` SHALL distinguish error types based on HTTP status codes:

| HTTP Status | Condition | GithubError variant |
|---|---|---|
| 429 | Primary rate limit | `RateLimited` |
| 403 | `x-ratelimit-remaining: 0` | `RateLimited` |
| 401 | Unauthorized | `Unauthorized` |
| 403 | Non-rate-limit | `Unauthorized` |
| 404 | Not found | `NotFound` |
| 500+ | Server error | `ApiError` |

---

## Recoverable Errors

### Requirement: Resolution errors are classified as recoverable or strict
`ResolutionError` SHALL provide an `is_recoverable()` method.

| Variant | Recoverable? |
|---|---|
| `RateLimited` | Yes |
| `AuthRequired` | Yes |
| `ResolveFailed` | No |
| `NoTagsForSha` | No |

### Requirement: Lock resolution warns on recoverable errors and fails only on strict errors
`update_lock` SHALL classify each resolution failure using `is_recoverable()`. Recoverable failures are logged as warnings and skipped. Only strict failures cause `TidyError::ResolutionFailed`.

#### Scenario: All errors are recoverable
- **THEN** warnings logged, `update_lock` returns `Ok`

#### Scenario: Mix of recoverable and strict errors
- **THEN** recoverable logged as warning, returns `Err` containing only the strict failure

#### Scenario: All errors are strict
- **THEN** returns `Err(TidyError::ResolutionFailed)` with all strict failures

### Requirement: GithubError maps to ResolutionError preserving recoverability
- `GithubError::RateLimited` -> `ResolutionError::RateLimited`
- `GithubError::Unauthorized` -> `ResolutionError::AuthRequired`
- `GithubError::NotFound` / `GithubError::ApiError` -> `ResolutionError::ResolveFailed`
