### Requirement: ResolvedCommit extracts shared commit metadata [CLARIFICATION]

All references to `Resolved` in this requirement now refer to `RegistryResolution`.

#### Scenario: RegistryResolution uses ResolvedCommit [MODIFIED]
- **GIVEN** a registry resolution
- **THEN** `RegistryResolution` SHALL have fields `{ id: ActionId, specifier: Specifier, commit: ResolvedCommit }`
- **AND** accessing the SHA is `resolution.commit.sha`

#### Scenario: RegistryResolution to Spec conversion [MODIFIED]
- **WHEN** a `RegistryResolution` needs a lock key
- **THEN** `From<&RegistryResolution> for Spec` SHALL produce a `Spec` from the resolution's `id` and `specifier`

### Requirement: Functions consuming all fields take ownership [CLARIFICATION]

#### Scenario: RegistryResolution::with_sha consumes self [MODIFIED]
- **WHEN** replacing the SHA on a registry resolution
- **THEN** `with_sha(self, sha: CommitSha) -> Self` SHALL consume `self`
- **AND** the caller SHALL not use the original value after the call
