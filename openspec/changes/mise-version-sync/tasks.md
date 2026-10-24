## 1. Upgrade mise-action

- [x] 1.1 Run `cargo run -- upgrade --latest jdx/mise-action` and confirm `.github/gx.toml`/`gx.lock` and every workflow `uses:` point at a v5 SHA
- [x] 1.2 Confirm with `grep -A3 jdx/mise-action .github/workflows/*.yml` that no step passes a mise-action input other than `version` (removed in 2.1). Confirm the GitHub-API steps still set their own token: `mise run e2e` (`GITHUB_TOKEN`), release-plz (app token), `release.yml` (`GH_TOKEN`)

## 2. Drop the mise version pin

- [x] 2.1 In every workflow that uses `jdx/mise-action` (today `build.yml`, `release.yml`, `release-plz.yml`): replace the `MISE_VERSION` env entry and its "Generated from min_version" comment with `MISE_LOCKED: 1`, and delete every `version: ${{ env.MISE_VERSION }}` input
- [x] 2.2 Delete `min_version` and its comment from `.config/mise.toml`
- [x] 2.3 Delete `.config/mise/tasks/mise-version` and the `mise-version` hook in `.pre-commit-config.yaml`

## 3. Make CI read-only on mise.lock

- [x] 3.1 Delete the `lockfile` job from `build.yml` and `.config/mise/tasks/lock/check`
- [x] 3.2 Remove `lock:check` from `depends` and the description of `.config/mise/tasks/test/_default`
- [ ] 3.3 Replace `always_run: true` on the `mise-lockfile` hook with `files: ^(\.config/mise\.(toml|lock)|rust-toolchain\.toml)$` and update its description

## 4. Verify

- [ ] 4.1 Locally: `mise run test` and `prek run --all-files` pass
- [ ] 4.2 Open the PR; every CI job's locked install passes and no job leaves `.config/mise.lock` dirty
- [ ] 4.3 If the locked install of `core:rust` passes in CI, delete `locked = false` and its comment from `.config/mise.toml`; otherwise pause and revise design.md
- [ ] 4.4 After merge, confirm the first release-plz run on main passes; open a GitHub issue to check the macOS/Windows `release.yml` jobs on the next tag
