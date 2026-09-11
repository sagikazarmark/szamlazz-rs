//! Exact JSON monetary fields. Read original tokens rather than floats or
//! `serde_json`'s feature-dependent number maps. Form parsing remains separate.

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::value::RawValue;

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Decimal>, D::Error> {
    let Some(raw) = Option::<Box<RawValue>>::deserialize(de)? else {
        return Ok(None);
    };
    let text = raw.get();
    let owned;
    let text = if text.starts_with('"') {
        owned = serde_json::from_str::<String>(text).map_err(D::Error::custom)?;
        &owned
    } else {
        text
    };
    // Validate scalar grammar too: direct JSON objects, including private
    // Number/RawValue lookalikes, are not monetary values.
    text.parse::<serde_json::Number>()
        .map_err(D::Error::custom)?;
    let amount = if let Some((base, _)) = text.split_once(['e', 'E']) {
        // from_scientific uses a rounding parser for its base. Validate that
        // base exactly first; the non-lossy exponent adjustment then either
        // fits or errors. Never use from_scientific_lossy.
        Decimal::from_str_exact(base).and_then(|_| Decimal::from_scientific(text))
    } else {
        Decimal::from_str_exact(text)
    };
    amount.map(Some).map_err(D::Error::custom)
}

#[allow(clippy::ref_option)] // Serde's `with` adapter receives a reference to the field.
pub(crate) fn serialize<S: Serializer>(
    value: &Option<Decimal>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    // Pin strings/null independently of rust_decimal's unified serde features.
    value.map(|amount| amount.to_string()).serialize(serializer)
}
