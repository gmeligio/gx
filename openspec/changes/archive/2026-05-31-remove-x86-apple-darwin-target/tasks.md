## 1. Remove the target

- [x] 1.1 Edit `dist-workspace.toml`: remove `"x86_64-apple-darwin"` from the `targets` array (leaving `aarch64-apple-darwin`, the two Linux targets, and Windows)

## 2. Regenerate and verify consistency

- [x] 2.1 Run `dist generate` (cargo-dist 0.31.0, via mise)
- [x] 2.2 Confirm `git diff` touches only `dist-workspace.toml` — `.github/workflows/release.yml` must be unchanged; reconcile any non-target drift if a future cargo-dist version rewrites it
- [x] 2.3 Run `dist plan` and confirm the planned artifacts list exactly 4 targets with no `x86_64-apple-darwin` entry

## 3. Documentation

- [x] 3.1 Add a CHANGELOG entry noting the BREAKING change (Intel-Mac binary / Homebrew install removed; build from source)

## 4. Release commit

- [x] 4.1 Commit with a `feat!:` (or `BREAKING CHANGE:` footer) message so release tooling surfaces the breaking change
