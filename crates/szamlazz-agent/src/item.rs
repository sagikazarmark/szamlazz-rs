//! Line items (`tétel`).

use jiff::civil::Date;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::types::{Currency, VatRate};

/// General-ledger metadata attached to an invoice or receipt line item.
///
/// Receipt creation supports only `revenue_account` and `vat_account`; the
/// economic-event and settlement fields are invoice-only protocol fields.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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
/// This crate does **not** duplicate that validation — the server is the
/// authority. Use [`LineItem::calculated`] to have the values computed, or
/// [`LineItem::new`] when your system already computed them and must match.
#[doc(alias = "tétel")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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
    /// This helper has explicit minor-unit semantics and is suitable for
    /// currencies with two decimal places. For HUF and currency-aware code,
    /// use [`LineItem::calculated_for_currency`]. Non-percentage VAT codes
    /// (AAM, EUT, …) yield a VAT value of 0.
    pub fn calculated(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
    ) -> Self {
        Self::calculated_with_precision(name, quantity, unit, unit_price, vat_rate, 2)
    }

    /// Computes net, VAT, and gross using protocol currency rules: whole
    /// forints for HUF (`HUF`/`Ft`), exact decimal arithmetic for other
    /// currencies. Use [`LineItem::new`] when a foreign-currency business rule
    /// requires an explicit rounding scale.
    pub fn calculated_for_currency(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
        currency: &Currency,
    ) -> Self {
        if currency.is_huf() {
            Self::calculated_with_precision(name, quantity, unit, unit_price, vat_rate, 0)
        } else {
            let net_value = unit_price * quantity;
            let vat_value = match &vat_rate {
                VatRate::Percent(rate) => net_value * *rate / Decimal::ONE_HUNDRED,
                _ => Decimal::ZERO,
            };
            let gross_value = net_value + vat_value;

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
    }

    fn calculated_with_precision(
        name: impl Into<String>,
        quantity: Decimal,
        unit: impl Into<String>,
        unit_price: Decimal,
        vat_rate: VatRate,
        precision: u32,
    ) -> Self {
        let round = |value: Decimal| {
            value.round_dp_with_strategy(precision, RoundingStrategy::MidpointAwayFromZero)
        };
        let net_value = round(unit_price * quantity);
        let vat_value = match &vat_rate {
            VatRate::Percent(rate) => round(net_value * *rate / Decimal::ONE_HUNDRED),
            _ => Decimal::ZERO,
        };
        let gross_value = net_value + vat_value;

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
}
