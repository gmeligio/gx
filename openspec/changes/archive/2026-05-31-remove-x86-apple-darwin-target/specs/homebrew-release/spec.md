## ADDED Requirements

### Requirement: macOS distribution targets Apple Silicon only

The Homebrew formula SHALL provide a macOS binary for Apple Silicon (`aarch64-apple-darwin`) only. Intel (`x86_64-apple-darwin`) macOS binaries SHALL NOT be distributed; Intel-Mac users obtain `gx` by building from source.

#### Scenario: Apple Silicon user installs gx via Homebrew

- **GIVEN** a released formula
- **WHEN** an Apple Silicon (arm64) macOS user runs `brew install <tap>/gx`
- **THEN** the formula resolves to the `aarch64-apple-darwin` binary with a matching checksum

#### Scenario: Intel macOS user has no prebuilt binary

- **GIVEN** a released formula
- **WHEN** an Intel (x86_64) macOS user attempts to install gx via Homebrew
- **THEN** no Intel macOS binary is offered by the formula
- **AND** the user builds from source via `cargo install` instead
