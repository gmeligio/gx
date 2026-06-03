//! Shared custom deserializers for the workflow parse model.

use serde::de::{Deserializer, Visitor};
use std::fmt;

/// Deserializes `needs:` in either the scalar (`needs: build`) or sequence
/// (`needs: [build, test]`) form into a `Vec<String>`. Mirrors the custom-deserialize
/// pattern `JobSecrets` uses for its scalar-or-map union.
pub(super) fn deserialize_needs<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<String>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a job id or a list of job ids")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            Ok(vec![v.to_owned()])
        }
        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Vec<String>, E> {
            Ok(vec![v])
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Vec<String>, A::Error> {
            let mut out = Vec::new();
            while let Some(id) = seq.next_element::<String>()? {
                out.push(id);
            }
            Ok(out)
        }
    }
    de.deserialize_any(V)
}
