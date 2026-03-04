### Requirement: Create commits via GitHub GraphQL API
The workflow SHALL use the GitHub GraphQL `createCommitOnBranch` mutation to create commits on `homebrew-tap`, ensuring commits are automatically signed by GitHub.

#### Scenario: Formula commit is created via API
- **WHEN** the formula `.rb` files have been downloaded and linted
- **THEN** the workflow SHALL create a commit using `createCommitOnBranch` with the formula file contents as `FileAddition` objects (base64-encoded)

#### Scenario: Commit has verified signature
- **WHEN** the commit is created via the GraphQL API using the GitHub App installation token
- **THEN** the resulting commit SHALL have a verified signature on GitHub

### Requirement: Commit to a temporary branch
The workflow SHALL create commits on a temporary branch, not directly on `main`.

#### Scenario: Temporary branch is used
- **WHEN** the workflow creates a formula commit
- **THEN** the commit SHALL target a new branch (e.g., `formula/<name>-<version>`) created from the current `main` HEAD

### Requirement: Open and merge a pull request
The workflow SHALL open a pull request from the temporary branch to `main` and merge it automatically.

#### Scenario: PR is created and merged
- **WHEN** the commit has been created on the temporary branch
- **THEN** the workflow SHALL create a pull request using `gh pr create` and merge it using `gh pr merge --merge`

#### Scenario: PR targets main branch
- **WHEN** the pull request is created
- **THEN** it SHALL target the `main` branch of `homebrew-tap`

### Requirement: Clean up temporary branch after merge
The workflow SHALL delete the temporary branch after the pull request is merged.

#### Scenario: Branch is deleted after merge
- **WHEN** the pull request has been successfully merged
- **THEN** the temporary branch SHALL be deleted (either via `--delete-branch` flag or explicit deletion)

### Requirement: Accumulate all formula changes into a single commit
The workflow SHALL collect all formula file changes and create a single commit and single PR, rather than one per formula.

#### Scenario: Multiple formulae in one commit
- **WHEN** the release plan contains multiple releases with `.rb` artifacts
- **THEN** all formula files SHALL be included as `FileAddition` objects in a single `createCommitOnBranch` call

#### Scenario: Single formula in one commit
- **WHEN** the release plan contains a single release with a `.rb` artifact
- **THEN** the formula file SHALL be included as a `FileAddition` in the `createCommitOnBranch` call

### Requirement: Resolve main branch HEAD OID before commit
The workflow SHALL query the current HEAD OID of the `main` branch before creating the commit, to satisfy the `expectedHeadOid` parameter of `createCommitOnBranch`.

#### Scenario: HEAD OID is fetched
- **WHEN** the workflow prepares to create a commit
- **THEN** it SHALL query the `main` branch's latest commit OID via the GraphQL API
