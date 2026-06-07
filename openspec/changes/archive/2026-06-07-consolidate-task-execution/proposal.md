## Why

Each developer check (format, clippy, deny, tidy) is currently defined in up to three places — CI workflow YAML, the pre-commit hook config, and the mise task — and those definitions have drifted. This drift directly caused a CI failure on PR #102: the `cargo-deny` `-D warnings` flag lived in the mise task, and a stale `windows-sys@0.52` skip entry produced an `unmatched-skip-root` warning that CI's fail-on-warnings config promoted to a hard error, while a local `cargo deny check` only warned. Consolidating every check behind a single mise-task definition makes this class of local-vs-CI drift structurally impossible, and `AGENTS.md` already mandates the principle ("Always use `mise run <task>` to run project tools, never invoke cargo/clippy/etc directly").

## What Changes

- **Establish mise tasks as the single source of truth.** Every check is defined once as a mise task; CI jobs and pre-commit hooks both invoke `mise run <task>` — no inline `cargo`/`clippy` commands anywhere.
- **Split mutate vs. verify by environment.** Local + pre-commit MUTATE (`cargo fmt --all`, `cargo clippy --fix`) and auto re-stage with `git add` so commits succeed clean in one shot; CI VERIFIES with non-mutating `:check` task variants that fail loud and never touch the tree.
- **Add `:check` task variants:** `format:check` (`cargo fmt --all --check`), `clippy:check` (`cargo clippy --locked --tests -- -D warnings`, no `--fix`), and a `check` task (`cargo check --locked`). Because mise 2026.6.0 derives a namespaced task name from a task *directory* and has no bare/"primary" file inside one, the existing `format` and `clippy` file-tasks move into directories alongside their `:check` siblings — so the mutating tasks become `format:format` and `clippy:clippy`, and the bare `format`/`clippy` names are retired (see design D7). `check` stays a bare file task (no `:check` sibling — `cargo check` never mutates).
- **Convert `build.yml`** so every job runs `mise run <task>` (Format → `format:check`, Clippy → `clippy:check`, Check → `check`; test/integ/e2e/deny already delegate).
- **Point pre-commit hooks at their tasks** with re-staging: `cargo-fmt` → `mise run format:format` + `git add`, `cargo-linter` → `mise run clippy:clippy` + `git add`, `gx-lockfile` → `mise run tidy` (fixes a latent stale-installed-binary bug; the hook currently runs the globally-installed `gx`, not the source under commit).
- Resolve the redundancy where the `clippy` task's `depends=["lint:size", "format"]` would double-run `format` if both the `cargo-fmt` and `cargo-linter` hooks fire.
- Use `cargo fmt --all` (not `rustfmt`-direct): research confirms `cargo fmt` is a thin wrapper over the same `rustfmt` engine, is the officially recommended entry point, auto-detects edition, and avoids the drift `rustfmt`-direct's edition-2015 default would reintroduce.

No **BREAKING** user-facing changes — this is dev tooling only.

## Capabilities

### New Capabilities
- `task-execution-consistency`: every developer check is defined once as a mise task and invoked identically by local shell, pre-commit hooks, and CI, so a check's verdict cannot differ between a contributor's machine and CI. The contributor is the user (cf. the existing `lockfile-integrity` capability, which is likewise contributor-facing).

### Modified Capabilities
<!-- None. The existing user-facing CLI specs (action-resolution, command-output,
     lint-command, manifest-and-lock, upgrade-operations, homebrew-release) are
     untouched. lockfile-integrity is a sibling contributor-facing capability and
     is not modified — its lockfile hooks stay inline. -->

## Impact

- **`.config/mise/tasks/`** — the `format` and `clippy` task files become directories (`format/format`, `clippy/clippy`) so each can hold a `:check` sibling (`format/check`, `clippy/check`); a new bare `check` file-task is added. Bare `format`/`clippy` task names are retired in favor of `format:format`/`clippy:clippy`. The `clippy:clippy` task's `depends` is reconsidered to avoid double-formatting.
- **`.github/workflows/build.yml`** — Format, Clippy, and Check jobs switch from inline `cargo` commands to `mise run <task>`. Parallel per-job structure is preserved (collapsing to a single serial `prek run --all-files` step would nearly triple CI wall-clock, 152s → 432s, and lose per-check failure status).
- **`.pre-commit-config.yaml`** — `cargo-fmt`, `cargo-linter`, `gx-lockfile` hooks delegate to mise tasks and add `git add` re-staging. (`cargo-deny` pre-push hook already delegates; the `cargo-lockfile`/`mise-lockfile` hooks stay inline as they have no task or CI counterpart.)
- No changes to application code, CLI behavior, public APIs, or dependencies.
