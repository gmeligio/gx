# Tasks

- [x] Move `locked = true` from `mise.test.toml` to `mise.toml`
- [x] Delete `mise.test.toml`
- [x] Add `MISE_DISABLE_TOOLS: ttyd,github:charmbracelet/vhs` env var to all CI workflows that use `jdx/mise-action`
- [x] Remove `MISE_ENV: test` from CI workflows (no longer needed)
