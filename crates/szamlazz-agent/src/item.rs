//! Line items (`tétel`).

use jiff::civil::Date;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::error::ArithmeticError;
use crate::types::{Currency, VatRate};

/// How [`LineItem::try_calculated`] rounds the net and VAT values it derives.
///
/// Rounding is half away from zero (`2.5 → 3`, `-2.5 → -3`) and is applied
/// at each step: the net is rounded before the VAT is computed from it, so
/// gross = net + VAT holds exactly on the wire. A scale above the value's own
/// leaves it unchanged, so `Scale(n)` never adds precision.
/// This is a local arithmetic invariant, not a guarantee of server acceptance
/// or storage precision. See [`CreateReceipt`](crate::ops::receipt::CreateReceipt)
/// for the distinct HUF/Ft receipt amount rules; `Scale(2)` alone does not
/// ensure a whole gross value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rounding {
    /// Round to a fixed number of decimal places.
    Scale(u32),
    /// No rounding: exact decimal arithmetic, which can carry more decimal
    /// places than the currency has (`100.005 EUR` with a five-decimal VAT).
    /// A EUR invoice sent as `100.004 / 27.00108 / 127.00508` was stored as
    /// `100 / 27 / 127.01`, rounding each value independently (test account,
    /// 2026-09-06). This is invoice evidence, not receipt evidence or a rule
    /// for KWD. The caller chooses values appropriate to the document.
    Exact,
}

impl Rounding {
    /// Round to the currency's minor unit: whole forints for HUF, cents for
    /// EUR, thousandths for KWD; see [`Currency::minor_unit_digits`].
    /// This local policy does not establish server precision for each currency.
    #[must_use]
    pub fn minor_unit(currency: &Currency) -> Self {
        Self::Scale(currency.minor_unit_digits())
    }

    fn apply(self, value: Decimal) -> Decimal {
        match self {
            Self::Scale(places) => {
                value.round_dp_with_strategy(places, RoundingStrategy::MidpointAwayFromZero)
            }
            Self::Exact => value,
        }
    }
}

/// General-ledger metadata attached to an invoice or receipt line item.
///
/// A receipt row carries only `revenue_account` and `vat_account`; the
/// economic-event and settlement fields are invoice-only, and a receipt
/// request that sets one is refused
/// ([`RequestError::UnsupportedOnReceipt`](crate::RequestError::UnsupportedOnReceipt)).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct LineItemLedger {
    /// Economic-event code (`gazdasagiEsem`).
    pub economic_event: Option<String>,
    /// VAT economic-event code (`gazdasagiEsemAfa`).
    pub vat_economic_event: Option<String>,
    /// Revenue general-ledger account (`arbevetelFokonyviSzam` / `arbevetel`).
    pub revenue_account: Option<String>,
    /// VAT general-ledger account (`afaFokonyviSzam` / `afa`).
    pub vat_account: Option<String>,
    /// Settlement period start (`elszDatumTol`).
    pub settlement_from: Option<Date>,
    /// Settlement period end (`elszDatumIg`).
    pub settlement_to: Option<Date>,
}

/// One row of a document (`tétel`).
///
/// szamlazz.hu verifies the arithmetic server-side: net = unit price ×
/// quantity, VAT = net × rate / 100, gross = net + VAT (error codes 259–264).
/// This crate does **not** duplicate that validation: the server is the
/// authority. Use [`LineItem::try_calculated`] to have the values computed
/// with an explicit [`Rounding`], or [`LineItem::new`] when your system
/// already computed them and must match. Plain data like every request
/// type: the optional fields are set with functional update
/// (`LineItem { comment: Some(..), ..item }`).
/// HUF/Ft receipt net and VAT may be fractional with at most two decimals while
/// gross must be whole and exactly their sum; see
/// [`CreateReceipt`](crate::ops::receipt::CreateReceipt). The explicit constructor
/// preserves such values (`787.40 / 212.60 / 1000`) as supplied.
#[doc(alias = "tétel")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LineItem {
    /// Item name (`megnevezes`).
    pub name: String,
    /// Account-side item identifier (`azonosito`).
    pub id: Option<String>,
    /// Quantity (`mennyiseg`).
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub quantity: Decimal,
    /// Unit of measure (`mennyisegiEgyseg`), e.g. `db`.
    pub unit: String,
    /// Net unit price (`nettoEgysegar`).
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub unit_price: Decimal,
    /// VAT rate (`afakulcs`).
    pub vat_rate: VatRate,
    /// Invoice-only margin-scheme VAT base (`arresAfaAlap`).
    #[serde(default, deserialize_with = "crate::number::de::optional")]
    #[serde(serialize_with = "rust_decimal::serde::str_option::serialize")]
    pub margin_vat_base: Option<Decimal>,
    /// Net value (`nettoErtek`): unit price × quantity, subject to rounding.
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub net_value: Decimal,
    /// VAT value (`afaErtek`): net × rate / 100, subject to rounding.
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub vat_value: Decimal,
    /// Gross value (`bruttoErtek`); must equal net + VAT.
    #[serde(deserialize_with = "crate::number::de::required")]
    #[serde(serialize_with = "rust_decimal::serde::str::serialize")]
    pub gross_value: Decimal,
    /// Free-text comment for the row (`megjegyzes`).
    pub comment: Option<String>,
    /// General-ledger metadata (`tetelFokonyv` / `fokonyv`).
    pub ledger: Option<LineItemLedger>,
    /// Number of data erasure codes requested for this row (`torloKod`), at
    /// most [`MAX_ERASURE_CODE_COUNT`]. This is a count, not a code identifier.
    ///
    /// The account must enable the feature (otherwise code 539); demo and
    /// test accounts cannot use it (538). Code 537 denotes exceeding the
    /// per-item count limit. For invoices, the vendor separately requires the
    /// `SzlaMost` template. Codes may come from the account's uploaded stock
    /// or be supplied by szamlazz.hu.
    /// See the [vendor erasure-code guidance](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor).
    #[doc(alias = "adattörlő kód")]
    #[serde(alias = "erasure_code")]
    pub erasure_code_count: Option<u32>,
}

/// szamlazz.hu's documented maximum number of data erasure codes per line
/// item.
pub const MAX_ERASURE_CODE_COUNT: u32 = 400;

impl LineItem {
    /// A line item with explicitly asserted values, passed to the wire as-is.
    #[expect(clippy::too_many_arguments, reason = "mirrors the wire row")]
    pub fn new(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
        net_value: Decimal,
        vat_value: Decimal,
        gross_value: Decimal,
    ) -> Self {
        Self {
            name: name.into(),
            id: None,
            quantity,
            unit: unit.into(),
            unit_price,
            vat_rate,
            margin_vat_base: None,
            net_value,
            vat_value,
            gross_value,
            comment: None,
            ledger: None,
            erasure_code_count: None,
        }
    }

    /// A line item with net, VAT, and gross computed from quantity, unit
    /// price, and rate, rounded as `rounding` says, or an error when a value
    /// does not fit a [`Decimal`].
    ///
    /// The one derived constructor: the rounding is the caller's explicit
    /// choice ([`Rounding::minor_unit`] for a document that must reconcile
    /// to a ledger), and an overflow of caller-supplied money is an error,
    /// never a panic. Non-percentage VAT codes (AAM, EUT, …) yield a VAT
    /// value of 0. Numeric tokens carried by `VatRate::Other` are interpreted
    /// as percentages while retaining their original wire text.
    /// A successful calculation proves local arithmetic only, not receipt
    /// amount compliance or the precision szamlazz.hu will store.
    ///
    /// # Errors
    ///
    /// [`ArithmeticError`] names the step whose intermediate or result cannot
    /// fit exactly in a [`Decimal`]: the net, the VAT, or the gross. This includes
    /// precision loss and underflow, even if later rounding would make it fit.
    /// A numeric
    /// VAT token outside Decimal's exact domain is `UnrepresentableVatRate`.
    pub fn try_calculated(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
        rounding: Rounding,
    ) -> Result<Self, ArithmeticError> {
        let net_value = crate::number::exact_mul(unit_price, quantity)
            .map(|net| rounding.apply(net))
            .ok_or(ArithmeticError::NetOverflow)?;
        let percentage = match &vat_rate {
            VatRate::Percent(rate) => Some(*rate),
            VatRate::Other(token) => {
                crate::number::numeric(token.trim_matches(crate::xml::is_xml_space))
                    .transpose()
                    .map_err(|_| ArithmeticError::UnrepresentableVatRate)?
            }
            _ => None,
        };
        let vat_value = match percentage {
            Some(rate) => crate::number::exact_mul(net_value, rate)
                .and_then(crate::number::exact_div_100)
                .map(|vat| rounding.apply(vat))
                .ok_or(ArithmeticError::VatOverflow)?,
            _ => Decimal::ZERO,
        };
        let gross_value =
            crate::number::exact_add(net_value, vat_value).ok_or(ArithmeticError::GrossOverflow)?;

        Ok(Self::new(
            name,
            quantity,
            unit,
            unit_price,
            vat_rate,
            net_value,
            vat_value,
            gross_value,
        ))
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn derived_amounts_refuse_precision_loss_before_rounding() {
        for rounding in [Rounding::Exact, Rounding::Scale(2)] {
            for (quantity, price) in [
                (dec!(0.9999999999999999999999999999), dec!(0.005)),
                (dec!(0.1), dec!(0.0000000000000000000000000001)),
            ] {
                assert_eq!(
                    LineItem::try_calculated("x", quantity, "db", price, VatRate::Aam, rounding),
                    Err(ArithmeticError::NetOverflow),
                );
            }
        }
        assert_eq!(
            LineItem::try_calculated(
                "x",
                dec!(1),
                "db",
                dec!(0.0000000000000000000000000001),
                VatRate::percent(27),
                Rounding::Exact,
            ),
            Err(ArithmeticError::VatOverflow),
        );
        assert_eq!(
            LineItem::try_calculated(
                "x",
                dec!(1),
                "db",
                dec!(10000000000000000000000000000),
                VatRate::Percent(dec!(0.0000000000000000000000000001)),
                Rounding::Exact,
            ),
            Err(ArithmeticError::GrossOverflow),
        );
    }

    #[test]
    fn exact_calculation_keeps_representable_boundary_products() {
        for (quantity, price, expected) in [
            (
                dec!(2.5),
                dec!(20000000000000000000000000000),
                dec!(50000000000000000000000000000),
            ),
            (
                dec!(-2.5),
                dec!(20000000000000000000000000000),
                dec!(-50000000000000000000000000000),
            ),
            (
                dec!(10000000000000000000000000000),
                dec!(0.0000000000000000000000000001),
                dec!(1),
            ),
            (Decimal::ZERO, Decimal::MAX, Decimal::ZERO),
        ] {
            let item =
                LineItem::try_calculated("x", quantity, "db", price, VatRate::Aam, Rounding::Exact)
                    .expect("exact product fits");
            assert_eq!(item.net_value, expected);
            assert_eq!(item.gross_value, expected);
        }
    }

    #[test]
    fn try_calculated_matches_docs_example() {
        // The docs' example invoice: 2 × 10000 at 27%.
        let item = LineItem::try_calculated(
            "Elado izé 2",
            dec!(2),
            "db",
            dec!(10000),
            VatRate::percent(27),
            Rounding::minor_unit(&Currency::HUF),
        )
        .expect("fits");
        assert_eq!(item.net_value, dec!(20000));
        assert_eq!(item.vat_value, dec!(5400));
        assert_eq!(item.gross_value, dec!(25400));
    }

    #[test]
    fn exact_rounding_does_not_discard_sub_unit_values() {
        let item = LineItem::try_calculated(
            "x",
            dec!(1),
            "db",
            dec!(0.001),
            VatRate::percent(5),
            Rounding::Exact,
        )
        .expect("fits");
        assert_eq!(item.net_value, dec!(0.001));
        assert_eq!(item.vat_value, dec!(0.00005));
        assert_eq!(item.gross_value, dec!(0.00105));
    }

    #[test]
    fn special_codes_have_zero_vat() {
        let item = LineItem::try_calculated(
            "x",
            dec!(1),
            "db",
            dec!(100),
            VatRate::Aam,
            Rounding::Scale(2),
        )
        .expect("fits");
        assert_eq!(item.vat_value, Decimal::ZERO);
        assert_eq!(item.gross_value, dec!(100));
    }

    #[test]
    fn try_calculated_reports_overflow_instead_of_panicking() {
        // Decimal::MAX × 10 does not fit a 96-bit mantissa.
        let net = LineItem::try_calculated(
            "x",
            dec!(10),
            "db",
            Decimal::MAX,
            VatRate::percent(27),
            Rounding::Exact,
        );
        assert_eq!(net, Err(ArithmeticError::NetOverflow));

        // The net fits; net × 27 does not.
        let vat = LineItem::try_calculated(
            "x",
            dec!(1),
            "db",
            Decimal::MAX,
            VatRate::percent(27),
            Rounding::Exact,
        );
        assert_eq!(vat, Err(ArithmeticError::VatOverflow));

        // 7.9e28 fits, so does its 1% VAT (7.9e26); their sum (7.979e28) does
        // not (Decimal::MAX ≈ 7.9228e28).
        let gross = LineItem::try_calculated(
            "x",
            dec!(1),
            "db",
            dec!(79000000000000000000000000000),
            VatRate::percent(1),
            Rounding::Exact,
        );
        assert_eq!(gross, Err(ArithmeticError::GrossOverflow));
    }

    #[test]
    fn try_calculated_rounds_to_the_minor_unit_at_each_step() {
        // 3 × 33.335 = 100.005; the net is rounded before the VAT is derived.
        let calculate = |rounding| {
            LineItem::try_calculated(
                "x",
                dec!(3),
                "db",
                dec!(33.335),
                VatRate::percent(27),
                rounding,
            )
            .expect("fits")
        };

        // Whole forints.
        let huf = calculate(Rounding::minor_unit(&Currency::HUF));
        assert_eq!(huf.net_value, dec!(100));
        assert_eq!(huf.vat_value, dec!(27));
        assert_eq!(huf.gross_value, dec!(127));

        // Cents: 100.005 → 100.01 (half away from zero); 27.0027 → 27.00.
        let eur = calculate(Rounding::minor_unit(&Currency::EUR));
        assert_eq!(eur.net_value, dec!(100.01));
        assert_eq!(eur.vat_value, dec!(27.00));
        assert_eq!(eur.gross_value, dec!(127.01));

        // A three-decimal currency keeps the thousandth.
        let kwd = calculate(Rounding::minor_unit(&Currency::new("KWD")));
        assert_eq!(kwd.net_value, dec!(100.005));
        assert_eq!(kwd.vat_value, dec!(27.001));
        assert_eq!(kwd.gross_value, dec!(127.006));

        let four = calculate(Rounding::Scale(4));
        assert_eq!(four.net_value, dec!(100.005));
        assert_eq!(four.vat_value, dec!(27.0014));
        assert_eq!(four.gross_value, dec!(127.0064));

        let exact = calculate(Rounding::Exact);
        assert_eq!(exact.net_value, dec!(100.005));
        assert_eq!(exact.vat_value, dec!(27.00135));
        assert_eq!(exact.gross_value, dec!(127.00635));
    }

    #[test]
    fn try_calculated_rounds_half_away_from_zero() {
        // A credit line: -2.5 → -3, not -2 (half away from zero, not half up).
        let item = LineItem::try_calculated(
            "x",
            dec!(1),
            "db",
            dec!(-2.5),
            VatRate::Aam,
            Rounding::Scale(0),
        )
        .expect("fits");
        assert_eq!(item.net_value, dec!(-3));
        assert_eq!(item.gross_value, dec!(-3));
    }
}
