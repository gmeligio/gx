# Research: Release-plz failure (run 39244136741)

## Root cause

Workflows read `secrets.VERIFIED_COMMIT_ID`; that repo secret no longer exists.
Missing secret → empty string → `actions/create-github-app-token@v2.2.1` gets no
`app-id` → `[@octokit/auth-app] appId option is required`.

Evidence:
- `gh secret list -R gmeligio/gx`: `CARGO_REGISTRY_TOKEN`, `VERIFIED_COMMIT_CLIENT_ID` (2026-07-11), `VERIFIED_COMMIT_KEY`. No `VERIFIED_COMMIT_ID`.
- Last success 31793534297 (2026-08-14) log shows `app-id: ***`; failing run log shows `with:` without `app-id`.
- First failure 37220132550 (2026-10-04, commit 1a4be2f, a `.claude/` change only) → secret deleted between 2026-08-14 and 2026-10-04, not a code regression.
- References: `.github/workflows/release-plz.yml:30`, `:61`, `.github/workflows/release.yml:284` (Homebrew publish will fail on next tag too).

Likely intent: migrate to Client ID. `client-id` input exists from v3.1.0; `app-id` deprecated
(https://github.com/actions/create-github-app-token/blob/v3.3.0/action.yml). Pinned v2.2.1 has only `app-id`.

## Systemic (inline; subagent isolation not used)

- Model: secret name repeated at 3 call sites; no single source.
- Structure: secret rotation half-done (new secret added, old deleted, workflows not updated). No guard fails fast on empty secret.
- gx itself manages these actions; v2.2.1 → v3.3.0 not upgraded (major).

## Options

| Option | Change | Cons |
|---|---|---|
| A. Re-add `VERIFIED_COMMIT_ID` | secret only, no code | keeps deprecated input path |
| B. Bump action to v3.3.0, use `client-id: ${{ secrets.VERIFIED_COMMIT_CLIENT_ID }}` | 3 sites + pin | major bump; check v3 breaking notes |

## Recommendation

B. Matches the already-created `VERIFIED_COMMIT_CLIENT_ID` secret, removes deprecated input, fixes release.yml before next tag.
