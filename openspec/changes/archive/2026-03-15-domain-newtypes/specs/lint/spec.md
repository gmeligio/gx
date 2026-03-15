<!-- MODIFIED: lint/spec.md -->
<!-- Change: Rule identification uses RuleName enum instead of strings. Unknown rule names in config are now rejected at parse time. -->

### Requirement: Configure rule severity

MODIFIED: The system SHALL allow each rule's severity to be set to `error`, `warn`, or `off` via the `[lint.rules]` section in `gx.toml`. Rule names are validated at parse time — unrecognized rule names produce a deserialization error.

#### Scenario: Unrecognized rule name in config
- **GIVEN** `gx.toml` contains `sha-missmatch = { level = "error" }` (typo)
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL fail with an error identifying the unrecognized rule name
- **BECAUSE** rule names are deserialized as a `RuleName` enum via `#[serde(rename_all = "kebab-case")]` in `LintData.rules` (`infra::manifest::convert`), not as arbitrary strings

#### Scenario: All valid rule names accepted
- **GIVEN** `gx.toml` contains any combination of `sha-mismatch`, `unpinned`, `stale-comment`, `unsynced-manifest` in `[lint.rules]`
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL succeed and each rule's configured level is applied

### Requirement: RuleName enum identifies rules

ADDED: The `RuleName` enum SHALL be the canonical identifier for lint rules. It implements `Display` (kebab-case output), `FromStr` (kebab-case input), and serde support.

#### Scenario: RuleName FromStr with valid name
- **GIVEN** the string `"sha-mismatch"`
- **WHEN** `RuleName::from_str` is called
- **THEN** it SHALL return `Ok(RuleName::ShaMismatch)`

#### Scenario: RuleName FromStr with invalid name
- **GIVEN** the string `"nonexistent-rule"`
- **WHEN** `RuleName::from_str` is called
- **THEN** it SHALL return `Err` with a message describing the unrecognized rule name

#### Scenario: RuleName Display roundtrips with FromStr
- **GIVEN** any `RuleName` variant
- **WHEN** formatted with `Display` and parsed back with `FromStr`
- **THEN** the result SHALL equal the original variant
