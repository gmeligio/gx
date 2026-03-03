## MODIFIED Requirements

### Requirement: Tag selection prefers most specific version
`select_best_tag` SHALL prefer tags with MORE components (most specific) over tags with fewer components. Among tags with the same number of components, the highest version SHALL win. Non-semver tags SHALL sort last.

#### Scenario: Most specific tag wins
- **GIVEN** tags `[v4, v4.1, v4.1.0]`
- **WHEN** selecting the best tag
- **THEN** the result is `v4.1.0` (most components)

#### Scenario: Highest version wins among same precision
- **GIVEN** tags `[v3, v4, v5]`
- **WHEN** selecting the best tag
- **THEN** the result is `v5` (highest)

#### Scenario: Non-semver tags sort last
- **GIVEN** tags `[latest, v4]`
- **WHEN** selecting the best tag
- **THEN** the result is `v4` (semver preferred)

#### Scenario: Mixed components prefers most specific
- **GIVEN** tags `[v3, v3.6, v3.6.1]`
- **WHEN** selecting the best tag
- **THEN** the result is `v3.6.1`
