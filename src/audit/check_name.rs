//! The audit check identity.
//!
//! Deliberately a separate type from [`crate::diagnostic::RuleName`], which identifies lint
//! rules. Sharing one enum would let a user configure a networked audit check under
//! `[lint.rules]` and would put time-varying checks in the same namespace as offline ones —
//! the distinction `gx audit` exists to draw.

crate::diagnostic::rule_ids! {
    /// Canonical identifier for an audit check.
    ///
    /// Adding a check is a one-line edit here: the enum, `as_str`, `ALL`, `Display`,
    /// `FromStr`, `Serialize`, and `Deserialize` are all generated from this list. That
    /// property is what lets several checks be developed in parallel without conflicting.
    CheckName {
        MutableRef => "mutable-ref",
    }
}
