//! Receiver types for szamlazz.hu **IPN** (instant payment notification).
//!
//! When an invoice's or proforma's paid amount changes on the szamlazz.hu
//! side, an `application/x-www-form-urlencoded` POST is sent to the IPN URL
//! configured in the account settings. Its amounts are an absolute snapshot
//! of the document's current payment status, not a newly registered payment
//! or a delta. Deliveries may be retried or coalesced, so consumers should
//! replace or upsert the stored status rather than add [`PaymentNotification::paid_gross`].
//! Scope that upsert by the configured account or endpoint as well as the
//! document number: IPN does not carry a Szamlazz.hu account identifier.
//!
//! The payload does not reliably identify whether its document is an invoice
//! or a proforma. The receiver's whole contract is: parse the body, answer
//! HTTP 200 once the notification is durably accepted. Any other status makes
//! szamlazz.hu retry every 3 minutes, up to 10 times.
//!
//! IPN is **unauthenticated by design**. Practical mitigations: register an
//! IPN URL containing an unguessable path segment, and optionally check the
//! source address against [`SOURCE_IPS`].
//!
//! The core is framework-free and `wasm32`-clean: call
//! [`PaymentNotification::from_form_bytes`] on the raw body from any HTTP
//! stack (Cloudflare Workers included). With the `axum` feature,
//! [`PaymentNotification`] is an axum extractor.
//!
//! # Quick start
//!
//! Parse the raw form body and treat its amounts as the latest snapshot:
//!
//! ```
//! use szamlazz_ipn::PaymentNotification;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let body = b"szlahu_szamlaszam=E-2026-123&szlahu_bruttovegosszeg=12700&\
//!              szlahu_kifizetettbrutto=12700&szlahu_fizetesmod=bankkartya";
//! let notification = PaymentNotification::from_form_bytes(body)?;
//!
//! assert_eq!(notification.document_number, "E-2026-123");
//! assert!(notification.is_fully_paid());
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! Default features are empty and provide framework-free parsing on native
//! and `wasm32-unknown-unknown` targets.
//!
//! - `serde` adds `Serialize` and `Deserialize` implementations for
//!   [`PaymentNotification`], with no additional platform restrictions.
//! - `axum` adds a [`PaymentNotification`] request extractor and
//!   `IpnRejection`. It does not select an HTTP server or runtime; native and
//!   wasm applications provide the axum-compatible runtime themselves.
// docs.rs builds with all features on nightly and sets `--cfg docsrs`;
// current rustdoc's doc_cfg automatically annotates feature- and target gates.
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::net::{IpAddr, Ipv4Addr};

use jiff::civil::Date;
use rust_decimal::Decimal;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use axum::IpnRejection;

/// The szamlazz.hu addresses IPN calls originate from, as of 2025-08-01.
///
/// Parsed [`IpAddr`]s so they compare directly against a connection's peer
/// address. IP allowlists rot: szamlazz.hu can change these without notice.
/// Treat this list as a defense-in-depth signal, not as authentication.
pub const SOURCE_IPS: &[IpAddr] = &[
    IpAddr::V4(Ipv4Addr::new(3, 73, 214, 98)),
    IpAddr::V4(Ipv4Addr::new(3, 76, 149, 232)),
    IpAddr::V4(Ipv4Addr::new(18, 153, 156, 51)),
];

/// An absolute snapshot of an invoice's or proforma's current payment status.
///
/// This is not a newly registered payment or a delta. Deliveries may be
/// retried or coalesced, so use the configured account or endpoint context
/// together with [`document_number`](Self::document_number) to replace or
/// upsert payment status; never add [`paid_gross`](Self::paid_gross) to a
/// previously stored amount. Document numbers alone are not unique across
/// Szamlazz.hu accounts, and the payload contains no account identifier.
///
/// The payload has no reliable document-kind discriminator. Field names
/// follow the crate's English vocabulary; each documents the `szlahu_*` form
/// parameter it comes from.
#[doc(alias = "IPN")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct PaymentNotification {
    /// The invoice or proforma whose status this is (`szlahu_szamlaszam`).
    #[doc(alias = "számlaszám")]
    #[doc(alias = "invoice_number")]
    #[cfg_attr(feature = "serde", serde(alias = "invoice_number"))]
    pub document_number: String,
    /// Current gross total of the document (`szlahu_bruttovegosszeg`).
    pub gross_total: Decimal,
    /// Current gross amount paid so far (`szlahu_kifizetettbrutto`).
    ///
    /// This is an absolute amount, not the amount of a new payment or a delta.
    #[doc(alias = "kifizetett bruttó")]
    pub paid_gross: Decimal,
    /// Payment method (`szlahu_fizetesmod`), e.g. `átutalás`, `kp`,
    /// `bankkártya`.
    #[doc(alias = "fizetési mód")]
    pub payment_method: String,
    /// Payment date (`szlahu_kifizdat`); it is omitted unless szamlazz.hu
    /// customer service has enabled sending it for the account.
    pub payment_date: Option<Date>,
    /// The parent proforma's number (`szlahu_dijbekero_szama`), when the
    /// invoice was issued from a proforma. Its presence does not reliably
    /// identify the kind of the document this snapshot describes.
    #[doc(alias = "díjbekérő száma")]
    pub proforma_number: Option<String>,
    /// The document's order number (`szlahu_rendelesszam`), when present.
    #[doc(alias = "rendelésszám")]
    pub order_number: Option<String>,
}

impl PaymentNotification {
    /// A notification with the always-present fields; the optional fields
    /// (`payment_date`, `proforma_number`, `order_number`) default to absent
    /// and can be set on the returned value.
    ///
    /// Mainly for constructing test fixtures: real notifications arrive via
    /// [`PaymentNotification::from_form_bytes`].
    pub fn new(
        document_number: impl Into<String>,
        gross_total: Decimal,
        paid_gross: Decimal,
        payment_method: impl Into<String>,
    ) -> Self {
        Self {
            document_number: document_number.into(),
            gross_total,
            paid_gross,
            payment_method: payment_method.into(),
            payment_date: None,
            proforma_number: None,
            order_number: None,
        }
    }

    /// Whether the document is fully paid according to this snapshot.
    #[must_use]
    pub fn is_fully_paid(&self) -> bool {
        if self.gross_total.is_sign_negative() {
            self.paid_gross <= self.gross_total
        } else {
            self.paid_gross >= self.gross_total
        }
    }

    /// Parses a raw `application/x-www-form-urlencoded` request body.
    ///
    /// # Errors
    ///
    /// Returns an error if a required parameter is missing or an amount or
    /// payment date is invalid.
    pub fn from_form_bytes(body: &[u8]) -> Result<Self, IpnParseError> {
        Self::from_pairs(form_urlencoded::parse(body))
    }

    /// Builds a notification from already-decoded key/value pairs, for HTTP
    /// stacks that pre-parse form bodies.
    ///
    /// # Errors
    ///
    /// Returns an error if a required parameter is missing or an amount or
    /// payment date is invalid.
    pub fn from_pairs<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> Result<Self, IpnParseError>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut document_number = None;
        let mut gross_total = None;
        let mut paid_gross = None;
        let mut payment_method = None;
        let mut payment_date = None;
        let mut proforma_number = None;
        let mut order_number = None;

        for (key, value) in pairs {
            let value = value.as_ref();

            match key.as_ref() {
                "szlahu_szamlaszam" => document_number = non_empty(value),
                "szlahu_bruttovegosszeg" => {
                    gross_total = Some(parse_decimal("szlahu_bruttovegosszeg", value)?);
                }
                "szlahu_kifizetettbrutto" => {
                    paid_gross = Some(parse_decimal("szlahu_kifizetettbrutto", value)?);
                }
                "szlahu_fizetesmod" => payment_method = non_empty(value),
                "szlahu_kifizdat" => {
                    payment_date = match non_empty(value) {
                        Some(date) => Some(Date::strptime("%Y-%m-%d", &date).map_err(|err| {
                            IpnParseError::Invalid {
                                field: "szlahu_kifizdat",
                                message: err.to_string(),
                            }
                        })?),
                        None => None,
                    };
                }
                "szlahu_dijbekero_szama" => proforma_number = non_empty(value),
                "szlahu_rendelesszam" => order_number = non_empty(value),
                _ => {}
            }
        }

        Ok(Self {
            document_number: document_number.ok_or(IpnParseError::Missing("szlahu_szamlaszam"))?,
            gross_total: gross_total.ok_or(IpnParseError::Missing("szlahu_bruttovegosszeg"))?,
            paid_gross: paid_gross.ok_or(IpnParseError::Missing("szlahu_kifizetettbrutto"))?,
            payment_method: payment_method.ok_or(IpnParseError::Missing("szlahu_fizetesmod"))?,
            payment_date,
            proforma_number,
            order_number,
        })
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn parse_decimal(field: &'static str, value: &str) -> Result<Decimal, IpnParseError> {
    let value = value.trim();

    value
        .parse()
        .or_else(|error: rust_decimal::Error| {
            // The docs show only integer amounts and never specify a decimal
            // separator. Tolerate a lone comma so an unexpected "1234,56"
            // does not 400 — szamlazz.hu discards a notification after ten
            // failed deliveries.
            if value.matches(',').count() == 1 && !value.contains('.') {
                value.replace(',', ".").parse().map_err(|_| error)
            } else {
                Err(error)
            }
        })
        .map_err(|error| IpnParseError::Invalid {
            field,
            message: error.to_string(),
        })
}

/// A request body that is not a valid IPN message.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IpnParseError {
    /// A required parameter is absent.
    #[error("missing IPN parameter {0}")]
    Missing(&'static str),
    /// A parameter failed to parse into its typed representation.
    #[error("invalid IPN parameter {field}: {message}")]
    Invalid {
        /// The offending parameter.
        field: &'static str,
        /// What went wrong.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;

    use super::*;

    #[test]
    fn parses_full_notification() {
        let body = b"szlahu_szamlaszam=E-2026-123&szlahu_dijbekero_szama=DB-2026-456&\
                     szlahu_rendelesszam=RND1234&szlahu_bruttovegosszeg=10000&\
                     szlahu_kifizetettbrutto=5000&szlahu_fizetesmod=%C3%A1tutal%C3%A1s&\
                     szlahu_kifizdat=2026-07-04";
        let ipn = PaymentNotification::from_form_bytes(body).expect("parse");
        assert_eq!(ipn.document_number, "E-2026-123");
        assert_eq!(ipn.gross_total, dec!(10000));
        assert_eq!(ipn.paid_gross, dec!(5000));
        assert_eq!(ipn.payment_method, "átutalás");
        assert_eq!(ipn.payment_date, Some(date(2026, 7, 4)));
        assert_eq!(ipn.proforma_number.as_deref(), Some("DB-2026-456"));
        assert_eq!(ipn.order_number.as_deref(), Some("RND1234"));
        assert!(!ipn.is_fully_paid());
    }

    #[test]
    fn parses_minimal_notification() {
        let body = b"szlahu_szamlaszam=E-2026-123&szlahu_bruttovegosszeg=10000&\
                     szlahu_kifizetettbrutto=10000&szlahu_fizetesmod=kp";
        let ipn = PaymentNotification::from_form_bytes(body).expect("parse");
        assert_eq!(ipn.payment_date, None);
        assert_eq!(ipn.proforma_number, None);
        assert!(ipn.is_fully_paid());
    }

    #[test]
    fn parses_proforma_payment_status_snapshot() {
        let body = b"szlahu_szamlaszam=DB-2026-456&szlahu_bruttovegosszeg=10000&\
                     szlahu_kifizetettbrutto=3000&szlahu_fizetesmod=bankkartya";
        let ipn = PaymentNotification::from_form_bytes(body).expect("parse");
        assert_eq!(ipn.document_number, "DB-2026-456");
        assert_eq!(ipn.paid_gross, dec!(3000));
        assert_eq!(ipn.proforma_number, None);
        assert!(!ipn.is_fully_paid());
    }

    #[test]
    fn full_payment_comparison_follows_the_total_sign() {
        for (gross, paid, expected) in [
            (dec!(100), dec!(0), false),
            (dec!(100), dec!(100), true),
            (dec!(100), dec!(150), true),
            (dec!(-100), dec!(0), false),
            (dec!(-100), dec!(-50), false),
            (dec!(-100), dec!(-100), true),
            (dec!(-100), dec!(-150), true),
        ] {
            let notification = PaymentNotification::new("E-1", gross, paid, "kp");
            assert_eq!(notification.is_fully_paid(), expected, "{gross} / {paid}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn deserializes_legacy_invoice_number_field() {
        let notification = PaymentNotification::new("DB-2026-456", dec!(10000), dec!(3000), "kp");
        let mut value = serde_json::to_value(&notification).expect("serialize");
        let object = value.as_object_mut().expect("object");
        let document_number = object.remove("document_number").expect("document number");
        object.insert("invoice_number".to_owned(), document_number);

        let decoded: PaymentNotification = serde_json::from_value(value).expect("deserialize");
        assert_eq!(decoded, notification);
    }

    #[test]
    fn missing_required_parameter() {
        let body = b"szlahu_szamlaszam=E-2026-123";
        let error = PaymentNotification::from_form_bytes(body).expect_err("error");
        assert!(matches!(
            error,
            IpnParseError::Missing("szlahu_bruttovegosszeg")
        ));
    }

    #[test]
    fn tolerates_comma_decimal_amounts() {
        // The docs never specify the decimal separator; a comma amount must
        // not reject the delivery (szamlazz.hu discards the notification
        // after ten failed retries).
        let body = b"szlahu_szamlaszam=E-2026-123&szlahu_bruttovegosszeg=1000%2C50&\
                     szlahu_kifizetettbrutto=1000.50&szlahu_fizetesmod=kp";
        let ipn = PaymentNotification::from_form_bytes(body).expect("parse");
        assert_eq!(ipn.gross_total, dec!(1000.50));
        assert!(ipn.is_fully_paid());
    }

    #[test]
    fn invalid_amount() {
        let body = b"szlahu_szamlaszam=E&szlahu_bruttovegosszeg=abc&\
                     szlahu_kifizetettbrutto=1&szlahu_fizetesmod=kp";
        let error = PaymentNotification::from_form_bytes(body).expect_err("error");
        assert!(matches!(
            error,
            IpnParseError::Invalid {
                field: "szlahu_bruttovegosszeg",
                ..
            }
        ));
    }

    #[test]
    fn payment_date_requires_the_documented_date_only_format() {
        let body = b"szlahu_szamlaszam=E&szlahu_bruttovegosszeg=1&\
                     szlahu_kifizetettbrutto=1&szlahu_fizetesmod=kp&\
                     szlahu_kifizdat=2026-07-04T12%3A30%3A00%2B02%3A00";
        let error = PaymentNotification::from_form_bytes(body).expect_err("error");
        assert!(matches!(
            error,
            IpnParseError::Invalid {
                field: "szlahu_kifizdat",
                ..
            }
        ));
    }

    #[test]
    fn unknown_parameters_are_ignored() {
        let body = b"szlahu_szamlaszam=E&szlahu_bruttovegosszeg=1&\
                     szlahu_kifizetettbrutto=1&szlahu_fizetesmod=kp&future_param=x";
        PaymentNotification::from_form_bytes(body).expect("parse");
    }
}
