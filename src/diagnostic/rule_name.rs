//! The lint rule identity.
//!
//! Defined alongside the shared diagnostics vocabulary rather than inside `lint/`
//! because `config` and `infra::manifest` must name a rule to parse `[lint.rules]`,
//! and neither may depend on a command module. `lint::RuleName` re-exports this.

super::identity::rule_ids! {
    /// Canonical identifier for a lint rule.
    ///
    /// Adding a rule is a one-line edit here: the enum, `as_str`, `ALL`, `Display`,
    /// `FromStr`, `Serialize`, and `Deserialize` are all generated from this list.
    RuleName {
        ShaMismatch => "sha-mismatch",
        Unpinned => "unpinned",
        StaleComment => "stale-comment",
        UnsyncedManifest => "unsynced-manifest",
        MissingPermissions => "missing-permissions",
        ExcessivePermissions => "excessive-permissions",
        DangerousTrigger => "dangerous-trigger",
        PrHeadCheckout => "pr-head-checkout",
        MissingConcurrency => "missing-concurrency",
        UnprotectedSecrets => "unprotected-secrets",
        DanglingReference => "dangling-reference",
        InvalidExpression => "invalid-expression",
        RunShellcheck => "run-shellcheck",
    }
}
