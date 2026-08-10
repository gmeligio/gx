## 1. Domain type

- [x] 1.1 Create `src/domain/remediation.rs` with the `Remediation` enum
      (`Upgradable { fixed }`, `NoFixAvailable`, `OutOfRange { fixed }`), with
      doc comments on the type and every variant and field (clippy requires
      private-item and field docs).
- [x] 1.2 Declare `pub mod remediation;` in `src/domain/mod.rs`, keeping the
      module list alphabetical.

## 2. Classification logic

- [x] 2.1 Widen `parse_semver` in `src/domain/action/specifier.rs` from
      `pub(super)` to `pub(crate)` so `src/domain/remediation.rs` can reuse the
      crate's single semver parser instead of duplicating it.
- [x] 2.2 Implement the constructor taking the manifest `Specifier` and an
      `Option<&str>` advisory first-patched identifier; an absent **or
      unparseable** identifier ⇒ `NoFixAvailable`, so `fixed` is never carried
      as an uninterpretable string.
- [x] 2.3 Canonicalize `fixed` from the **parsed** semver
      (`Version::normalized(&parsed.to_string())`), not the raw identifier, so
      an uppercase `V` and an imprecise `2.37` both reach the user as `v46.0.1`
      and `v2.37.0`.
- [x] 2.6 Filter out prerelease patched versions to `NoFixAvailable`; no range
      admits them, so `OutOfRange`'s "requires a major bump" would be false.
- [x] 2.4 For a `Specifier::Range`, delegate the reachability test to
      `Specifier::matches_version(&ResolvedRef::Tag(fixed))`; in range ⇒
      `Upgradable`, otherwise ⇒ `OutOfRange`.
- [x] 2.5 For `Specifier::Ref` and `Specifier::Sha`, classify as `OutOfRange` —
      `gx upgrade` has no range to search. Document on the arm why this inverts
      `matches_version`'s "exempt" answer for the same specifiers.

## 3. Tests

- [x] 3.1 `Upgradable` against the real cases: `^46` + `46.0.1` (tj-actions) and
      `~2.37` + `2.37.1` (setup-php).
- [x] 3.2 `OutOfRange` against the real cases: `^2` + `3.0.0` (codeql-action,
      major bump) and `^0.34` + `0.35.0` (trivy-action, 0.x caret patch-locked).
- [x] 3.3 `NoFixAvailable` for an absent identifier, using the real
      `reviewdog/action-setup` shape (advisory range `= 1`, no
      `firstPatchedVersion`; the specifier is never consulted).
- [x] 3.4 `v`-prefix tolerance: `v46.0.1` reaches the same verdict as `46.0.1`,
      and the `fixed` value is `v`-prefixed in both. Also uppercase `V46.0.1`
      canonicalized to `v46.0.1`, and imprecise `2.37` padded to `v2.37.0`.
- [x] 3.7 A prerelease patched version (`^2` + `2.1.0-beta.1`) classifies as
      `NoFixAvailable`.
- [x] 3.5 No upgrade suggested for a branch specifier (`main`) or a bare 40-hex
      SHA specifier, even with a patched version present.
- [x] 3.6 An unparseable patched identifier classifies as `NoFixAvailable` —
      not `Upgradable`, and specifically not `OutOfRange`, which would claim a
      fix exists beyond the user's range when none is known.

## 4. Verification

- [x] 4.1 `mise run test` passes, including `tests/code_health.rs` with no budget
      number changed (`src/domain/` must stay within the 8-file budget and the
      new file within 440 logic / 550 total lines).
- [x] 4.2 Commit on the worktree branch with a Conventional Commits title.
