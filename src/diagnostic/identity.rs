//! The `rule_ids!` macro: one list of `Variant => "kebab-name"` pairs generates the
//! enum, `as_str`, `ALL`, `Display`, `FromStr`, `Serialize`, and `Deserialize`.
//!
//! One list, because the name a rule is configured by in `[lint.rules]` and the name
//! it prints in output must be the same string. Maintaining those as separate lists
//! makes them agree only by discipline, and that is a user-visible failure when it
//! slips: a rule gx names in output that `gx.toml` then refuses to configure.
//!
//! Adding a rule is a one-line edit here, so concurrent changes that each add one
//! cannot conflict.

/// Define a rule-identity enum from a single list of `Variant => "name"` pairs.
///
/// Generates the enum plus `as_str`, `ALL`, `Display`, `FromStr`, `Serialize`, and
/// `Deserialize` — all reading from that one list, so the name accepted in
/// configuration and the name printed in output cannot drift apart.
///
/// `Serialize` emits a plain string via `serialize_str` rather than a unit variant,
/// because these values are used as `BTreeMap` keys in the `[lint.rules]` table and
/// TOML requires a string in key position.
#[macro_export]
macro_rules! rule_ids {
    (
        $(#[$enum_meta:meta])*
        $name:ident {
            $($variant:ident => $text:literal),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $(
                #[doc = concat!("The `", $text, "` rule.")]
                $variant
            ),+
        }

        impl $name {
            /// Every variant, in declaration order. Iterate this instead of
            /// restating the list, so a rule added later is covered automatically.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The canonical kebab-case name — the single source for both the
            /// configuration key and the rendered output.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|variant| variant.as_str() == s)
                    .ok_or_else(|| format!("unrecognized rule name: {s}"))
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                let raw = <String as ::serde::Deserialize>::deserialize(deserializer)?;
                <Self as ::std::str::FromStr>::from_str(&raw).map_err(::serde::de::Error::custom)
            }
        }
    };
}
