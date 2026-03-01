## Why

`gx upgrade --latest` overwrites manifest versions with exact tags (e.g., `"v1"` becomes `"v1.1.2"`), destroying the user's intended semver range. Additionally, 5 of 9 lock entries are missing the `version` and `specifier` fields required by the lock-format v1.3 spec. Pre-release versions are also included in bulk upgrade candidates, which is undesirable default behavior.

## What Changes

- **Upgrade: preserve manifest precision.** `find_upgrade_candidate` returns richer data indicating whether the candidate is within the current manifest range. In-range candidates only update the lock (re-resolve); cross-range candidates update the manifest with precision preserved (major stays major, minor stays minor, patch stays patch).
- **Upgrade: populate lock metadata.** `resolve_and_store` in upgrade.rs calls `populate_resolved_fields` to enrich lock entries with `version` and `specifier`, matching what tidy already does.
- **Upgrade: Renovate-like pre-release handling.** Stable manifests exclude all pre-release candidates. Pre-release manifests prefer stable candidates but fall back to newer pre-releases when no stable exists. Pinned mode (`gx upgrade action@v3.0.0-beta.2`) bypasses candidate selection entirely.
- **Domain: fix `Version::precision()` for pre-releases.** Strip the pre-release suffix before counting components so `v3.0.0-beta.2` is recognized as Patch precision. This unblocks pre-release manifests from being upgraded and allows `specifier()` to work for pre-releases.
- **Lock: version/specifier fallback.** When `tags_for_sha` fails or returns no more-specific tag, `resolved_version` falls back to the manifest version instead of staying None.
- **Lock: always serialize all 6 fields.** The serializer outputs `version` and `specifier` unconditionally, using the manifest version as fallback for version and empty string for specifier when not computable (pre-releases).

## Capabilities

### Modified Capabilities

- `version-resolution/spec.md`: Replace "Latest includes pre-releases" with Renovate-like scenarios (stable excludes pre-releases, pre-release prefers stable, pre-release falls back to newer pre-release). Add pre-release precision scenarios.
- `lock-format/spec.md`: Add scenario for fallback when no more-specific tag exists. Clarify pre-release version/specifier behavior.

## Impact

- `crates/gx-lib/src/domain/action.rs` — `find_upgrade_candidate` returns richer type, add pre-release filtering
- `crates/gx-lib/src/commands/upgrade.rs` — use richer return type, conditional manifest update, enrich lock entries
- `crates/gx-lib/src/commands/tidy.rs` — extract `populate_resolved_fields` to shared location
- `crates/gx-lib/src/infrastructure/lock.rs` — unconditional serialization of all 6 fields
- `crates/gx-lib/src/domain/lock.rs` — `LockEntry::set` accepts fallback version
- Spec files: `openspec/specs/version-resolution/spec.md`, `openspec/specs/lock-format/spec.md`
