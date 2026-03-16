### Requirement: ResolvedAction carries metadata [CLARIFICATION]

The `ResolvedAction` defined in this spec (registry resolution result with `repository`, `ref_type`, `date`) is renamed to `RegistryResolution` by the subsequent `rename-resolved` proposal. The `serialization-boundary` proposal introduces a **new** `ResolvedAction` struct for workflow output:

```
ResolvedAction { id: ActionId, sha: CommitSha, version: Option<Version> }
```

This is a different type from the registry result. Both types coexist in `domain/action/resolved.rs` during the interim. The name clash is temporary — after `rename-resolved` applies, the two types are:
- `RegistryResolution` — what the registry returned (has `repository`, `ref_type`, `date`)
- `ResolvedAction` — what goes into the workflow (has `id`, `sha`, `version`)

#### Scenario: ResolvedAction for workflow output [NEW]
- **GIVEN** a specifier `^4` resolved to version `v4.2.1` with SHA `abc123...`
- **WHEN** `ResolvedAction` is constructed for workflow output
- **THEN** it has `id = "actions/checkout"`, `sha = "abc123..."`, `version = Some("v4.2.1")`

#### Scenario: Bare SHA specifier produces no version annotation [NEW]
- **GIVEN** a bare SHA specifier
- **WHEN** `ResolvedAction` is constructed
- **THEN** `version` is `None`
- **BECAUSE** bare SHA specifiers need no `# comment` annotation in the workflow
