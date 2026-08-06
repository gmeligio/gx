## Why

A repository where ten workflows all reference `actions/checkout@v4` issues ten identical registry lookups. `gx tidy` and `gx upgrade` re-run the same resolution chains for every duplicate reference, burning API quota and pushing unauthenticated users into the 60 requests/hour rate limit — where the command degrades into skip warnings and an incomplete lock.

Today only *one* of the four `VersionRegistry` methods is deduplicated (`describe_sha`, via `ShaIndex`), and the method that repeats hardest — `all_tags`, called once per manifest spec inside the upgrade loop — is not deduplicated at all.

## What Changes

- Memoize **all four** `VersionRegistry` methods for the duration of a single command run: repeat lookups of the same key are served from memory instead of the network.
- Deduplication moves from an explicitly threaded `&mut ShaIndex` parameter to a caching adapter wrapped around the registry at each command's composition root, so it applies uniformly to every call path — including `all_tags`, which the current mechanism cannot reach.
- **BREAKING (internal only)**: `ShaIndex` is deleted, and the `sha_index: &mut ShaIndex` parameter is dropped from `ActionResolver::resolve_from_sha`, `lock_sync::update_lock`, and `manifest_sync::upgrade_sha_versions_to_tags`. No user-facing API or file format changes.
- No user-visible configuration: caching is unconditional, in-process, and per-run. No TTL, no eviction, no disk cache, no flag.

## Capabilities

### New Capabilities

None. This broadens an existing guarantee rather than introducing a new concept.

### Modified Capabilities

- `action-resolution`: the requirement "SHA descriptions are deduplicated within a single run" currently promises deduplication for SHA descriptions alone. It is replaced by a requirement covering every registry lookup — SHA lookups, tags-for-SHA, all-tags, and SHA descriptions — so users on the unauthenticated rate limit are far less likely to hit it.

**Spec relevance gate.** The proposal rules say "requires spec: adds, removes, or changes user-facing behavior" and "skip spec: internal refactoring with no user-visible change". This is not purely internal: the number of GitHub API requests a run makes is user-visible through the 60 req/hour unauthenticated limit — the same limit `gx` already warns about at startup. A repository that previously exhausted quota mid-run and emitted `ResolutionSkipped` warnings can now complete. The existing spec already treats single-run deduplication as spec-worthy behavior (`action-resolution`, "SHA descriptions are deduplicated within a single run"); leaving that requirement narrower than the shipped behavior would let the spec drift. The delta modifies that one requirement and adds nothing else.

## Impact

- **Code**: new caching adapter under `src/infra/`; `src/domain/action/tag_selection.rs` loses `ShaIndex` (keeps `select_most_specific_tag` and `parse_version_components`); signature changes in `src/domain/resolution.rs`, `src/tidy/lock_sync.rs`, `src/tidy/manifest_sync.rs`; composition roots in `src/init/command.rs`, `src/tidy/command.rs`, `src/upgrade/command.rs` wrap their registry. Net line deletion across roughly 28 call sites, most of them tests.
- **Dependencies**: one new direct dependency under evaluation (`elsa`, MIT/Apache-2.0, no required transitive deps) — see design.md.
- **Behavior**: strictly fewer network requests for the same inputs. Results are identical; no command output changes except that rate-limit warnings become rarer.
- **Sequencing**: this lands before issue #137's retry layer so retry composes over a settled cache boundary rather than being retrofitted into it.
