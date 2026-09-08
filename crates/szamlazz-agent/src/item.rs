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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rounding {
    /// Round to a fixed number of decimal places.
    Scale(u32),
    /// No rounding: exact decimal arithmetic, which can carry more decimal
    /// places than the currency has (`100.005 EUR` with a five-decimal VAT).
    /// szamlazz.hu then rounds **each value to two decimals on its own** and
    /// does not recompute the gross, so `100.004 / 27.00108 / 127.00508` is
    /// stored as `100 / 27 / 127.01`: a document whose gross is not net + VAT
    /// (observed on the test account, 2026-09-06). Ask for this only when your
    /// business rule requires it and you accept that outcome.
    Exact,
}

impl Rounding {
    /// Round to the currency's minor unit: whole forints for HUF, cents for
    /// EUR, thousandths for KWD; see [`Currency::minor_unit_digits`].
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
/// Receipt creation supports only `revenue_account` and `vat_account`; the
/// economic-event and settlement fields are invoice-only protocol fields.
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
/// with an explicit [`Rounding`] and no risk of a panic, or [`LineItem::new`]
/// when your system already computed them and must match.
#[doc(alias = "tétel")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LineItem {
    /// Item name (`megnevezes`).
    pub name: String,
    /// Account-side item identifier (`azonosito`).
    pub id: Option<String>,
    /// Quantity (`mennyiseg`).
    pub quantity: Decimal,
    /// Unit of measure (`mennyisegiEgyseg`), e.g. `db`.
    pub unit: String,
    /// Net unit price (`nettoEgysegar`).
    pub unit_price: Decimal,
    /// VAT rate (`afakulcs`).
    pub vat_rate: VatRate,
    /// Invoice-only margin-scheme VAT base (`arresAfaAlap`).
    pub margin_vat_base: Option<Decimal>,
    /// Net value (`nettoErtek`); must equal unit price × quantity.
    pub net_value: Decimal,
    /// VAT value (`afaErtek`); must equal net × rate / 100.
    pub vat_value: Decimal,
    /// Gross value (`bruttoErtek`); must equal net + VAT.
    pub gross_value: Decimal,
    /// Free-text comment for the row (`megjegyzes`).
    pub comment: Option<String>,
    /// General-ledger metadata (`tetelFokonyv` / `fokonyv`).
    pub ledger: Option<LineItemLedger>,
    /// Number of data erasure codes to request for this row (`torloKod`), at
    /// most [`MAX_ERASURE_CODE_COUNT`]. Despite the wire name, the value is a
    /// requested quantity, not a code identifier: szamlazz.hu generates this
    /// many codes for the item. Requires the account feature; on invoices the
    /// `SzlaMost` template (errors 537–539 otherwise).
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

    /// A line item with two-decimal net, VAT, and gross values computed from
    /// quantity, unit price, and rate.
    ///
    /// The infallible form of [`LineItem::try_calculated`] with
    /// [`Rounding::Scale`]`(2)`: suitable for currencies with two decimal
    /// places. For HUF and currency-aware code use `try_calculated` with
    /// [`Rounding::minor_unit`]. Non-percentage VAT codes (AAM, EUT, …) yield
    /// a VAT value of 0.
    ///
    /// # Panics
    ///
    /// When a derived value overflows a [`Decimal`] (see
    /// [`ArithmeticError`]). Use [`LineItem::try_calculated`] on values you do
    /// not control.
    pub fn calculated(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
    ) -> Self {
        Self::calculated_or_panic(
            name,
            quantity,
            unit,
            unit_price,
            vat_rate,
            Rounding::Scale(2),
        )
    }

    /// Computes net, VAT, and gross using protocol currency rules: whole
    /// forints for HUF (`HUF`/`Ft`), **exact, unrounded** decimal arithmetic
    /// for every other currency, so a `100.005 EUR` net goes on the wire with
    /// sub-cent VAT and gross values, which szamlazz.hu rounds to two decimals
    /// each on its own, possibly to a gross that is not net + VAT (see
    /// [`Rounding::Exact`]).
    ///
    /// Kept for compatibility. Prefer [`LineItem::try_calculated`] with
    /// [`Rounding::minor_unit`], which rounds every currency to its minor unit
    /// (the same whole forints for HUF) and returns an error instead of
    /// panicking; the exact arithmetic is [`Rounding::Exact`] there, chosen
    /// explicitly. Use [`LineItem::new`] when your system already computed the
    /// values and they must match.
    ///
    /// # Panics
    ///
    /// When a derived value overflows a [`Decimal`] (see
    /// [`ArithmeticError`]).
    pub fn calculated_for_currency(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
        currency: &Currency,
    ) -> Self {
        let rounding = if currency.is_huf() {
            Rounding::Scale(0)
        } else {
            Rounding::Exact
        };
        Self::calculated_or_panic(name, quantity, unit, unit_price, vat_rate, rounding)
    }

    /// The infallible forms' shared body: [`LineItem::try_calculated`], with
    /// the [`ArithmeticError`] as the panic message.
    fn calculated_or_panic(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
        rounding: Rounding,
    ) -> Self {
        Self::try_calculated(name, quantity, unit, unit_price, vat_rate, rounding)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// A line item with net, VAT, and gross computed from quantity, unit
    /// price, and rate, rounded as `rounding` says, or an error when a value
    /// does not fit a [`Decimal`].
    ///
    /// The fallible form of [`LineItem::calculated`] (`Rounding::Scale(2)`)
    /// and [`LineItem::calculated_for_currency`] (`Rounding::minor_unit` for
    /// HUF; `Rounding::Exact` for anything else). Non-percentage VAT codes
    /// (AAM, EUT, …) yield a VAT value of 0.
    ///
    /// # Errors
    ///
    /// [`ArithmeticError`] names the step whose result overflows the 96-bit
    /// mantissa of a [`Decimal`]: the net, the VAT, or the gross.
    pub fn try_calculated(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
        rounding: Rounding,
    ) -> Result<Self, ArithmeticError> {
        let net_value = unit_price
            .checked_mul(quantity)
            .map(|net| rounding.apply(net))
            .ok_or(ArithmeticError::NetOverflow)?;
        let vat_value = match &vat_rate {
            VatRate::Percent(rate) => net_value
                .checked_mul(*rate)
                .and_then(|scaled| scaled.checked_div(Decimal::ONE_HUNDRED))
                .map(|vat| rounding.apply(vat))
                .ok_or(ArithmeticError::VatOverflow)?,
            _ => Decimal::ZERO,
        };
        let gross_value = net_value
            .checked_add(vat_value)
            .ok_or(ArithmeticError::GrossOverflow)?;

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

    /// Sets the row comment (`megjegyzes`).
    #[must_use]
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn calculated_matches_docs_example() {
        // The docs' example invoice: 2 × 10000 at 27%.
        let item = LineItem::calculated(
            "Elado izé 2",
            dec!(2),
            "db",
            dec!(10000),
            VatRate::percent(27),
        );
        assert_eq!(item.net_value, dec!(20000));
        assert_eq!(item.vat_value, dec!(5400));
        assert_eq!(item.gross_value, dec!(25400));
    }

    #[test]
    fn calculated_rounds_half_up() {
        // 3 × 33.335 = 100.005 → 100.01 (half-up), VAT 27% = 27.0027 → 27.00
        let item = LineItem::calculated("x", dec!(3), "db", dec!(33.335), VatRate::percent(27));
        assert_eq!(item.net_value, dec!(100.01));
        assert_eq!(item.vat_value, dec!(27.00));
        assert_eq!(item.gross_value, dec!(127.01));
    }

    #[test]
    fn huf_calculation_rounds_monetary_totals_to_whole_forints() {
        let item = LineItem::calculated_for_currency(
            "x",
            dec!(3),
            "db",
            dec!(33.335),
            VatRate::percent(27),
            &crate::Currency::HUF,
        );
        assert_eq!(item.net_value, dec!(100));
        assert_eq!(item.vat_value, dec!(27));
        assert_eq!(item.gross_value, dec!(127));
    }

    #[test]
    fn foreign_currency_calculation_preserves_exact_decimal_values() {
        let item = LineItem::calculated_for_currency(
            "x",
            dec!(3),
            "db",
            dec!(33.335),
            VatRate::percent(27),
            &crate::Currency::EUR,
        );
        assert_eq!(item.net_value, dec!(100.005));
        assert_eq!(item.vat_value, dec!(27.00135));
        assert_eq!(item.gross_value, dec!(127.00635));
    }

    #[test]
    fn foreign_currency_calculation_does_not_discard_sub_cent_values() {
        let item = LineItem::calculated_for_currency(
            "x",
            dec!(1),
            "db",
            dec!(0.001),
            VatRate::percent(5),
            &crate::Currency::new("KWD"),
        );
        assert_eq!(item.net_value, dec!(0.001));
        assert_eq!(item.vat_value, dec!(0.00005));
        assert_eq!(item.gross_value, dec!(0.00105));
    }

    #[test]
    fn special_codes_have_zero_vat() {
        let item = LineItem::calculated("x", dec!(1), "db", dec!(100), VatRate::Aam);
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

        // Whole forints: the same values `calculated_for_currency` sends.
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
    #[should_panic(expected = "line item net value (unit price × quantity) overflows a decimal")]
    fn calculated_for_currency_panics_on_overflow() {
        let _ = LineItem::calculated_for_currency(
            "x",
            dec!(10),
            "db",
            Decimal::MAX,
            VatRate::percent(27),
            &Currency::EUR,
        );
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
