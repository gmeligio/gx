use std::collections::HashMap;

use super::{ActionId, ActionSpec, Version};

/// Domain entity owning the manifest's action→version mapping and all domain behaviour.
/// No I/O — persistence is handled by infrastructure's `ManifestStore` trait.
#[derive(Debug, Default)]
pub struct Manifest {
    actions: HashMap<ActionId, ActionSpec>,
}

impl Manifest {
    /// Create a `Manifest` from an existing map of IDs to specs.
    #[must_use]
    pub fn new(actions: HashMap<ActionId, ActionSpec>) -> Self {
        Self { actions }
    }

    /// Get the version pinned for an action.
    #[must_use]
    pub fn get(&self, id: &ActionId) -> Option<&Version> {
        self.actions.get(id).map(|s| &s.version)
    }

    /// Set or update the version for an action.
    pub fn set(&mut self, id: ActionId, version: Version) {
        let spec = ActionSpec::new(id.clone(), version);
        self.actions.insert(id, spec);
    }

    /// Remove an action from the manifest.
    pub fn remove(&mut self, id: &ActionId) {
        self.actions.remove(id);
    }

    /// Check if the manifest contains an action.
    #[must_use]
    pub fn has(&self, id: &ActionId) -> bool {
        self.actions.contains_key(id)
    }

    /// Check if the manifest has no actions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Get all action specs.
    #[must_use]
    pub fn specs(&self) -> Vec<&ActionSpec> {
        self.actions.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, Version};

    #[test]
    fn test_set_and_get() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        assert_eq!(
            m.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
    }

    #[test]
    fn test_has_and_remove() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        assert!(m.has(&ActionId::from("actions/checkout")));
        m.remove(&ActionId::from("actions/checkout"));
        assert!(!m.has(&ActionId::from("actions/checkout")));
    }

    #[test]
    fn test_is_empty() {
        let mut m = Manifest::default();
        assert!(m.is_empty());
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        assert!(!m.is_empty());
    }

    #[test]
    fn test_specs() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        m.set(ActionId::from("actions/setup-node"), Version::from("v3"));
        assert_eq!(m.specs().len(), 2);
    }
}
