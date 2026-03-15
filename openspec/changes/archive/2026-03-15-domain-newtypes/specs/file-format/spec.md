<!-- MODIFIED: file-format/spec.md -->
<!-- Change: Clarify that forward-compatible reads apply to lock/manifest data sections, not to lint config. -->

### Requirement: Forward-compatible reads

MODIFIED (clarification): The parser SHALL ignore unknown TOML keys and sections without erroring. This applies to `[resolutions]` and `[actions]` sections in the lock file, and unknown top-level sections in the manifest.

This requirement does NOT apply to `[lint.rules]` keys in the manifest. Lint rule names are a closed set deserialized as a `RuleName` enum — unrecognized rule names produce a parse error (see lint delta spec). This distinction is intentional: unknown data fields should be forward-compatible, but misconfigured lint rules should fail early to catch typos.
