# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.7](https://github.com/gmeligio/gx/compare/v0.5.6...v0.5.7) - 2026-03-03

### Added

- use verified GitHub App commits for Homebrew formula push ([#47](https://github.com/gmeligio/gx/pull/47))
- SHA-first lock resolution from workflow SHAs ([#45](https://github.com/gmeligio/gx/pull/45))

## [0.5.6](https://github.com/gmeligio/gx/compare/v0.5.5...v0.5.6) - 2026-03-02

### Fixed

- update release_commits ([#43](https://github.com/gmeligio/gx/pull/43))
- correct release-plz regex ([#42](https://github.com/gmeligio/gx/pull/42))
- use iterators ([#41](https://github.com/gmeligio/gx/pull/41))

## [0.5.5](https://github.com/gmeligio/gx/compare/v0.5.4...v0.5.5) - 2026-03-01

### Fixed

- use manifest as source of truth ([#38](https://github.com/gmeligio/gx/pull/38))

## [0.5.4](https://github.com/gmeligio/gx/compare/v0.5.3...v0.5.4) - 2026-02-28

### Added

- add lint command ([#33](https://github.com/gmeligio/gx/pull/33))

### Other

- read config from a single place ([#31](https://github.com/gmeligio/gx/pull/31))
- split into cargo workspace (gx + gx-lib) ([#30](https://github.com/gmeligio/gx/pull/30))

## [0.5.3] - 2026-02-26

### Features

- Add actions overrides ([#19](https://github.com/gmeligio/gx/pull/19))

### Bug Fixes

- Serialize action overrides with correct TOML  ([#21](https://github.com/gmeligio/gx/pull/21))

### Documentation

- Split agents.md into skills ([#22](https://github.com/gmeligio/gx/pull/22))

### Miscellaneous

- Automated releases ([#20](https://github.com/gmeligio/gx/pull/20))
- Use github app token ([#23](https://github.com/gmeligio/gx/pull/23))
