//! The audit check identity.
//!
//! Deliberately a separate type from [`crate::diagnostic::RuleName`], which identifies lint
//! rules. Sharing one enum would let a user configure a networked audit check under
//! `[lint.rules]` and would put time-varying checks in the same namespace as offline ones —
//! the distinction `gx audit` exists to draw.

crate::diagnostic::rule_ids! {
    /// Canonical identifier for an audit check. Add one by adding a line.
    CheckName {
        MutableRef => "mutable-ref",
    }
}
