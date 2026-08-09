//! The audit check identity.
//!
//! Separate from [`crate::diagnostic::RuleName`]: one enum would let a user configure a
//! networked audit check under `[lint.rules]`.

crate::diagnostic::rule_ids! {
    /// Canonical identifier for an audit check. Add one by adding a line.
    CheckName {
        MutableRef => "mutable-ref",
    }
}
