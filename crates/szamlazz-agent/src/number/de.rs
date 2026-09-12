//! Exact public monetary input. Direct JSON must distinguish actual numbers
//! from objects impersonating `serde_json`'s arbitrary-precision number adapter.
use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, de::Visitor};

/// Serde has no format/capability query for raw JSON tokens. Keep the private
/// `RawValue` protocol confined to `serde_json`'s concrete deserializers, rather
/// than imposing JSON's transparent-newtype convention on every text format.
/// `type_name` is used only for dispatch, never for trusting a map's contents.
/// An unrecognized adapter gets the conservative scalar path (maps refused).
pub(crate) fn is_json<D>() -> bool {
    let name = std::any::type_name::<D>();
    name.starts_with("&mut serde_json::de::Deserializer<")
        || name == std::any::type_name::<serde_json::Value>()
        || name == std::any::type_name::<&serde_json::Value>()
}

/// `serde_ignored` 0.1 forwards `deserialize_newtype_struct` and its name
/// unchanged to its inner deserializer; its visitor/seed wrappers do not buffer
/// tokens. Restrict this exception to a directly wrapped `serde_json` parser.
/// A wrapper over Serde Content (or RON) still cannot supply raw JSON.
///
/// Only scalars use this exception: capturing an entire struct as `RawValue`
/// would hide its ignored fields from the callback (notably a storno envelope).
fn has_raw_json_scalar<D>() -> bool {
    is_json::<D>()
        || std::any::type_name::<D>()
            .strip_prefix("serde_ignored::Deserializer<")
            .is_some_and(|inner| {
                // type_name may display the wrapper's two erased lifetimes.
                let inner = inner.strip_prefix("'_, '_, ").unwrap_or(inner);
                inner.starts_with("&mut serde_json::de::Deserializer<")
            })
}

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
    if !de.is_human_readable() {
        // Monetary serialization is pinned to strings, including binary formats
        // that cannot implement deserialize_any.
        return parse(&String::deserialize(de)?);
    }

    if has_raw_json_scalar::<D>() {
        // Ask for the original token before arbitrary_precision can turn it
        // into a map indistinguishable from a caller's lookalike object.
        let raw = Box::<serde_json::value::RawValue>::deserialize(de)?;
        match raw.get().as_bytes().first() {
            Some(b'"') => {
                let text: String =
                    serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
                parse(&text)
            }
            Some(b'-' | b'0'..=b'9') => parse(raw.get()),
            _ => Err(serde::de::Error::custom(
                "expected a decimal string or JSON number",
            )),
        }
    } else {
        de.deserialize_any(Scalar)
    }
}

pub(crate) fn optional<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Decimal>, D::Error> {
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Exact(#[serde(deserialize_with = "required")] Decimal);
    Ok(Option::<Exact>::deserialize(de)?.map(|value| value.0))
}
