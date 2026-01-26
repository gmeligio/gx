# Implementation Plan: gx CLI Tool

## Overview

A Rust CLI tool to manage GitHub Actions dependency versions across workflows.

## Phase 1: MVP - `gx set` Command

Focus on the simplest case as outlined in architecture.md.

### 1.1 Project Setup

- [ ] Initialize Rust project with `cargo init`
- [ ] Add dependencies to Cargo.toml:
  - `clap` - CLI argument parsing
  - `toml` - TOML parsing for manifest and workflow files
  - `serde` - Serialization/deserialization
  - `glob` - File pattern matching for workflows
  - `anyhow` - Error handling

### 1.2 Manifest File Structure

Location: `.github/gx.toml`

```toml
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
"docker/build-push-action" = "v5"
```

### 1.3 Core Implementation

**Files to create:**
- `src/main.rs` - CLI entry point with clap
- `src/manifest.rs` - Parse and manage `.github/gx.toml`
- `src/workflow.rs` - Parse and update GitHub workflow YAML files
- `src/commands/mod.rs` - Command module
- `src/commands/set.rs` - `gx set` command implementation

**`gx set` command behavior:**
1. Read `.github/gx.toml` manifest
2. Find all `.github/workflows/*.yml` files
3. Parse each workflow file
4. Update action versions (e.g., `uses: actions/checkout@v3` → `uses: actions/checkout@v4`)
5. Write updated files back

### 1.4 Workflow Parsing Strategy

GitHub workflow files use YAML. For each job step with `uses:`:
```yaml
steps:
  - uses: actions/checkout@v3  # Target for replacement
```

Parse the `uses` field format: `{owner}/{repo}@{version}`

## Phase 2: `gx discover` Command

### 2.1 Implementation

**Files to create/modify:**
- `src/commands/discover.rs` - `gx discover` command

**Behavior:**
1. Scan `.github/workflows/*.yml` for action versions
2. Scan `.github/actions/**/*.yml` if exists
3. Extract unique action references
4. Merge with existing `.github/gx.toml` (preserve existing entries)
5. Write updated manifest

## Phase 3: Enhanced Configuration

### 3.1 Per-Workflow Overrides

Extend manifest to support workflow-specific versions:

```toml
[actions]
"actions/checkout" = "v4"

[workflows."ci.yml".actions]
"actions/checkout" = "v3"  # Override for specific workflow
```

## Verification

- [ ] Unit tests for manifest parsing
- [ ] Unit tests for workflow YAML parsing
- [ ] Integration tests with sample workflow files
- [ ] Manual testing with real GitHub workflows

## Critical Files

- `Cargo.toml` - Project configuration
- `src/main.rs` - CLI entry point
- `src/manifest.rs` - Manifest handling
- `src/workflow.rs` - Workflow file manipulation
- `src/commands/set.rs` - Set command logic
