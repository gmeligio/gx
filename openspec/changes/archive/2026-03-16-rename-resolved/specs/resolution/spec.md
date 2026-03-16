### Requirement: resolve_from_sha derives all lock fields from SHA [CLARIFICATION]

The `ResolvedAction` referenced in this requirement is the **registry result type**, now renamed to `RegistryResolution`. `resolve_from_sha` returns a `RegistryResolution` with version, ref_type, and date derived from the SHA.

**Note**: After `serialization-boundary` introduces a new `ResolvedAction` (workflow output type), these are two distinct types:
- `RegistryResolution` — what the registry returned (has `repository`, `ref_type`, `date`, `specifier`)
- `ResolvedAction` — what goes into the workflow (has `id`, `sha`, `version`)

### Requirement: ResolvedAction carries metadata [CLARIFICATION]

This requirement describes the **registry result type**, now renamed to `RegistryResolution`. The struct SHALL include `repository` (`Repository`), `ref_type`, and `date` (`CommitDate`). The field previously named `version: Specifier` is now `specifier: Specifier`.

### Requirement: Lock::set_resolved renamed [NEW]

`Lock::set_resolved()` SHALL be renamed to `Lock::set_from_registry()` to match the `RegistryResolution` type name.
