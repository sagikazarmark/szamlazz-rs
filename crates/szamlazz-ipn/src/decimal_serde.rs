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
    let amount = if let Some((base, exponent)) = text.split_once(['e', 'E']) {
        scientific(base, exponent)
    } else {
        significand(text)
    };
    amount.map(Some).map_err(D::Error::custom)
}

fn significand(text: &str) -> Result<Decimal, rust_decimal::Error> {
    // Redundant fractional zeroes do not consume precision. JSON grammar has
    // already been checked; unlike the form parser, this is typed exact input.
    Decimal::from_str_exact(text).or_else(|error| {
        if text.contains('.') {
            Decimal::from_str_exact(text.trim_end_matches('0').trim_end_matches('.'))
        } else {
            Err(error)
        }
    })
}

fn scientific(base: &str, exponent: &str) -> Result<Decimal, rust_decimal::Error> {
    // Check the significand independently: an exponent must not rescue a base
    // outside Decimal's domain. Normalize *before* adjusting its scale, so
    // 1.00e-28 and 10e-29 fit without either rounding or a false underflow.
    let base = significand(base)?;
    let source_scale = base.scale();
    let mut base = base.normalize();
    if base.is_zero() {
        if let Ok(exponent) = exponent.parse::<i64>()
            && let Some(scale) = i64::from(source_scale).checked_sub(exponent)
            && let Ok(scale) = u32::try_from(scale)
        {
            base.rescale(scale.min(Decimal::MAX_SCALE));
        }
        return Ok(base);
    }
    let exponent = exponent
        .parse::<i64>()
        .map_err(|_| rust_decimal::Error::Underflow)?;
    let mut scale = i64::from(base.scale())
        .checked_sub(exponent)
        .ok_or(rust_decimal::Error::Underflow)?;
    let mut coefficient = base.mantissa();
    while scale > 0 && coefficient % 10 == 0 {
        coefficient /= 10;
        scale -= 1;
    }
    if scale > i64::from(Decimal::MAX_SCALE) {
        return Err(rust_decimal::Error::Underflow);
    }
    if scale < 0 {
        let extra = u32::try_from(scale.unsigned_abs())
            .ok()
            .filter(|extra| *extra <= 28)
            .ok_or(rust_decimal::Error::ExceedsMaximumPossibleValue)?;
        coefficient = coefficient
            .checked_mul(10i128.pow(extra))
            .ok_or(rust_decimal::Error::ExceedsMaximumPossibleValue)?;
    }
    let mut result = Decimal::try_from_i128_with_scale(
        coefficient,
        u32::try_from(scale.max(0)).map_err(|_| rust_decimal::Error::Underflow)?,
    )?;
    if let Some(scale) = i64::from(source_scale).checked_sub(exponent)
        && let Ok(scale) = u32::try_from(scale)
    {
        result.rescale(scale.min(Decimal::MAX_SCALE));
    }
    Ok(result)
}

#[allow(clippy::ref_option)] // Serde's `with` adapter receives a reference to the field.
pub(crate) fn serialize<S: Serializer>(
    value: &Option<Decimal>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    // Pin strings/null independently of rust_decimal's unified serde features.
    value.map(|amount| amount.to_string()).serialize(serializer)
}
