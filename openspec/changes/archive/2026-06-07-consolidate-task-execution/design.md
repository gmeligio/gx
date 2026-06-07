## Context

`gx` runs the same checks (format, clippy, deny, tidy, check, tests) in three environments: a developer's shell, the pre-commit hooks (via `prek`), and CI (`build.yml`). Each check's command currently lives in up to three places, and they have drifted:

| Check | CI (`build.yml`) | pre-commit hook | mise task |
|---|---|---|---|
| format | `cargo fmt --check` | `cargo fmt --all` | `cargo fmt` |
| clippy | `mise run clippy` ✅ | `cargo clippy --fix --allow-dirty` | `cargo clippy --locked --tests --fix --allow-dirty` + `depends=[lint:size, format]` |
| tidy | — | `gx tidy` (installed binary) | `cargo run -- tidy` (from source) |
| check | `cargo check --locked` | — | — |
| deny | `mise run deny` ✅ | `mise run deny` ✅ (pre-push) | `cargo deny --locked check -D warnings` |
| test/integ/e2e | `mise run <task>` ✅ | — | ✅ |

`AGENTS.md` already states the intended principle: *"Always use `mise run <task>` to run project tools, never invoke cargo/clippy/etc directly."* CI is ~70% there; the hooks lag. The drift is not cosmetic — it caused a real CI failure (PR #102): the `cargo-deny` `-D warnings` flag lived only in the mise task, so a stale `windows-sys@0.52` skip entry produced an `unmatched-skip-root` warning that CI promoted to a hard error while a hand-run `cargo deny check` merely warned.

Constraints established during research (all measured/sourced):
- `cargo fmt --all` on this single-crate repo is ~0.48s warm (100 `.rs` files); a per-commit clippy hook is ~3s warm (clippy-dominated).
- CI's parallel job matrix is 152s wall-clock; the serial sum of all jobs is 432s.
- `cargo fmt` is a thin wrapper over the same `rustfmt` engine and is the officially recommended entry point ([Rust Book App. D](https://doc.rust-lang.org/book/appendix-04-useful-development-tools.html), [rustfmt#4485](https://github.com/rust-lang/rustfmt/issues/4485)). The repo already pins `style_edition = "2024"` in `rustfmt.toml`.

## Goals / Non-Goals

**Goals:**
- One definition per check: a mise task. CI and pre-commit both invoke `mise run <task>`. No inline `cargo`/`clippy` in YAML or hook config.
- Each environment runs the verb suited to its job: local + pre-commit MUTATE and re-stage; CI VERIFIES and fails loud without mutating.
- Eliminate the local-vs-CI drift class structurally (changing a flag in a task moves every environment at once).
- Preserve fast per-commit DevEx and CI's parallel per-check failure status.

**Non-Goals:**
- Collapsing CI to a single `prek run --all-files` step (rejected — see Decisions).
- Formatting only staged files via `rustfmt`-direct (rejected — see Decisions).
- Changing any application code, CLI behavior, public API, or dependency.
- Unifying the lockfile hooks (`cargo metadata`, `mise install`) — they have no CI or task counterpart, so there is nothing to consolidate.

## Decisions

### D1. mise tasks are the single source of truth; CI and hooks delegate
Both CI jobs and pre-commit hooks call `mise run <task>`. This is the only structural fix for drift and it satisfies the existing `AGENTS.md` rule. CI YAML becomes pure orchestration (parallel `mise run <task>` jobs).
- *Alternative — keep commands inline, just align flags:* rejected. Relies on the exact human discipline that already failed (PR #102); two places stay in sync only until they don't.

### D2. Mutate vs. verify is split by environment, not unified
Local + pre-commit MUTATE (`cargo fmt --all`, `clippy --fix`) because their job is to *fix* the developer's code; CI VERIFIES with non-mutating `:check` variants because its job is to *gate* an immutable commit — CI has nothing to fix, nowhere to push a fix, and silently rewriting contributor code is an anti-pattern. The `:check` tasks share all formatting/lint configuration with their mutating siblings (same `rustfmt.toml`, same `--all` scope, same lint set); only the verb (`--check` / drop `--fix` + `-D warnings`) differs. This keeps one source of truth *per check* even though two task entrypoints exist.
- *Alternative — single mutating command everywhere, let the runner fail on a dirty tree:* viable but couples "is it formatted?" to "did the runner detect a write?"; the explicit `:check` is clearer and is what CI conventionally expects.

### D3. Pre-commit hooks fix AND auto re-stage (zero-friction commits)
The fmt/clippy hooks run the mutating task then `git add` the result, so a commit that needed formatting succeeds in one shot rather than being rejected and forcing a re-stage. This is the decisive DevEx win — a developer never sees a commit bounced for formatting.

### D4. Use `cargo fmt --all`, not `rustfmt`-direct
Research is conclusive: `cargo fmt` is *"nothing more than a `rustfmt` command builder"* ([rustfmt#4485](https://github.com/rust-lang/rustfmt/issues/4485)) — same engine, no per-file speed difference. The "rustfmt scales better" belief traces to the `cargo fmt --all` registry-index stall ([rustfmt#4247](https://github.com/rust-lang/rustfmt/issues/4247)), a workspace-only target-discovery cost irrelevant to this single crate. `rustfmt`-direct defaults to edition-2015 formatting and requires manual `--edition` sync — which would *reintroduce* drift, the opposite of this change's goal. `cargo fmt --all` matches CI exactly, auto-detects edition, and is the official entry point. The marginal scoping benefit of `rustfmt`-direct is worthless at 0.48s/100 files.

### D5. Keep CI's parallel per-check jobs; reject the `prek run --all-files` collapse
Collapsing CI into one serial `prek run --all-files` step would nearly triple wall-clock (152s → 432s, measured) and replace per-check failure status with a single opaque "prek failed". Heavy checks (test/integ/e2e) do not fit a hook model. The thin-YAML win is not worth the regression; D1 already removes the drift without it.

### D6. Resolve the clippy `depends` double-format
The `clippy` task declares `depends=["lint:size", "format"]`. If both the `cargo-fmt` hook (`mise run format`) and the `cargo-linter` hook (`mise run clippy`, which re-runs `format` via its dependency) fire on the same commit, `format` runs twice. Resolution: drop `format` from the `clippy` task's `depends` so each task does one job, and let the dedicated `cargo-fmt` hook own formatting. CI's `clippy:check` is `--fix`-free and independent, so it is unaffected. (`lint:size` stays as a clippy dependency — it is cheap, ~0.2s, and wanting file-size budgets enforced alongside lint is intentional.)

### D7. The `:check` variants force an all-file directory layout; bare task names become namespaced
This repo defines tasks as **executable files** in `.config/mise/tasks/`; namespaced names come from a subdirectory (the existing `lint:size` is the file `lint/size`). To attach a `:check` variant to `format`/`clippy`, the task must live in a directory — and **mise 2026.6.0 has no "primary file in a task dir" convention**: a directory yields *no* implicit bare task. Empirically verified against mise 2026.6.0:
- `.config/mise/tasks/format` (file) **and** `.config/mise/tasks/format/` (dir) cannot coexist — the filesystem rejects it (`mkdir: File exists`).
- A file literally named `format:check` is **sanitized to `format_check`** (colon → underscore) — wrong name.
- `.config/mise/tasks/format/check` → task `format:check` ✅, but `.config/mise/tasks/format/{format,default}` → `format:format` / `format:default`, **never bare `format`**.

So the bare `format`/`clippy` task names cannot survive a directory layout. **Decision: convert each to a directory holding both variants** — `format/format` (task `format:format`, mutating) + `format/check` (task `format:check`, verify); likewise `clippy/clippy` + `clippy/check`. The bare names are retired and every reference migrates to the namespaced name (CI clippy job, pre-commit hook entries, any `depends`). The migration is mechanical and the blast radius is small — verified by grep, the only references are the clippy job in `build.yml`, the two fmt/clippy hook entries, and the clippy task's own `depends` (which D6 already touches); `AGENTS.md` uses the generic `mise run <task>` and needs no edit; no README/docs/demo.tape use the bare names.
- *Alternative — define the `:check` variants as TOML tasks in `.config/mise.toml`, keeping `format`/`clippy` as bare files:* viable and verified working (bare file task + `[tasks."format:check"]` TOML task coexist, both resolve). **Rejected** to keep a single task-definition style — the repo is currently 100% file-based tasks; mixing in TOML tasks would split the convention and make "where is this task defined?" ambiguous. The all-file directory layout matches the existing `lint/size` precedent. The cost (retiring bare names) is a one-time mechanical rename, paid once.

`check` needs no `:check` sibling — `cargo check` never mutates — so it stays a plain file task with the bare name `check`.

## Risks / Trade-offs

- **Clippy `--fix` auto-applies machine edits on commit** → Mitigation: `clippy --fix` only applies machine-applicable suggestions (conservative by design), and the edits still appear in the staged diff for review before push. Accepted deliberately for consistency with format-on-commit.
- **`git add` re-staging can sweep in an already-dirty-but-unstaged file the formatter touched** → Mitigation: low real-world impact (formatting only rewrites already-near-correct files, and commits are usually coherent sets); this is standard pre-commit behavior. Use `git add -u` (tracked files only) to avoid staging brand-new untracked files.
- **Dropping `format` from clippy's `depends` (D6) means `mise run clippy:clippy` no longer auto-formats first** → Mitigation: intended; formatting is owned by the `format:format` task / `cargo-fmt` hook. A developer running `mise run clippy:clippy` directly who wants formatting runs `mise run format:format` (or relies on the hook). Document if surprising.
- **Two task entrypoints per check (`format:format` / `format:check`) could themselves drift** → Mitigation: they share `rustfmt.toml` and `--all`; the only delta is the `--check` verb, which cannot drift independently because there is no separate config to keep in sync.
- **Retiring the bare `format`/`clippy` names (D7) breaks muscle-memory and any out-of-tree script that calls `mise run format`** → Mitigation: blast radius inside the repo is grep-verified small (CI clippy job, two hook entries, clippy's `depends`); `mise run format` will fail loudly with "task not found" rather than silently mis-run, so any missed reference surfaces immediately. The namespaced names are discoverable via `mise tasks ls`.

## Automated Test Strategy

This change is verified by the CI pipeline it modifies plus local hook exercise; no new application tests:
- **Critical path:** push the branch and confirm every `build.yml` job (Format → `format:check`, Clippy → `clippy:check`, Check → `check`, plus test/integ/e2e/deny) passes via `mise run <task>`.
- **Local verification:** run each `mise run <task>` and each `:check` variant manually; confirm `:check` tasks are non-mutating (clean tree before and after) and mutating tasks fix + the hooks re-stage.
- **Hook verification:** make a deliberately-misformatted staged change, commit, and confirm the commit succeeds in one shot with formatting applied and re-staged (D3). Make a clean change touching no dep files and confirm the deny pre-push hook skips.
- **Drift regression:** confirm there is no inline `cargo`/`clippy` invocation left in `build.yml` (every `run:` is `mise run <task>`), which is the structural guarantee against the PR #102 failure class.

## Observability

- **CI failures surface per-check:** the parallel job matrix (D5) means a red ✗ names exactly which task failed (`Format`, `Clippy`, `Check`, `Deny`, …) rather than one opaque step.
- **`:check` tasks fail loud:** `cargo fmt --all --check` exits non-zero with a diff; `clippy:check` uses `-D warnings`; `deny` uses `-D warnings`. None can pass silently on a violation.
- **Local fast feedback:** the deny pre-push hook (already shipped) catches the supply-chain/skip-root class in ~3s before CI; the fmt/clippy pre-commit hooks catch formatting/lint at commit time.
- **No silent mutation in CI:** because CI runs only `:check` variants, a formatting/lint problem can never be silently "fixed and forgotten" — it is always reported.

## Migration Plan

1. Convert the `format` and `clippy` task files into directories (`format/format`, `clippy/clippy`) and add the `format/check`, `clippy/check`, and `check` tasks; adjust the clippy task `depends` (D6). Migrate all bare-name references to the namespaced names (D7).
2. Update `build.yml` Format/Clippy/Check jobs to `mise run <task>` (`format:check`, `clippy:check`, `check`).
3. Update `.pre-commit-config.yaml` fmt/clippy/tidy hooks to delegate (`mise run format:format`, `mise run clippy:clippy`, `mise run tidy`) + `git add -u`.
4. Verify locally (Automated Test Strategy), then push and confirm all CI jobs green.

Rollback: revert the touched files (the `.config/mise/tasks/` directory moves, `build.yml`, `.pre-commit-config.yaml`); no state or data is migrated, so rollback is a pure git revert with no side effects.
