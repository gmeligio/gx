//! The `permissions:` block and its three YAML shapes. Extracted from the parse
//! module so that module stays within the file-size budget.

use serde::Deserialize;
use serde::de::{Deserializer, MapAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;

/// Access level for a single permission scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Read,
    Write,
    None,
}

/// A workflow's `permissions:` block, in one of GitHub's three shapes:
/// `read-all`, `write-all`, or a per-scope map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Permissions {
    ReadAll,
    WriteAll,
    /// Empty `permissions: {}` — drops all defaults.
    Empty,
    Specific(BTreeMap<String, Access>),
}

impl Permissions {
    /// True when this block grants anything broader than `contents: read`.
    #[must_use]
    pub fn is_excessive(&self) -> bool {
        match self {
            Self::WriteAll | Self::ReadAll => true,
            Self::Empty => false,
            Self::Specific(map) => map.iter().any(|(scope, access)| {
                !(scope == "contents" && matches!(access, Access::Read | Access::None))
            }),
        }
    }

    /// True when this block grants any write scope.
    #[must_use]
    pub fn has_write(&self) -> bool {
        match self {
            Self::WriteAll => true,
            Self::ReadAll | Self::Empty => false,
            Self::Specific(map) => map.values().any(|a| matches!(a, Access::Write)),
        }
    }
}

impl<'de> Deserialize<'de> for Permissions {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Permissions;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("\"read-all\", \"write-all\", or a per-scope map")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Permissions, E> {
                match v {
                    "read-all" => Ok(Permissions::ReadAll),
                    "write-all" => Ok(Permissions::WriteAll),
                    other => Err(E::custom(format!("unknown permissions shorthand: {other}"))),
                }
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Permissions, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((k, v)) = map.next_entry::<String, Access>()? {
                    out.insert(k, v);
                }
                if out.is_empty() {
                    Ok(Permissions::Empty)
                } else {
                    Ok(Permissions::Specific(out))
                }
            }
        }
        de.deserialize_any(V)
    }
}
