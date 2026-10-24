## 1. Upgrade mise-action

- [ ] 1.1 Run `cargo run -- upgrade --latest jdx/mise-action` and confirm `.github/gx.toml`/`gx.lock` and every workflow `uses:` point at a v5 SHA
- [ ] 1.2 Read the v4.0–v5.2 release notes for inputs the workflows use and adjust any breaking ones (`persist_github_token` only where a later step needs the token)

## 2. Drop the mise version pin

- [ ] 2.1 Remove the `MISE_VERSION` env block, its "Generated from min_version" comment, and every `version: ${{ env.MISE_VERSION }}` input from `build.yml`, `release.yml`, `release-plz.yml`
- [ ] 2.2 Delete `min_version` and its comment from `.config/mise.toml`
- [ ] 2.3 Delete `.config/mise/tasks/mise-version` and the `mise-version` hook in `.pre-commit-config.yaml`

## 3. Make CI read-only on mise.lock

- [ ] 3.1 Delete the `lockfile` job from `build.yml` and `.config/mise/tasks/lock/check`
- [ ] 3.2 Remove `lock:check` from `depends` and the description of `.config/mise/tasks/test/_default`
- [ ] 3.3 Replace `always_run: true` on the `mise-lockfile` hook with `files: ^(\.config/mise\.toml|rust-toolchain\.toml)$` and update its description

## 4. Verify

- [ ] 4.1 Locally: `mise run test` and `prek run --all-files` pass
- [ ] 4.2 Open the PR; every CI job's locked install passes and no job leaves `.config/mise.lock` dirty
- [ ] 4.3 If the locked install of `core:rust` passes in CI, delete `locked = false` and its comment from `.config/mise.toml`; otherwise keep it and record the failure in design.md
