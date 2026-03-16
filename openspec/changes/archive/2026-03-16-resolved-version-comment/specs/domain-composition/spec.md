## Domain Type Composition

### Requirement: Spec replaces LockKey

(No changes to this requirement.)

### Requirement: ResolvedCommit extracts shared commit metadata

(No changes to this requirement.)

#### Scenario: Resolution comment updated to VersionComment [REMOVED]
~~- **GIVEN** the lock's `Resolution` struct has a `comment` field~~
~~- **THEN** `Resolution::comment` SHALL be `VersionComment` (not `String`)~~
~~- **AND** `Resolution.version` remains `Version` (unchanged)~~
~~- **NOTE** The main spec's `Entry { commit: ResolvedCommit, version: Option<String>, comment: String }` is aspirational naming. In the codebase, `Resolution` holds the `comment` field, and `Commit` holds commit metadata.~~

**Reason**: `Resolution` no longer has a `comment` field. The struct is `Resolution { version: Version }`. The resolved version serves as the workflow annotation directly.

### Requirement: Specifier lookup tag method [NEW]

`Specifier::to_comment()` SHALL be renamed to `Specifier::to_lookup_tag()`. This method converts a specifier to the tag name used for GitHub API lookups (e.g., `"^4"` produces `"v4"` for querying the `v4` tag). It SHALL NOT be used for workflow annotation — workflow annotations use the resolved version from `Resolution.version`.

#### Scenario: to_lookup_tag used for registry lookup
- **GIVEN** a specifier `^4`
- **WHEN** `ActionResolver::resolve()` needs to query the GitHub API
- **THEN** it calls `specifier.to_lookup_tag()` which returns `"v4"`
- **AND** this value is used as the tag name for `lookup_sha`

#### Scenario: to_lookup_tag not used for workflow comments
- **GIVEN** a specifier `^4` resolved to version `v4.2.1`
- **WHEN** gx writes the workflow annotation
- **THEN** it uses `resolution.version` (`"v4.2.1"`), NOT `specifier.to_lookup_tag()` (`"v4"`)
