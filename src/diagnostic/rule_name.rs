//! The lint rule identity.
//!
//! Here and not in `lint/` because `config` and `infra::manifest` must name a rule to
//! parse `[lint.rules]`, and neither may depend on a command.

super::identity::rule_ids! {
    /// Canonical identifier for a lint rule. Add one by adding a line.
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
