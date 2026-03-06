## ADDED Requirements

### Requirement: Resolution errors are classified as recoverable or strict
`ResolutionError` SHALL provide an `is_recoverable()` method. `RateLimited` and `AuthRequired` variants SHALL be recoverable. `ResolveFailed` and `NoTagsForSha` SHALL be strict.

#### Scenario: RateLimited is recoverable
- **GIVEN** a `ResolutionError::RateLimited`
- **WHEN** `is_recoverable()` is called
- **THEN** it returns `true`

#### Scenario: AuthRequired is recoverable
- **GIVEN** a `ResolutionError::AuthRequired`
- **WHEN** `is_recoverable()` is called
- **THEN** it returns `true`

#### Scenario: ResolveFailed is strict
- **GIVEN** a `ResolutionError::ResolveFailed`
- **WHEN** `is_recoverable()` is called
- **THEN** it returns `false`

#### Scenario: NoTagsForSha is strict
- **GIVEN** a `ResolutionError::NoTagsForSha`
- **WHEN** `is_recoverable()` is called
- **THEN** it returns `false`

### Requirement: Lock resolution warns on recoverable errors and fails only on strict errors
`update_lock` SHALL classify each resolution failure using `is_recoverable()`. Recoverable failures SHALL be logged as warnings and skipped. Only strict failures SHALL cause `TidyError::ResolutionFailed`.

#### Scenario: All errors are recoverable — command succeeds with warnings
- **GIVEN** 3 actions need resolution
- **AND** all 3 fail with `RateLimited`
- **WHEN** `update_lock` runs
- **THEN** 3 warnings are logged (one per action)
- **AND** a summary warning is logged indicating incomplete lock
- **AND** `update_lock` returns `Ok` (no hard error)

#### Scenario: Mix of recoverable and strict errors — only strict cause failure
- **GIVEN** 3 actions need resolution
- **AND** 1 fails with `RateLimited` and 1 fails with `ResolveFailed`
- **WHEN** `update_lock` runs
- **THEN** `RateLimited` is logged as a warning
- **AND** `update_lock` returns `Err(TidyError::ResolutionFailed)` containing only the strict failure

#### Scenario: All errors are strict — all reported in failure
- **GIVEN** 2 actions need resolution
- **AND** both fail with `ResolveFailed`
- **WHEN** `update_lock` runs
- **THEN** `update_lock` returns `Err(TidyError::ResolutionFailed)` with count 2

#### Scenario: No errors — command succeeds normally
- **GIVEN** all actions resolve successfully
- **WHEN** `update_lock` runs
- **THEN** `update_lock` returns `Ok` with no warnings

### Requirement: GithubError maps to ResolutionError preserving recoverability
The `VersionRegistry` implementation SHALL map `GithubError` variants to `ResolutionError` variants preserving their recoverability classification.

#### Scenario: GithubError::RateLimited maps to ResolutionError::RateLimited
- **WHEN** a `VersionRegistry` method encounters `GithubError::RateLimited`
- **THEN** it returns `ResolutionError::RateLimited`

#### Scenario: GithubError::Unauthorized maps to ResolutionError::AuthRequired
- **WHEN** a `VersionRegistry` method encounters `GithubError::Unauthorized`
- **THEN** it returns `ResolutionError::AuthRequired`

#### Scenario: Other GithubErrors map to ResolutionError::ResolveFailed
- **WHEN** a `VersionRegistry` method encounters `GithubError::NotFound` or `GithubError::ApiError`
- **THEN** it returns `ResolutionError::ResolveFailed` with the error message as reason
