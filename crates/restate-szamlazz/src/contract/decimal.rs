//! Exact input numbers and balance arithmetic. JSON preserves the token through
//! `arbitrary_precision`; buffered Serde wrappers also support strings and exact integers.
use rust_decimal::Decimal;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use std::fmt;

/// Decimal's checked addition may round on success. Align integer coefficients
/// instead, as the Számla Agent's derived line-item arithmetic does. With two
/// 96-bit coefficients, alignment exceeding i128 cannot cancel back into Decimal:
/// the operand already at the common scale still has at most 96 bits.
pub(super) fn exact_add(left: Decimal, right: Decimal) -> Option<Decimal> {
    let result = left.checked_add(right)?;
    let left = left.normalize();
    let right = right.normalize();
    let mut scale = left.scale().max(right.scale());
    let a = left
        .mantissa()
        .checked_mul(10i128.pow(scale - left.scale()))?;
    let b = right
        .mantissa()
        .checked_mul(10i128.pow(scale - right.scale()))?;
    let mut coefficient = a.checked_add(b)?;
    while scale > 0 && coefficient % 10 == 0 {
        coefficient /= 10;
        scale -= 1;
    }
    let exact = Decimal::try_from_i128_with_scale(coefficient, scale).ok()?;
    // Keep the existing result's display/serde scale, but only if it is exact.
    (result == exact).then_some(result)
}

/// The finite textual grammar, before the runtime's exact-representability
/// check. JSON numbers are already constrained by JSON syntax. Explicitly
/// exclude line endings because ECMAScript `$` can match before a final newline.
#[cfg(feature = "schemars")]
pub(super) fn schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": ["string", "number"],
        "pattern": r"^[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?$",
        "not": {"type": "string", "pattern": r"[\r\n\u2028\u2029]"}
    })
}

#[cfg(feature = "schemars")]
pub(super) fn optional_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut schema = schema(generator);
    schema.insert(
        "type".into(),
        serde_json::json!(["string", "number", "null"]),
    );
    schema
}

fn parse<E: serde::de::Error>(text: &str) -> Result<Decimal, E> {
    szamlazz_agent::parse_decimal(text).map_err(|error| {
        E::custom(format!(
            "expected an exactly representable decimal: {error}"
        ))
    })
}

pub(super) fn required<'de, D: Deserializer<'de>>(de: D) -> Result<Decimal, D::Error> {
    struct Exact;
    impl<'de> Visitor<'de> for Exact {
        type Value = Decimal;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an exactly representable decimal string or JSON number")
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
        fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Decimal, E> {
            // With arbitrary_precision, direct JSON text never takes this path.
            // Number's Value deserializer uses it only when its token round-trips
            // through f64's Display unchanged. Other formats supply the float they
            // already hold; their original source precision cannot be recovered.
            parse(&value.to_string())
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Decimal, A::Error> {
            let number =
                serde_json::Number::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
            parse(&number.to_string())
        }
    }
    de.deserialize_any(Exact)
}

pub(super) fn optional<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Decimal>, D::Error> {
    #[derive(Deserialize)]
    struct Exact(#[serde(deserialize_with = "required")] Decimal);
    Ok(Option::<Exact>::deserialize(de)?.map(|value| value.0))
}
