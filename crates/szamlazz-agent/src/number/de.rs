//! Exact public monetary input. Direct JSON must distinguish actual numbers
//! from objects impersonating `serde_json`'s arbitrary-precision number adapter.
use std::fmt;

use rust_decimal::Decimal;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};

fn parse<E: serde::de::Error>(text: &str) -> Result<Decimal, E> {
    super::parse(text).map_err(|error| {
        E::custom(format!(
            "expected an exactly representable decimal: {error}"
        ))
    })
}

struct Scalar;

impl Visitor<'_> for Scalar {
    type Value = Decimal;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an exactly representable decimal string or number")
    }

    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Decimal, E> {
        parse(text)
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Decimal, E> {
        Ok(value.into())
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Decimal, E> {
        Ok(value.into())
    }

    fn visit_i128<E: serde::de::Error>(self, value: i128) -> Result<Decimal, E> {
        parse(&value.to_string())
    }

    fn visit_u128<E: serde::de::Error>(self, value: u128) -> Result<Decimal, E> {
        parse(&value.to_string())
    }

    fn visit_f32<E: serde::de::Error>(self, value: f32) -> Result<Decimal, E> {
        parse(&value.to_string())
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Decimal, E> {
        // Other formats may already hold floats. Interpret their shortest
        // decimal spelling; source digits lost before this visitor are unknown.
        parse(&value.to_string())
    }
}

pub(crate) fn required<'de, D: Deserializer<'de>>(de: D) -> Result<Decimal, D::Error> {
    struct Exact;

    impl<'de> Visitor<'de> for Exact {
        type Value = Decimal;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            Scalar.expecting(f)
        }

        fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Decimal, E> {
            Scalar.visit_str(text)
        }

        fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Decimal, E> {
            Scalar.visit_i64(value)
        }

        fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Decimal, E> {
            Scalar.visit_u64(value)
        }

        fn visit_i128<E: serde::de::Error>(self, value: i128) -> Result<Decimal, E> {
            Scalar.visit_i128(value)
        }

        fn visit_u128<E: serde::de::Error>(self, value: u128) -> Result<Decimal, E> {
            Scalar.visit_u128(value)
        }

        fn visit_f32<E: serde::de::Error>(self, value: f32) -> Result<Decimal, E> {
            Scalar.visit_f32(value)
        }

        fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Decimal, E> {
            Scalar.visit_f64(value)
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Decimal, A::Error> {
            let raw = Box::<serde_json::value::RawValue>::deserialize(
                serde::de::value::MapAccessDeserializer::new(map),
            )?;
            let text = raw.get();
            match text.as_bytes().first() {
                Some(b'"') => {
                    let text: String =
                        serde_json::from_str(text).map_err(serde::de::Error::custom)?;
                    parse(&text)
                }
                Some(b'-' | b'0'..=b'9') => parse(text),
                _ => Err(serde::de::Error::custom(
                    "expected a decimal string or JSON number",
                )),
            }
        }

        fn visit_newtype_struct<D: Deserializer<'de>>(self, de: D) -> Result<Decimal, D::Error> {
            // Ordinary formats and buffered Serde adapters unwrap newtypes.
            // Reject maps here: a buffered number and a caller's lookalike
            // object are indistinguishable. Strings and integers still compose.
            de.deserialize_any(Scalar)
        }
    }

    if !de.is_human_readable() {
        // Decimal serializes as a string by default, including binary formats
        // that cannot implement deserialize_any.
        return parse(&String::deserialize(de)?);
    }

    // The RawValue newtype protocol asks serde_json for the original token,
    // before arbitrary_precision can turn numbers into private maps. Other
    // formats treat this as an ordinary transparent newtype (the fallback above).
    de.deserialize_newtype_struct("$serde_json::private::RawValue", Exact)
}

pub(crate) fn optional<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Decimal>, D::Error> {
    #[derive(Deserialize)]
    struct Exact(#[serde(deserialize_with = "required")] Decimal);
    Ok(Option::<Exact>::deserialize(de)?.map(|value| value.0))
}
