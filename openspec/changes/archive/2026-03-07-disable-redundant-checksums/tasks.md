## 1. Disable checksum generation

- [x] 1.1 Add `checksum = "false"` to the `[dist]` section in `dist-workspace.toml`

## 2. Verification

- [x] 2.1 Run `dist plan` locally and verify no `.sha256` sidecar files or `sha256.sum` appear in the output

## 3. Documentation

- [x] 3.1 Update README if it references `.sha256` sidecar files or manual checksum verification steps
