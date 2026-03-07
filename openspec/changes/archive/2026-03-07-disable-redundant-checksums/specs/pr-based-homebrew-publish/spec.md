## MODIFIED Requirements

### Requirement: Create commits via GitHub GraphQL API
The workflow SHALL use the GitHub GraphQL `createCommitOnBranch` mutation to create commits on `homebrew-tap`, ensuring commits are automatically signed by GitHub.

The formula SHALL contain SHA256 checksums baked in by cargo-dist at build time. The workflow SHALL NOT depend on `.sha256` sidecar files for checksum data.

#### Scenario: Formula commit is created via API
- **WHEN** the formula `.rb` files have been downloaded and linted
- **THEN** the workflow SHALL create a commit using `createCommitOnBranch` with the formula file contents as `FileAddition` objects (base64-encoded)

#### Scenario: Commit has verified signature
- **WHEN** the commit is created via the GraphQL API using the GitHub App installation token
- **THEN** the resulting commit SHALL have a verified signature on GitHub

#### Scenario: Formula contains SHA256 without sidecar files
- **WHEN** cargo-dist generates the `.rb` formula file
- **THEN** the formula SHALL contain inline `sha256` values computed from the built artifacts, independent of any `.sha256` sidecar files
