---
name: gx-project-overview
description: Use when needing project context, build commands, environment setup, or logging configuration for the gx CLI tool
---

gx is a Rust CLI that manages Github Actions dependencies across workflows, similar to `go mod tidy`. It can maintain `.github/gx.toml` (manifest) and `.github/gx.lock` (lock file), or run in memory-only mode.
