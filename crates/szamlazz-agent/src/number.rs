//! Exact conversion of finite wire numbers, independent of their spelling.
use rust_decimal::Decimal;

/// Checked Decimal operations may round on success. Work with integer
/// coefficients instead, refusing any intermediate that cannot fit exactly.
pub(crate) fn exact_mul(left: Decimal, right: Decimal) -> Option<Decimal> {
    let left = left.normalize();
    let right = right.normalize();
    let (mut a, mut b) = (left.mantissa(), right.mantissa());
    let mut scale = left.scale() + right.scale();
    // Cancel powers of ten before multiplying: a 192-bit product may still
    // fit Decimal once its trailing zeroes are removed. The factors of ten
    // can straddle the operands (e.g. 2 × 5).
    while scale > 0 {
        if a % 10 == 0 {
            a /= 10;
        } else if b % 10 == 0 {
            b /= 10;
        } else if a % 2 == 0 && b % 5 == 0 {
            a /= 2;
            b /= 5;
        } else if a % 5 == 0 && b % 2 == 0 {
            a /= 5;
            b /= 2;
        } else {
            break;
        }
        scale -= 1;
    }
    exact_coefficient(a.checked_mul(b)?, scale)
}

pub(crate) fn exact_div_100(value: Decimal) -> Option<Decimal> {
    exact_coefficient(value.mantissa(), value.scale() + 2)
}

pub(crate) fn exact_add(left: Decimal, right: Decimal) -> Option<Decimal> {
    let left = left.normalize();
    let right = right.normalize();
    let scale = left.scale().max(right.scale());
    let a = left
        .mantissa()
        .checked_mul(10i128.pow(scale - left.scale()))?;
    let b = right
        .mantissa()
        .checked_mul(10i128.pow(scale - right.scale()))?;
    exact_coefficient(a.checked_add(b)?, scale)
}

fn exact_coefficient(mut coefficient: i128, mut scale: u32) -> Option<Decimal> {
    while scale > 0 && coefficient % 10 == 0 {
        coefficient /= 10;
        scale -= 1;
    }
    if scale > Decimal::MAX_SCALE {
        return None;
    }
    Decimal::try_from_i128_with_scale(coefficient, scale).ok()
}

/// `None` denotes a nonnumeric token; a numeric value outside Decimal's domain
/// is an error, never a special VAT code or an implicitly rounded amount.
pub(crate) fn numeric(value: &str) -> Option<Result<Decimal, rust_decimal::Error>> {
    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
    let (mantissa, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
    let exponent_digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
    if exponent_digits.is_empty() || !exponent_digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut digits = String::with_capacity(mantissa.len());
    let mut point = None;
    for byte in mantissa.bytes() {
        match byte {
            b'0'..=b'9' => digits.push(char::from(byte)),
            b'.' if point.is_none() => point = Some(digits.len()),
            _ => return None,
        }
    }
    if digits.is_empty() {
        return None;
    }
    Some((|| {
        let significant = digits.trim_start_matches('0');
        if significant.is_empty() {
            let mut zero = Decimal::ZERO;
            if let Ok(exponent) = exponent.parse::<i64>()
                && let Ok(fractional) = i64::try_from(point.map_or(0, |p| digits.len() - p))
                && let Some(scale) = fractional.checked_sub(exponent)
                && let Ok(scale) = u32::try_from(scale)
            {
                zero.rescale(scale.min(Decimal::MAX_SCALE));
            }
            return Ok(zero);
        }
        let coefficient = significant.trim_end_matches('0');
        let trailing = significant.len() - coefficient.len();
        let fractional = point.map_or(0, |p| digits.len() - p);
        let exponent = exponent
            .parse::<i64>()
            .map_err(|_| rust_decimal::Error::Underflow)?;
        let scale = i64::try_from(fractional)
            .ok()
            .and_then(|n| n.checked_sub(i64::try_from(trailing).ok()?))
            .and_then(|n| n.checked_sub(exponent))
            .ok_or(rust_decimal::Error::Underflow)?;
        if scale > i64::from(Decimal::MAX_SCALE) {
            return Err(rust_decimal::Error::Underflow);
        }
        // At most 29 significant integer digits fit. Check before expanding
        // an exponent, so hostile exponents cannot allocate enormous strings.
        let extra = if scale < 0 { scale.unsigned_abs() } else { 0 };
        if coefficient.len() > 29 || extra > 29 - coefficient.len() as u64 {
            return Err(rust_decimal::Error::ExceedsMaximumPossibleValue);
        }
        let mut normalized = coefficient.to_owned();
        for _ in 0..extra {
            normalized.push('0');
        }
        let mut integer = normalized
            .parse::<i128>()
            .map_err(|_| rust_decimal::Error::ExceedsMaximumPossibleValue)?;
        if value.starts_with('-') {
            integer = -integer;
        }
        let scale = u32::try_from(scale.max(0)).map_err(|_| rust_decimal::Error::Underflow)?;
        let mut result = Decimal::try_from_i128_with_scale(integer, scale)?;
        // Preserve the previous display/serde scale where it fits. Rescaling
        // upward can stop at the mantissa limit but never discards digits.
        if let Ok(fractional) = i64::try_from(fractional)
            && let Some(source_scale) = fractional.checked_sub(exponent)
            && let Ok(source_scale) = u32::try_from(source_scale)
        {
            result.rescale(source_scale.min(Decimal::MAX_SCALE));
        }
        Ok(result)
    })())
}

/// Parses a finite decimal, including exponent notation, without rounding.
/// Redundant leading/trailing zeroes are accepted; whitespace and grouping are not.
///
/// # Errors
/// Returns an error for malformed text or a value not exactly representable by
/// [`Decimal`]. Exponents never cause unbounded expansion.
pub fn parse(value: &str) -> Result<Decimal, rust_decimal::Error> {
    numeric(value).unwrap_or_else(|| Err("expected a finite decimal number".into()))
}
