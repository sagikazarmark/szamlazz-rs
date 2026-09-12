//! Iterative exact significand parsing, independent of input length on the stack.

use rust_decimal::{Decimal, Error};

pub(crate) fn exact(text: &str) -> Result<Decimal, Error> {
    let (negative, text) = if let Some(text) = text.strip_prefix('-') {
        (true, text)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let mut coefficient = 0i128;
    let mut scale = 0;
    let mut point = false;
    let mut digit_seen = false;
    for byte in text.bytes() {
        // from_str_exact refuses any suffix once all 28 fractional positions
        // have been consumed, including redundant zeroes or separators.
        if point && scale == Decimal::MAX_SCALE {
            return Err(Error::Underflow);
        }
        match byte {
            b'0'..=b'9' => {
                digit_seen = true;
                // The preceding coefficient fits 96 bits, so this fits i128.
                coefficient = coefficient * 10 + i128::from(byte - b'0');
                if coefficient > Decimal::MAX.mantissa() {
                    return Err(Error::ExceedsMaximumPossibleValue);
                }
                if point {
                    scale += 1;
                }
            }
            b'.' if !point => point = true,
            // Preserve the form parser's from_str_exact separator tolerance.
            // JSON checks its stricter grammar before calling this parser.
            b'_' if digit_seen => {}
            _ => return Err(Error::from("invalid decimal character")),
        }
    }
    if !digit_seen {
        return Err(Error::from("decimal has no digits"));
    }
    Decimal::try_from_i128_with_scale(if negative { -coefficient } else { coefficient }, scale)
}
