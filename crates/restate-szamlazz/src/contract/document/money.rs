//! Monetary assertions and exact, bounded integer arithmetic for their validation.
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use szamlazz_agent::{ArithmeticError, Currency, LineItem, VatRate};

use super::{DocumentInput, LineItemInput};
use crate::contract::decimal::exact_add;

/// Authoritative net, VAT and gross amounts, either for a line or the document.
///
/// Explicit lines support HUF/Ft (whole amounts) and EUR (cents). Net must be
/// rounded net unit price × quantity. VAT must match either rounded net × rate
/// / 100 or rounded gross × rate / (100 + rate); gross must equal net + VAT.
/// Rounding is half away from zero. Numeric rates are restricted to 0..=100;
/// AAM, TAM, TAHK, ATK, EUT, EUKT, F.AFA, HO, EUE, EUFADE, EUFAD37, NAM,
/// EAM, KBAUK and KBAET are accepted with zero VAT. Other codes are refused
/// for explicit lines, without changing the open-set legacy calculation.
/// This validates monetary consistency, not tax eligibility or vendor acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Amounts {
    /// Net amount.
    #[serde(
        deserialize_with = "crate::contract::decimal::required",
        serialize_with = "rust_decimal::serde::str::serialize"
    )]
    #[cfg_attr(
        feature = "schemars",
        schemars(schema_with = "crate::contract::decimal::schema")
    )]
    pub net: Decimal,
    /// VAT amount.
    #[serde(
        deserialize_with = "crate::contract::decimal::required",
        serialize_with = "rust_decimal::serde::str::serialize"
    )]
    #[cfg_attr(
        feature = "schemars",
        schemars(schema_with = "crate::contract::decimal::schema")
    )]
    pub vat: Decimal,
    /// Gross amount.
    #[serde(
        deserialize_with = "crate::contract::decimal::required",
        serialize_with = "rust_decimal::serde::str::serialize"
    )]
    #[cfg_attr(
        feature = "schemars",
        schemars(schema_with = "crate::contract::decimal::schema")
    )]
    pub gross: Decimal,
}

/// Validated monetary content: the exact lines used by request construction and
/// their totals. Pure local preflight; no credentials, I/O or send permission.
#[derive(Debug, Clone, PartialEq)]
pub struct MonetaryPreflight {
    /// The lines submitted to Számla Agent.
    pub items: Vec<LineItem>,
    /// Exact sums of the line amounts.
    pub totals: Amounts,
}

/// A monetary refusal, before any vendor mutation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MonetaryError {
    /// Existing net-based calculation could not represent an intermediate.
    #[error(transparent)]
    Arithmetic(#[from] ArithmeticError),
    /// Exact integer arithmetic exceeded its bounded representation.
    #[error("monetary arithmetic is not exactly representable")]
    Unrepresentable,
    /// Explicit amounts are intentionally limited to evidenced currency precision.
    #[error("explicit amounts support HUF/Ft and EUR only")]
    UnsupportedCurrency,
    /// A rate is outside the explicit convention.
    #[error("explicit amounts require a percentage in 0..=100 or a supported zero-VAT code")]
    UnsupportedVatRate,
    /// A document has no lines to submit.
    #[error("at least one line item is required")]
    NoItems,
    /// Approved totals do not match the prepared document.
    #[error("expected_totals differ from submitted line totals")]
    TotalsMismatch,
    /// Explicit amounts carry fractional minor units.
    #[error("explicit amounts must already be rounded to the currency minor unit")]
    AmountPrecision,
    /// Explicit lines need a nonzero quantity.
    #[error("explicit amounts require nonzero quantity")]
    ZeroQuantity,
    /// The asserted gross differs from net plus VAT.
    #[error("amounts.gross must equal amounts.net + amounts.vat")]
    GrossMismatch,
    /// The asserted net differs from rounded net unit price times quantity.
    #[error("amounts.net must equal rounded unit_price * quantity")]
    NetMismatch,
    /// The asserted VAT follows neither supported rounding convention.
    #[error("amounts.vat must follow net-first or gross-first rounding")]
    VatMismatch,
    /// Position of a refused line in the public request.
    #[error("items[{index}]: {source}")]
    Item {
        /// Zero-based line position.
        index: usize,
        /// The failed monetary rule.
        #[source]
        source: Box<Self>,
    },
}

impl DocumentInput {
    /// Validate and prepare the exact monetary content for the effective currency.
    ///
    /// Use the same account-default/override currency as `Account::build_create`.
    /// That constructor calls this method too, including `expected_totals` checks.
    /// A caller may compare `totals` locally and attach the approved totals to its
    /// request so a different resolved currency cannot silently change the money.
    ///
    /// # Errors
    /// Refuses empty documents, inconsistent assertions and unrepresentable sums.
    pub fn monetary_preflight(
        &self,
        currency: &Currency,
    ) -> Result<MonetaryPreflight, MonetaryError> {
        if self.items.is_empty() {
            return Err(MonetaryError::NoItems);
        }
        let items = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                item.to_line_item(currency)
                    .map_err(|source| MonetaryError::Item {
                        index,
                        source: Box::new(source),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut totals = Amounts {
            net: Decimal::ZERO,
            vat: Decimal::ZERO,
            gross: Decimal::ZERO,
        };
        for item in &items {
            totals.net =
                exact_add(totals.net, item.net_value).ok_or(MonetaryError::Unrepresentable)?;
            totals.vat =
                exact_add(totals.vat, item.vat_value).ok_or(MonetaryError::Unrepresentable)?;
            totals.gross =
                exact_add(totals.gross, item.gross_value).ok_or(MonetaryError::Unrepresentable)?;
        }
        if self
            .expected_totals
            .as_ref()
            .is_some_and(|expected| expected != &totals)
        {
            return Err(MonetaryError::TotalsMismatch);
        }
        Ok(MonetaryPreflight { items, totals })
    }
}

pub(super) fn explicit_item(
    input: &LineItemInput,
    amounts: &Amounts,
    currency: &Currency,
) -> Result<LineItem, MonetaryError> {
    let scale = if currency.is_huf() {
        0
    } else if currency == &Currency::EUR {
        2
    } else {
        return Err(MonetaryError::UnsupportedCurrency);
    };
    let [net, vat, gross] =
        [amounts.net, amounts.vat, amounts.gross].map(|value| units(value, scale));
    let (net, vat, gross) = (net?, vat?, gross?);
    if net.checked_add(vat).ok_or(MonetaryError::Unrepresentable)? != gross {
        return Err(MonetaryError::GrossMismatch);
    }
    if input.quantity.is_zero() {
        return Err(MonetaryError::ZeroQuantity);
    }
    let price = input.unit_price.normalize();
    let quantity = input.quantity.normalize();
    let product = price
        .mantissa()
        .checked_mul(quantity.mantissa())
        .ok_or(MonetaryError::Unrepresentable)?;
    let net_units = rescale_round(product, price.scale() + quantity.scale(), scale)?;
    if net_units != net {
        return Err(MonetaryError::NetMismatch);
    }
    let rate = match input.vat_rate() {
        VatRate::Percent(rate) if rate >= Decimal::ZERO && rate <= Decimal::ONE_HUNDRED => {
            rate.normalize()
        }
        VatRate::Aam
        | VatRate::Tam
        | VatRate::Tahk
        | VatRate::Atk
        | VatRate::Eut
        | VatRate::Eukt
        | VatRate::FAfa
        | VatRate::Ho
        | VatRate::Eue
        | VatRate::Eufade
        | VatRate::Eufad37
        | VatRate::Nam
        | VatRate::Eam
        | VatRate::Kbauk
        | VatRate::Kbaet => Decimal::ZERO,
        _ => return Err(MonetaryError::UnsupportedVatRate),
    };
    let hundred = pow10(rate.scale())?
        .checked_mul(100)
        .ok_or(MonetaryError::Unrepresentable)?;
    let percent = rate.mantissa();
    let net_vat = rounded_ratio(
        net.checked_mul(percent)
            .ok_or(MonetaryError::Unrepresentable)?,
        hundred,
    )?;
    let gross_vat = rounded_ratio(
        gross
            .checked_mul(percent)
            .ok_or(MonetaryError::Unrepresentable)?,
        hundred
            .checked_add(percent)
            .ok_or(MonetaryError::Unrepresentable)?,
    )?;
    if vat != net_vat && vat != gross_vat {
        return Err(MonetaryError::VatMismatch);
    }
    Ok(LineItem {
        id: input.id.clone(),
        comment: input.comment.clone(),
        ..LineItem::new(
            &input.name,
            input.quantity,
            &input.unit,
            input.unit_price,
            input.vat_rate(),
            amounts.net,
            amounts.vat,
            amounts.gross,
        )
    })
}

fn pow10(scale: u32) -> Result<i128, MonetaryError> {
    10i128
        .checked_pow(scale)
        .ok_or(MonetaryError::Unrepresentable)
}

fn units(value: Decimal, scale: u32) -> Result<i128, MonetaryError> {
    let value = value.normalize();
    if value.scale() > scale {
        return Err(MonetaryError::AmountPrecision);
    }
    value
        .mantissa()
        .checked_mul(pow10(scale - value.scale())?)
        .ok_or(MonetaryError::Unrepresentable)
}

fn rescale_round(coefficient: i128, from: u32, to: u32) -> Result<i128, MonetaryError> {
    if from > to {
        rounded_ratio(coefficient, pow10(from - to)?)
    } else {
        coefficient
            .checked_mul(pow10(to - from)?)
            .ok_or(MonetaryError::Unrepresentable)
    }
}

// One rounding of an exact rational, avoiding rounded Decimal division followed
// by a second rounding (which can move a just-below-midpoint value over the tie).
fn rounded_ratio(numerator: i128, denominator: i128) -> Result<i128, MonetaryError> {
    let quotient = numerator / denominator;
    let remainder = (numerator % denominator).unsigned_abs();
    if remainder >= denominator.unsigned_abs().div_ceil(2) {
        quotient
            .checked_add(numerator.signum())
            .ok_or(MonetaryError::Unrepresentable)
    } else {
        Ok(quotient)
    }
}
