## 1. Domain type

- [ ] 1.1 Create `src/domain/remediation.rs` with the `Remediation` enum
      (`Upgradable { fixed }`, `NoFixAvailable`, `OutOfRange { fixed }`), with
      doc comments on the type and every variant and field (clippy requires
      private-item and field docs).
- [ ] 1.2 Declare `pub mod remediation;` in `src/domain/mod.rs`, keeping the
      module list alphabetical.

## 2. Classification logic

- [ ] 2.1 Widen `parse_semver` in `src/domain/action/specifier.rs` from
      `pub(super)` to `pub(crate)` so `src/domain/remediation.rs` can reuse the
      crate's single semver parser instead of duplicating it.
- [ ] 2.2 Implement the constructor taking the manifest `Specifier` and an
      `Option<&str>` advisory first-patched identifier; an absent **or
      unparseable** identifier ⇒ `NoFixAvailable`, so `fixed` is never carried
      as an uninterpretable string.
- [ ] 2.3 Normalize the identifier through `Version::normalized` so the `fixed`
      value carried in the enum is `v`-prefixed regardless of advisory form.
- [ ] 2.4 For a `Specifier::Range`, delegate the reachability test to
      `Specifier::matches_version(&ResolvedRef::Tag(fixed))`; in range ⇒
      `Upgradable`, otherwise ⇒ `OutOfRange`.
- [ ] 2.5 For `Specifier::Ref` and `Specifier::Sha`, classify as `OutOfRange` —
      `gx upgrade` has no range to search. Document on the arm why this inverts
      `matches_version`'s "exempt" answer for the same specifiers.

## 3. Tests

- [ ] 3.1 `Upgradable` against the real cases: `^46` + `46.0.1` (tj-actions) and
      `~2.37` + `2.37.1` (setup-php).
- [ ] 3.2 `OutOfRange` against the real cases: `^2` + `3.0.0` (codeql-action,
      major bump) and `^0.34` + `0.35.0` (trivy-action, 0.x caret patch-locked).
- [ ] 3.3 `NoFixAvailable` for an absent identifier, using the real
      `reviewdog/action-setup` shape (advisory range `= 1`, no
      `firstPatchedVersion`; the specifier is never consulted).
- [ ] 3.4 `v`-prefix tolerance: `v46.0.1` reaches the same verdict as `46.0.1`,
      and the `fixed` value is `v`-prefixed in both.
- [ ] 3.5 No upgrade suggested for a branch specifier (`main`) or a bare 40-hex
      SHA specifier, even with a patched version present.
- [ ] 3.6 An unparseable patched identifier classifies as `NoFixAvailable` —
      not `Upgradable`, and specifically not `OutOfRange`, which would claim a
      fix exists beyond the user's range when none is known.

## 4. Verification

- [ ] 4.1 `mise run test` passes, including `tests/code_health.rs` with no budget
      number changed (`src/domain/` must stay within the 8-file budget and the
      new file within 440 logic / 550 total lines).
- [ ] 4.2 Commit on the worktree branch with a Conventional Commits title.
