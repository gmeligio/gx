# Contributing

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (edition 2024)
- Git

## Local setup

1. Clone the repository:

    ```bash
    git clone https://github.com/gmeligio/gx.git
    cd gx
    ```

2. Install [prek](https://github.com/nicholasgasior/prek) for git hooks:

    ```bash
    cargo install prek
    ```

3. Install git hooks:

    ```bash
    prek install
    ```

## Building

```bash
cargo build
```

## Testing

```bash
cargo test
```

Tests are organized as:
- **Unit tests** — in each module's `#[cfg(test)]` block
- **Integration tests** — in workflow/manifest/lock modules testing real file I/O with `tempfile`

## Linting

```bash
cargo clippy
```

Clippy pedantic warnings are enabled in `Cargo.toml`. All warnings should be resolved before submitting a PR.

## Architecture

See [docs/development/architecture.md](development/architecture.md) for the layer diagram, trait abstractions, domain types, and how to add a new command.

## Code style

- Use strong domain types (`ActionId`, `Version`, `CommitSha`) instead of raw strings
- Use `thiserror` for module-specific error enums
- Use `anyhow::Result<T>` in command-level code
- Follow existing patterns for trait abstractions (`ManifestStore`, `LockStore`, `VersionRegistry`)
