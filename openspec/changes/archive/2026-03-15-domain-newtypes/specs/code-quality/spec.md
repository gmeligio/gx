<!-- MODIFIED: code-quality/spec.md -->
<!-- Change: New domain newtypes added alongside existing StepIndex precedent. -->

### Requirement: Domain newtypes for semantic string fields

ADDED: The following newtypes SHALL wrap bare `String` fields to provide type-safe domain identifiers. All use private inner fields with `as_str()` accessors, `Display`, and standard derives (`Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`). None perform validation at construction time (except `WorkflowPath` normalization) — they are type-level markers, not smart constructors.

#### WorkflowPath

`WorkflowPath` SHALL represent a workflow file path with forward-slash normalization. Defined in `domain::workflow_actions`.

##### Scenario: WorkflowPath normalizes Windows-style path
- **GIVEN** a path string `.github\workflows\ci.yml`
- **WHEN** `WorkflowPath::new` is called
- **THEN** `as_str()` SHALL return `.github/workflows/ci.yml`

##### Scenario: WorkflowPath preserves already-normalized path
- **GIVEN** a path string `.github/workflows/ci.yml`
- **WHEN** `WorkflowPath::new` is called
- **THEN** `as_str()` SHALL return `.github/workflows/ci.yml`

##### Scenario: WorkflowPath normalizes mixed slashes
- **GIVEN** a path string `.github/workflows\ci.yml`
- **WHEN** `WorkflowPath::new` is called
- **THEN** `as_str()` SHALL return `.github/workflows/ci.yml`

##### Scenario: WorkflowPath has no From<String> impl
- **GIVEN** the `WorkflowPath` type
- **THEN** construction SHALL only be via `WorkflowPath::new()` (no `From<String>` or `From<&str>`)
- **BECAUSE** the named constructor makes normalization explicit at every call site

##### Scenario: WorkflowPath accepts empty or degenerate input
- **GIVEN** an empty string `""`
- **WHEN** `WorkflowPath::new` is called
- **THEN** `as_str()` SHALL return `""` (no validation, normalization is a no-op on empty input)

#### JobId

`JobId` SHALL represent a workflow job identifier. Defined in `domain::workflow_actions`. Standard construction via `From<String>` and `From<&str>`. No validation — wraps the string as-is.

#### VersionComment

`VersionComment` SHALL represent a derived version comment (e.g., `"v6"` from specifier `^6`). Defined in `domain::action::identity` (not `domain::lock::resolution`) to avoid a reverse dependency from `domain::action` → `domain::lock`, since `Specifier::Range` in `domain::action::specifier` uses this type. Standard construction via `From<String>` and `From<&str>`.

##### Scenario: VersionComment is a type-level marker
- **GIVEN** a `VersionComment` value
- **THEN** it SHALL wrap a string without validation
- **BECAUSE** the invariant (comments are derived from specifiers) is enforced at the call site, not in the constructor

#### Repository

`Repository` SHALL represent an `owner/repo` identifier. Defined in `domain::action::identity`. Standard construction via `From<String>` and `From<&str>`. No validation at construction time — the string is wrapped as-is. If validation (e.g., must contain exactly one `/`) is needed later, the private field ensures the constructor is the single entry point.

#### CommitDate

`CommitDate` SHALL represent an ISO 8601 date string from commit metadata. Defined in `domain::action::identity`. Standard construction via `From<String>` and `From<&str>`. No validation — wraps the string as-is.

#### GitHubToken

`GitHubToken` SHALL represent a GitHub API token with masked debug output. Defined in `config`. Construction via `From<String>` only. No `Display` impl.

##### Scenario: GitHubToken masks Debug output
- **GIVEN** a `GitHubToken` wrapping the string `"ghp_abc123secret"`
- **WHEN** formatted with `Debug` (e.g., via `{:?}`)
- **THEN** the output SHALL be `GitHubToken(***)` — the token value SHALL NOT appear

##### Scenario: GitHubToken has no Display impl
- **GIVEN** the `GitHubToken` type
- **THEN** it SHALL NOT implement `Display`
- **BECAUSE** tokens should never be formatted for user-facing output; the only consumption path is `as_str()` for the Authorization header

##### Scenario: GitHubToken Clone is acceptable
- **GIVEN** a `GitHubToken` value
- **THEN** it SHALL derive `Clone`
- **BECAUSE** the token already exists as a plain string in environment variables and process memory — `Clone` does not increase the attack surface
