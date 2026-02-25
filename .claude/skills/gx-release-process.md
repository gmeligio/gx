---
name: gx-release-process
description: Use when working on CI/CD, release automation, version bumps, changelog, cargo-dist, release-plz, Homebrew tap, or crates.io publishing for the gx project
---

# gx Release Process

Releases are fully automated via two decoupled tools: **release-plz** manages Release PRs, version bumps, changelog, and git tagging; **cargo-dist** is triggered by the tag push and builds cross-platform binaries, creates the GitHub Release, and pushes the Homebrew formula.

## Pipeline

```
push to main
  → open/update Release PR (version bump + changelog)
  → merge Release PR
  → push version tag
  → build binaries, publish GitHub Release, push Homebrew formula, publish to crates.io
```

## Version Bump Rules

Only `feat:` and `fix:` commits trigger a version bump. All other types (`ci:`, `docs:`, `chore:`, etc.) appear in the changelog and update the Release PR, but do not increment the version.

## Required Secrets

Three secrets are needed in repo settings:
- A fine-grained PAT with write access to both the main repo and the Homebrew tap repo — required so the tag push can trigger downstream workflows
- A crates.io API token scoped to this crate with publish permission
- A token for cargo-dist to push the Homebrew formula to the tap repo
