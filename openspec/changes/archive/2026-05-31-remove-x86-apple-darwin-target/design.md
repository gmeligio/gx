## Context

`gx` distributes prebuilt binaries via cargo-dist (v0.31.0, pinned through mise). The single source of truth for built platforms is the `targets` array in `dist-workspace.toml`. Everything downstream is generated from it: the release build matrix in `.github/workflows/release.yml` (computed at runtime, no hardcoded triples) and the Homebrew formula in `gmeligio/homebrew-tap`.

Research established two facts about the `x86_64-apple-darwin` target:
- **Usage**: 4 lifetime downloads across all releases, zero since v0.6.0 (~3 months); tied for least-downloaded artifact. Linux GNU is ~90% of all downloads.
- **Cost**: slowest build job on the release critical path (369s / 317s across the two most recent release runs vs. ~240s next-slowest), on the most expensive runner tier (macOS bills ~10.3× Linux per minute). It gates `build-global-artifacts`, so it determines release latency.

## Goals / Non-Goals

**Goals:**
- Stop building and distributing the Intel-Mac (`x86_64-apple-darwin`) binary for future releases.
- Reduce release-build wall-clock (~35%, by removing the critical-path job) and runner cost.

**Non-Goals:**
- Removing or altering any other target (Apple-Silicon macOS, Linux GNU/musl, Windows all stay).
- Modifying past releases or their assets (historical Intel-Mac tarballs remain available).
- Hand-editing `.github/workflows/release.yml` or the Homebrew tap formula (both regenerate from config).
- Adding Intel-Mac support back via emulation or universal binaries.

## Decisions

**Decision: Remove the target by editing `dist-workspace.toml` only; let generation cascade.**
Rationale: The build matrix is computed at runtime from `fromJson(needs.plan.outputs.val).ci.github.artifacts_matrix`; the workflow file hardcodes zero target triples. Empirically verified — running `dist generate` after the edit produces a byte-identical `release.yml` (confirmed against a clean tree). The Homebrew formula regenerates on the next release with the Intel branch dropped automatically.
Alternative considered: manually editing `release.yml` / the tap formula. Rejected — they are generated artifacts; manual edits would drift from config and be overwritten.

**Decision: Run `dist generate` as a verification guard, not a required edit.**
Rationale: It is a no-op for `release.yml` but cheaply proves the config/workflow stay consistent and the cargo-dist plan hash doesn't drift. Cheap insurance against a future cargo-dist version that does embed targets in the workflow.

**Decision: Keep `aarch64-apple-darwin`.**
Rationale: ARM is the future-proof macOS default (all Apple hardware since 2020), builds fast (~132s, not on the critical path), and is the primary Homebrew macOS path. The asymmetry — Intel is *both* low-usage *and* slow/expensive — is the whole argument; ARM shares neither downside.

## Risks / Trade-offs

- **Intel-Mac users lose the prebuilt binary / Homebrew bottle** → Mitigation: documented as a BREAKING change; remaining Intel users can `cargo install` from source. Usage data (0 downloads in 3 months) indicates negligible impact.
- **`dist generate` unexpectedly rewrites `release.yml`** (e.g., after a future cargo-dist bump) → Mitigation: the task diffs the regenerated file; any non-target change is reconciled before commit. For v0.31.0 this is verified to be a no-op.
- **Stale Homebrew tap** (formula currently pinned at v0.5.9) → Out of scope; this change does not fix the tap-update lag, but does not worsen it — the next successful release regenerates the formula without the Intel branch.

## Migration Plan

1. Edit `dist-workspace.toml`: remove `"x86_64-apple-darwin"` from `targets`.
2. Run `dist generate`; confirm `release.yml` is unchanged (or reconcile any non-target drift).
3. Commit. The next tagged release produces 4 platform artifacts and an Intel-free Homebrew formula.

Rollback: re-add `"x86_64-apple-darwin"` to `targets` and re-run `dist generate`. Fully reversible; no data migration.

## Automated Test Strategy

No application code changes, so no unit/integration tests apply. Verification is at the packaging layer:
- **Critical path**: `dist plan` (or `dist generate`) locally must show exactly 4 targets and no `x86_64-apple-darwin` artifacts.
- **Consistency check**: `git diff` after `dist generate` must touch only `dist-workspace.toml` (proving the workflow regenerates identically).
- **End-to-end**: the next real release run is the ultimate verification — the build matrix should have 4 `build-local-artifacts` jobs and the published `gx.rb` should contain no `Hardware::CPU.intel?` macOS branch. This is observed post-merge, not gated in CI.

## Observability

Failures surface through existing channels:
- A misconfigured `targets` array fails fast: `dist generate` / `dist plan` errors locally, and the `plan` job in `release.yml` fails on the next release before any build runs.
- A missing expected artifact would surface as a Homebrew formula referencing a non-existent download URL — caught by the formula-lint step in the publish job. No silent-failure path: the release either produces the 4 expected artifacts or the publish job errors.
