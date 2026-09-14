//! Named request records accept maps only, including when nested in another input.

use serde::de::{Deserializer, MapAccess, Visitor};
use std::fmt;

/// Adapt only the record boundary. Field values go straight to their own
/// deserializers: no Value/Content buffer that could erase duplicate keys or
/// change exact numeric tokens, and no recursive ban on legitimate Vec arrays.
pub(super) struct Object<D>(pub D);

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Object<D> {
    type Error = D::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
        self.0.deserialize_map(MapOnly(visitor))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        self.deserialize_any(visitor)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map enum identifier ignored_any
    }
}

struct MapOnly<V>(V);

impl<'de, V: Visitor<'de>> Visitor<'de> for MapOnly<V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an object")
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.0.visit_map(map)
    }
}

/// Keep one record definition for serialization, discovery and decoding. Also
/// used for internally tagged input enums, whose map payload Serde validates.
/// The remote helper constructs the public type directly, retaining Serde's field
/// defaults, unknown-field refusal and duplicate-field checks. Its schema derive
/// merely accepts the shared field-level schemars attributes; discovery uses
/// the public type's schema.
macro_rules! object_input {
    (
        $(#[$attr:meta])*
        $vis:vis $kind:ident $name:ident {
            $($fields:tt)*
        }
    ) => {
        $(#[$attr])*
        $vis $kind $name { $($fields)* }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                type Remote = $name;
                // Copy the container attributes too: notably serde(default)
                // must use the same Default as the public record.
                $(#[$attr])*
                #[derive(serde::Deserialize)]
                #[serde(remote = "Remote")]
                #[allow(dead_code)]
                $kind Wire { $($fields)* }

                Wire::deserialize($crate::contract::object::Object(deserializer))
            }
        }
    };
}

pub(super) use object_input;
