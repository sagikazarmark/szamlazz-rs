//! Receiver types for szamlazz.hu **IPN** (instant payment notification).
//!
//! When an invoice's or proforma's paid amount changes on the szamlazz.hu
//! side, an `application/x-www-form-urlencoded` POST is sent to the IPN URL
//! configured in the account settings. Its amounts are an absolute snapshot
//! of the document's current payment status, not a newly registered payment
//! or a delta. Retries can interleave and changes can be coalesced; there is
//! no sequence number or ordering timestamp. Treat each IPN as “query now”,
//! never add [`PaymentNotification::paid_gross`] to a stored amount or let
//! arrival order overwrite verified status. Scope queries and stored status
//! by the configured account or endpoint as well as the document number:
//! IPN does not carry a szamlazz.hu account identifier.
//!
//! The payload does not reliably identify whether its document is an invoice
//! or a proforma. The receiver's whole contract is: parse the body, answer
//! HTTP 200 once the notification is durably accepted. Any other status makes
//! szamlazz.hu retry every 3 minutes, up to 10 times.
//!
//! IPN is **unauthenticated by design**: confirm the document's payment status
//! through the **Számla Agent** on the configured account before financial
//! actions. An unguessable URL and [`SOURCE_IPS`] are defense in depth, not
//! authentication. Behind a proxy, trust forwarded addresses only from a
//! configured trusted proxy that strips client-supplied forwarding headers.
//!
//! Parsing refuses only malformed form encoding or missing document identity.
//! Missing, empty or unreadable content becomes `None`, with the decoded,
//! untrimmed text retained in the corresponding `raw_*` field.
//!
//! The core is framework-free and `wasm32`-clean: call
//! [`PaymentNotification::from_form_bytes`] on the raw body from any HTTP
//! stack (Cloudflare Workers included). With the `axum` feature,
//! [`PaymentNotification`] is an axum extractor.
//!
//! # Quick start
//!
//! Parse the raw form body as an unverified hint to query the document:
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
//! assert_eq!(notification.is_fully_paid(), Some(true));
//! // This comparison is not verification: query the Számla Agent before acting.
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
//!   [`PaymentNotification`], with exact JSON monetary input and no additional
//!   platform restrictions. Amounts serialize as decimal strings or null;
//!   see the README for the JSON input contract.
//! - `axum` adds a [`PaymentNotification`] request extractor and
//!   `IpnRejection`. It does not select an HTTP server or runtime; native and
//!   wasm applications provide the axum-compatible runtime themselves.
// docs.rs builds with all features on nightly and sets `--cfg docsrs`;
// current rustdoc's doc_cfg automatically annotates feature- and target gates.
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::net::{IpAddr, Ipv4Addr};

use jiff::civil::Date;
use rust_decimal::Decimal;

#[cfg(feature = "serde")]
mod decimal_serde;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use axum::IpnRejection;

/// The README's examples, compiled as doctests in every feature configuration.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

/// The szamlazz.hu addresses IPN calls originate from, as of 2025-08-01.
///
/// Parsed [`IpAddr`]s so they compare directly against a connection's peer
/// address. IP allowlists rot: szamlazz.hu can change these without notice.
/// Treat this list as a defense-in-depth signal, not as authentication.
/// Check the vendor's current list before deploying or updating an allowlist.
/// Behind a reverse proxy the peer is the proxy, not szamlazz.hu: enforce the
/// allowlist at the edge, or use forwarded addresses only from explicitly
/// trusted proxies that strip and replace client-supplied forwarding headers.
/// Never trust arbitrary `Forwarded` / `X-Forwarded-For` headers. An obsolete
/// allowlist can discard legitimate deliveries after retries are exhausted.
pub const SOURCE_IPS: &[IpAddr] = &[
    IpAddr::V4(Ipv4Addr::new(3, 73, 214, 98)),
    IpAddr::V4(Ipv4Addr::new(3, 76, 149, 232)),
    IpAddr::V4(Ipv4Addr::new(18, 153, 156, 51)),
];

/// An absolute snapshot of an invoice's or proforma's current payment status.
///
/// This is not a newly registered payment or a delta. Deliveries may be
/// retried, interleaved or coalesced, so treat it as a hint to query the
/// **Számla Agent**, not an ordered event. Confirm payment status there before
/// financial actions: the IPN body is unauthenticated. Use the configured
/// account or endpoint context together with [`document_number`](Self::document_number)
/// for that query and any stored status; never add [`paid_gross`](Self::paid_gross)
/// to a previously stored amount. Document numbers alone are not unique across
/// szamlazz.hu accounts, and the payload contains no account identifier.
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
    /// Reported gross total (`szlahu_bruttovegosszeg`), if exactly representable.
    /// Missing, empty, malformed or out-of-range amounts are `None`.
    #[cfg_attr(feature = "serde", serde(default, with = "decimal_serde"))]
    pub gross_total: Option<Decimal>,
    /// Decoded, untrimmed `szlahu_bruttovegosszeg`, including empty or unreadable text.
    /// `None` means the parameter was absent (or no wire text was supplied to [`Self::new`]).
    pub raw_gross_total: Option<String>,
    /// Current gross amount paid so far (`szlahu_kifizetettbrutto`).
    ///
    /// This is an absolute amount, not the amount of a new payment or a delta.
    /// Missing, empty, malformed or not exactly representable amounts are `None`.
    #[doc(alias = "kifizetett bruttó")]
    #[cfg_attr(feature = "serde", serde(default, with = "decimal_serde"))]
    pub paid_gross: Option<Decimal>,
    /// Decoded, untrimmed `szlahu_kifizetettbrutto`, including empty or unreadable text.
    /// `None` means the parameter was absent (or no wire text was supplied to [`Self::new`]).
    pub raw_paid_gross: Option<String>,
    /// Payment method (`szlahu_fizetesmod`), e.g. `átutalás`, `kp`,
    /// `bankkártya`.
    /// Missing or whitespace-only text is `None`; unknown methods remain strings.
    #[doc(alias = "fizetési mód")]
    pub payment_method: Option<String>,
    /// Decoded, untrimmed `szlahu_fizetesmod`, including empty text.
    /// `None` means the parameter was absent (or no wire text was supplied to [`Self::new`]).
    pub raw_payment_method: Option<String>,
    /// Payment date (`szlahu_kifizdat`); it is omitted unless szamlazz.hu
    /// customer service has enabled sending it for the account. Missing, empty
    /// or unreadable dates are `None`; only `%Y-%m-%d` is read as a civil date.
    pub payment_date: Option<Date>,
    /// Decoded, untrimmed `szlahu_kifizdat`, including empty or unreadable text.
    /// `None` means the parameter was absent (or no wire text was supplied to [`Self::new`]).
    pub raw_payment_date: Option<String>,
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
    /// A notification with known amounts and payment method. Other fields
    /// default to absent and can be set on the returned value. The `raw_*`
    /// fields are `None`: this constructor has no original wire text.
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
            gross_total: Some(gross_total),
            raw_gross_total: None,
            paid_gross: Some(paid_gross),
            raw_paid_gross: None,
            payment_method: Some(payment_method.into()),
            raw_payment_method: None,
            payment_date: None,
            raw_payment_date: None,
            proforma_number: None,
            order_number: None,
        }
    }

    /// Whether the reported amounts describe full payment, or `None` if
    /// either amount is unknown. Negative totals compare in the reverse direction.
    ///
    /// This is arithmetic on an **unauthenticated** body, not verified payment.
    /// Confirm payment status through the **Számla Agent** on the configured
    /// account before financial actions. Arrival order does not establish freshness.
    #[must_use]
    pub fn is_fully_paid(&self) -> Option<bool> {
        let gross_total = self.gross_total?;
        let paid_gross = self.paid_gross?;
        Some(if gross_total.is_sign_negative() {
            paid_gross <= gross_total
        } else {
            paid_gross >= gross_total
        })
    }

    /// Parses a raw `application/x-www-form-urlencoded` request body.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed percent escapes, non-UTF-8 decoded text,
    /// or an absent or whitespace-only document number. Content never causes
    /// refusal: see [`Self::from_pairs`] for its reading and raw-text retention.
    pub fn from_form_bytes(body: &[u8]) -> Result<Self, IpnParseError> {
        validate_form(body)?;
        Self::from_pairs(form_urlencoded::parse(body))
    }

    /// Builds a notification from already-decoded key/value pairs, for HTTP
    /// stacks that pre-parse form bodies.
    ///
    /// The caller is responsible for rejecting malformed form encoding before
    /// decoding. Missing, empty or unreadable dates and amounts become `None`;
    /// decimals are never rounded. A lone comma is accepted as a decimal separator.
    /// Missing or whitespace-only methods become `None`. The four `raw_*` fields
    /// retain decoded text before trimming, including empty strings. Unknown
    /// parameters are ignored; repeated parameters use the last value, including
    /// an empty or unreadable one.
    ///
    /// # Errors
    ///
    /// Returns an error if the document number is absent or whitespace-only.
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
        let mut raw_gross_total = None;
        let mut raw_paid_gross = None;
        let mut raw_payment_method = None;
        let mut raw_payment_date = None;
        let mut proforma_number = None;
        let mut order_number = None;

        for (key, value) in pairs {
            let value = value.as_ref();

            match key.as_ref() {
                "szlahu_szamlaszam" => document_number = non_empty(value),
                "szlahu_bruttovegosszeg" => {
                    gross_total = parse_decimal(value);
                    raw_gross_total = Some(value.to_owned());
                }
                "szlahu_kifizetettbrutto" => {
                    paid_gross = parse_decimal(value);
                    raw_paid_gross = Some(value.to_owned());
                }
                "szlahu_fizetesmod" => {
                    payment_method = non_empty(value);
                    raw_payment_method = Some(value.to_owned());
                }
                "szlahu_kifizdat" => {
                    payment_date = Date::strptime("%Y-%m-%d", value.trim()).ok();
                    raw_payment_date = Some(value.to_owned());
                }
                "szlahu_dijbekero_szama" => proforma_number = non_empty(value),
                "szlahu_rendelesszam" => order_number = non_empty(value),
                _ => {}
            }
        }

        Ok(Self {
            document_number: document_number.ok_or(IpnParseError::Missing("szlahu_szamlaszam"))?,
            gross_total,
            raw_gross_total,
            paid_gross,
            raw_paid_gross,
            payment_method,
            raw_payment_method,
            payment_date,
            raw_payment_date,
            proforma_number,
            order_number,
        })
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn parse_decimal(value: &str) -> Option<Decimal> {
    let value = value.trim();

    Decimal::from_str_exact(value)
        .or_else(|error| {
            // The docs show only integer amounts and never specify a decimal
            // separator. Retain the lone-comma tolerance for "1234,56",
            // using the same exactness rule as for dot decimals.
            if value.matches(',').count() == 1 && !value.contains('.') {
                Decimal::from_str_exact(&value.replace(',', "."))
            } else {
                Err(error)
            }
        })
        .ok()
}

// form_urlencoded deliberately repairs broken escapes/UTF-8. Validate first
// so corrupt identity text is never silently substituted. Raw '+' is ASCII
// either way; replacing it with space is left to form_urlencoded.
fn validate_form(body: &[u8]) -> Result<(), IpnParseError> {
    let mut decoded = Vec::with_capacity(body.len());
    let mut bytes = body.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next().and_then(hex_digit);
            let low = bytes.next().and_then(hex_digit);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(IpnParseError::MalformedForm);
            };
            // Each digit is at most 15, so the combined value fits one byte.
            decoded.push(high * 16 + low);
        } else {
            decoded.push(byte);
        }
    }
    std::str::from_utf8(&decoded).map_err(IpnParseError::InvalidUtf8)?;
    Ok(())
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// A request body that is not a valid IPN message.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IpnParseError {
    /// The document-number parameter is absent or whitespace-only.
    #[error("missing IPN parameter {0}")]
    Missing(&'static str),
    /// A percent escape is incomplete or contains a non-hexadecimal digit.
    #[error("malformed IPN form encoding: invalid percent escape")]
    MalformedForm,
    /// The decoded form contains non-UTF-8 text.
    #[error("invalid UTF-8 in IPN form: {0}")]
    InvalidUtf8(#[source] std::str::Utf8Error),
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
        assert_eq!(ipn.gross_total, Some(dec!(10000)));
        assert_eq!(ipn.paid_gross, Some(dec!(5000)));
        assert_eq!(ipn.payment_method.as_deref(), Some("átutalás"));
        assert_eq!(ipn.payment_date, Some(date(2026, 7, 4)));
        assert_eq!(ipn.proforma_number.as_deref(), Some("DB-2026-456"));
        assert_eq!(ipn.order_number.as_deref(), Some("RND1234"));
        assert_eq!(ipn.is_fully_paid(), Some(false));
        assert_eq!(ipn.raw_gross_total.as_deref(), Some("10000"));
        assert_eq!(ipn.raw_paid_gross.as_deref(), Some("5000"));
        assert_eq!(ipn.raw_payment_method.as_deref(), Some("átutalás"));
        assert_eq!(ipn.raw_payment_date.as_deref(), Some("2026-07-04"));
    }

    #[test]
    fn parses_minimal_notification() {
        let body = b"szlahu_szamlaszam=E-2026-123&szlahu_bruttovegosszeg=10000&\
                     szlahu_kifizetettbrutto=10000&szlahu_fizetesmod=kp";
        let ipn = PaymentNotification::from_form_bytes(body).expect("parse");
        assert_eq!(ipn.payment_date, None);
        assert_eq!(ipn.proforma_number, None);
        assert_eq!(ipn.is_fully_paid(), Some(true));
    }

    #[test]
    fn parses_proforma_payment_status_snapshot() {
        let body = b"szlahu_szamlaszam=DB-2026-456&szlahu_bruttovegosszeg=10000&\
                     szlahu_kifizetettbrutto=3000&szlahu_fizetesmod=bankkartya";
        let ipn = PaymentNotification::from_form_bytes(body).expect("parse");
        assert_eq!(ipn.document_number, "DB-2026-456");
        assert_eq!(ipn.paid_gross, Some(dec!(3000)));
        assert_eq!(ipn.proforma_number, None);
        assert_eq!(ipn.is_fully_paid(), Some(false));
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
            (dec!(0), dec!(0), true),
            (dec!(0), dec!(-1), false),
        ] {
            let notification = PaymentNotification::new("E-1", gross, paid, "kp");
            assert_eq!(
                notification.is_fully_paid(),
                Some(expected),
                "{gross} / {paid}"
            );
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
    fn identity_alone_is_a_notification_with_unknown_content() {
        let body = b"szlahu_szamlaszam=E-2026-123";
        let ipn = PaymentNotification::from_form_bytes(body).expect("parse");
        assert_eq!(ipn.document_number, "E-2026-123");
        assert_eq!(ipn.gross_total, None);
        assert_eq!(ipn.paid_gross, None);
        assert_eq!(ipn.payment_method, None);
        assert_eq!(ipn.payment_date, None);
        assert_eq!(ipn.raw_gross_total, None);
        assert_eq!(ipn.raw_paid_gross, None);
        assert_eq!(ipn.raw_payment_method, None);
        assert_eq!(ipn.raw_payment_date, None);
        assert_eq!(ipn.is_fully_paid(), None);
    }

    #[test]
    fn tolerates_comma_decimal_amounts() {
        // The docs never specify the decimal separator; a comma amount must
        // not reject the delivery (szamlazz.hu discards the notification
        // after ten failed retries).
        let body = b"szlahu_szamlaszam=E-2026-123&szlahu_bruttovegosszeg=1000%2C50&\
                     szlahu_kifizetettbrutto=1000.50&szlahu_fizetesmod=kp";
        let ipn = PaymentNotification::from_form_bytes(body).expect("parse");
        assert_eq!(ipn.gross_total, Some(dec!(1000.50)));
        assert_eq!(ipn.raw_gross_total.as_deref(), Some("1000,50"));
        assert_eq!(ipn.is_fully_paid(), Some(true));
    }

    #[test]
    fn unreadable_amounts_are_unknown_and_retained_without_rounding() {
        for text in [
            "",
            "  ",
            "abc",
            "NaN",
            "1,234.56",
            "1,2,3",
            "79228162514264337593543950336",
            "0.00000000000000000000000000001",
            "0.12345678901234567890123456789",
            "0,12345678901234567890123456789",
            "79228162514264337593543950335.1",
        ] {
            for field in ["szlahu_bruttovegosszeg", "szlahu_kifizetettbrutto"] {
                let ipn = PaymentNotification::from_pairs([
                    ("szlahu_szamlaszam", "E"),
                    ("szlahu_bruttovegosszeg", "1"),
                    ("szlahu_kifizetettbrutto", "1"),
                    (field, text),
                ])
                .expect("content accepted");
                let (amount, raw) = if field == "szlahu_bruttovegosszeg" {
                    (ipn.gross_total, &ipn.raw_gross_total)
                } else {
                    (ipn.paid_gross, &ipn.raw_paid_gross)
                };
                assert_eq!(amount, None, "{field}: {text}");
                assert_eq!(raw.as_deref(), Some(text));
                assert_eq!(ipn.is_fully_paid(), None);
            }
        }
    }

    #[test]
    fn unreadable_payment_date_is_unknown_and_retained() {
        let body = b"szlahu_szamlaszam=E&szlahu_bruttovegosszeg=1&\
                     szlahu_kifizetettbrutto=1&szlahu_fizetesmod=kp&\
                     szlahu_kifizdat=2026-07-04T12%3A30%3A00%2B02%3A00";
        let ipn = PaymentNotification::from_form_bytes(body).expect("content accepted");
        assert_eq!(ipn.payment_date, None);
        assert_eq!(
            ipn.raw_payment_date.as_deref(),
            Some("2026-07-04T12:30:00+02:00")
        );

        for text in ["", "  ", "2026-02-30", "2026-07-04junk", "é12345"] {
            let ipn = PaymentNotification::from_pairs([
                ("szlahu_szamlaszam", "E"),
                ("szlahu_kifizdat", text),
            ])
            .expect("content accepted");
            assert_eq!(ipn.payment_date, None, "{text}");
            assert_eq!(ipn.raw_payment_date.as_deref(), Some(text));
        }
    }

    #[test]
    fn raw_content_is_decoded_but_not_trimmed() {
        let ipn = PaymentNotification::from_form_bytes(
            b"szlahu_szamlaszam=E&szlahu_bruttovegosszeg=+1%2C50+&\
              szlahu_kifizetettbrutto=%2B1.50&szlahu_fizetesmod=+new%2Bmethod+&\
              szlahu_kifizdat=+2026-07-04+",
        )
        .expect("parse");
        assert_eq!(ipn.gross_total, Some(dec!(1.50)));
        assert_eq!(ipn.paid_gross, Some(dec!(1.50)));
        assert_eq!(ipn.payment_method.as_deref(), Some("new+method"));
        assert_eq!(ipn.payment_date, Some(date(2026, 7, 4)));
        assert_eq!(ipn.raw_gross_total.as_deref(), Some(" 1,50 "));
        assert_eq!(ipn.raw_paid_gross.as_deref(), Some("+1.50"));
        assert_eq!(ipn.raw_payment_method.as_deref(), Some(" new+method "));
        assert_eq!(ipn.raw_payment_date.as_deref(), Some(" 2026-07-04 "));
    }

    #[test]
    fn empty_content_is_distinct_from_absent_content() {
        for text in ["", "  "] {
            let ipn = PaymentNotification::from_pairs([
                ("szlahu_szamlaszam", "E"),
                ("szlahu_bruttovegosszeg", text),
                ("szlahu_kifizetettbrutto", text),
                ("szlahu_fizetesmod", text),
                ("szlahu_kifizdat", text),
            ])
            .expect("content accepted");
            assert_eq!(ipn.gross_total, None);
            assert_eq!(ipn.paid_gross, None);
            assert_eq!(ipn.payment_method, None);
            assert_eq!(ipn.payment_date, None);
            assert_eq!(ipn.raw_gross_total.as_deref(), Some(text));
            assert_eq!(ipn.raw_paid_gross.as_deref(), Some(text));
            assert_eq!(ipn.raw_payment_method.as_deref(), Some(text));
            assert_eq!(ipn.raw_payment_date.as_deref(), Some(text));
        }
    }

    #[test]
    fn repeated_parameters_keep_the_last_reading_and_raw_text() {
        let ipn = PaymentNotification::from_form_bytes(
            b"szlahu_szamlaszam=old&szlahu_szamlaszam=E&\
              szlahu_bruttovegosszeg=1&szlahu_bruttovegosszeg=&\
              szlahu_kifizetettbrutto=1&szlahu_kifizetettbrutto=bad&\
              szlahu_fizetesmod=kp&szlahu_fizetesmod=+&\
              szlahu_kifizdat=2026-07-04&szlahu_kifizdat=unknown",
        )
        .expect("parse");
        assert_eq!(ipn.document_number, "E");
        assert_eq!(ipn.gross_total, None);
        assert_eq!(ipn.raw_gross_total.as_deref(), Some(""));
        assert_eq!(ipn.paid_gross, None);
        assert_eq!(ipn.raw_paid_gross.as_deref(), Some("bad"));
        assert_eq!(ipn.payment_method, None);
        assert_eq!(ipn.raw_payment_method.as_deref(), Some(" "));
        assert_eq!(ipn.payment_date, None);
        assert_eq!(ipn.raw_payment_date.as_deref(), Some("unknown"));
    }

    #[test]
    fn missing_or_empty_identity_is_refused() {
        for body in [
            "",
            "nonsense=1",
            "not a form",
            "{}",
            "szlahu_szamlaszam",
            "szlahu_szamlaszam=",
            "szlahu_szamlaszam=+%09",
            "szlahu_szamlaszam=E&szlahu_szamlaszam=",
        ] {
            assert!(
                matches!(
                    PaymentNotification::from_form_bytes(body.as_bytes()),
                    Err(IpnParseError::Missing("szlahu_szamlaszam"))
                ),
                "{body}"
            );
        }
        assert!(matches!(
            PaymentNotification::from_pairs([("szlahu_szamlaszam", " \t")]),
            Err(IpnParseError::Missing("szlahu_szamlaszam"))
        ));
    }

    #[test]
    fn malformed_form_is_refused_even_in_unknown_parameters() {
        for suffix in ["%", "%2", "%GG", "%2G", "%G2"] {
            for field in ["szlahu_szamlaszam", "szlahu_fizetesmod", "future"] {
                let body = format!("szlahu_szamlaszam=E&{field}={suffix}");
                assert!(
                    matches!(
                        PaymentNotification::from_form_bytes(body.as_bytes()),
                        Err(IpnParseError::MalformedForm)
                    ),
                    "{body}"
                );
            }
        }
        for body in [
            b"szlahu_szamlaszam=E&future=\xff".as_slice(),
            b"szlahu_szamlaszam=%FF",
            b"szlahu_szamlaszam=E&%FF=x",
            b"szlahu_szamlaszam=E&future=%C3%28",
        ] {
            let error = PaymentNotification::from_form_bytes(body).expect_err("bad UTF-8");
            assert!(matches!(error, IpnParseError::InvalidUtf8(_)));
            assert!(std::error::Error::source(&error).is_some());
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_accepts_exact_numeric_amounts() {
        let json = r#"{"document_number":"E-1","gross_total":12.34,"paid_gross":12.34,"payment_method":"cash"}"#;
        let ipn: PaymentNotification = serde_json::from_str(json).expect("numeric amounts");
        assert_eq!(ipn.gross_total, Some(dec!(12.34)));
        assert_eq!(ipn.paid_gross, Some(dec!(12.34)));
        assert_eq!(ipn.is_fully_paid(), Some(true));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_preserves_representable_scale() {
        for (token, scale) in [("12.3400", 4), ("1.20e-2", 4), ("0.00e-2", 4)] {
            let json = format!(r#"{{"document_number":"E-1","gross_total":{token}}}"#);
            let value: PaymentNotification = serde_json::from_str(&json).expect("exact amount");
            assert_eq!(value.gross_total.expect("known").scale(), scale);
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_amounts_preserve_exact_digits_as_numbers_or_strings() {
        for (text, expected) in [
            ("12.34", dec!(12.34)),
            ("9007199254740993", dec!(9007199254740993)),
            (
                "0.1234567890123456789012345678",
                dec!(0.1234567890123456789012345678),
            ),
            ("79228162514264337593543950335", Decimal::MAX),
            ("-79228162514264337593543950335", Decimal::MIN),
            ("1.234e1", dec!(12.34)),
            ("1E-28", dec!(0.0000000000000000000000000001)),
            ("1.00e-28", dec!(0.0000000000000000000000000001)),
            ("10e-29", dec!(0.0000000000000000000000000001)),
            ("-10e-29", dec!(-0.0000000000000000000000000001)),
            ("0e-29", Decimal::ZERO),
            ("0e-99999999999999999999999999", Decimal::ZERO),
            (
                "1.00000000000000000000000000000e-28",
                dec!(0.0000000000000000000000000001),
            ),
        ] {
            for token in [
                text.to_owned(),
                serde_json::to_string(text).expect("string"),
            ] {
                let json = format!(
                    r#"{{"document_number":"E-1","gross_total":{token},"paid_gross":{token}}}"#
                );
                let direct: PaymentNotification = serde_json::from_str(&json).expect("direct JSON");
                let value: serde_json::Value = serde_json::from_str(&json).expect("value");
                let buffered = serde_json::from_value::<PaymentNotification>(value);
                assert_eq!(direct, buffered.expect("JSON value"));
                assert_eq!(direct.gross_total, Some(expected), "{token}");
                assert_eq!(direct.paid_gross, Some(expected), "{token}");
                let encoded = serde_json::to_value(&direct).expect("serialize");
                assert_eq!(
                    encoded["gross_total"],
                    direct.gross_total.expect("known").to_string()
                );
                assert_eq!(
                    serde_json::from_value::<PaymentNotification>(encoded).expect("round trip"),
                    direct
                );
            }
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_refuses_inexact_or_nonscalar_amounts() {
        let inexact = [
            "79228162514264337593543950336",
            "79228162514264337593543950335.1",
            "0.12345678901234567890123456789",
            "1e-29",
            "1e9999999999999999999",
            "0.12345678901234567890123456789e1",
            "79228162514264337593543950336e-1",
            "1.01e-28",
            "11e-29",
            "1e-9223372036854775808",
        ];
        let mut tokens: Vec<String> = inexact
            .iter()
            .flat_map(|text| {
                [
                    (*text).to_owned(),
                    serde_json::to_string(text).expect("string"),
                ]
            })
            .collect();
        tokens.extend(
            [
                r#""NaN""#,
                r#""""#,
                r#"" 12.34 ""#,
                r#""1_000""#,
                r#""12,34""#,
                "true",
                "[]",
                "{}",
                r#"{"$serde_json::private::Number":"12.34"}"#,
                r#"{"$serde_json::private::RawValue":"12.34"}"#,
            ]
            .map(str::to_owned),
        );
        for field in ["gross_total", "paid_gross"] {
            for token in &tokens {
                let json = format!(r#"{{"document_number":"E-1","{field}":{token}}}"#);
                assert!(
                    serde_json::from_str::<PaymentNotification>(&json).is_err(),
                    "{json}"
                );
                // Buffered JSON can interpret private adapter-shaped objects
                // before our scalar check. Untrusted JSON must be read directly.
                if !token.contains("$serde_json::private::") {
                    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
                    assert!(
                        serde_json::from_value::<PaymentNotification>(value).is_err(),
                        "{json}"
                    );
                }
            }
        }
        let mut object = serde_json::Map::new();
        object.insert("$serde_json::private::Number".into(), "12.34".into());
        let mut notification = serde_json::Map::new();
        notification.insert("document_number".into(), "E-1".into());
        notification.insert("gross_total".into(), serde_json::Value::Object(object));
        assert!(serde_json::from_value::<PaymentNotification>(notification.into()).is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_null_and_omitted_amounts_are_unknown() {
        for json in [
            r#"{"document_number":"E-1"}"#,
            r#"{"document_number":"E-1","gross_total":null,"paid_gross":null}"#,
        ] {
            let ipn: PaymentNotification = serde_json::from_str(json).expect("unknown amounts");
            assert_eq!(ipn.gross_total, None);
            assert_eq!(ipn.paid_gross, None);
            assert_eq!(ipn.is_fully_paid(), None);
            let value = serde_json::to_value(ipn).expect("serialize");
            assert!(value["gross_total"].is_null());
            assert!(value["paid_gross"].is_null());
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_preserves_unknown_content_and_raw_text() {
        let ipn = PaymentNotification::from_form_bytes(
            b"szlahu_szamlaszam=E&szlahu_bruttovegosszeg=&\
              szlahu_kifizetettbrutto=abc&szlahu_kifizdat=unknown",
        )
        .expect("parse");
        let json = serde_json::to_value(&ipn).expect("serialize");
        assert_eq!(json["gross_total"], serde_json::Value::Null);
        assert_eq!(json["raw_gross_total"], "");
        assert_eq!(json["raw_paid_gross"], "abc");
        assert_eq!(
            serde_json::from_value::<PaymentNotification>(json).expect("deserialize"),
            ipn
        );
    }

    #[test]
    fn unknown_parameters_are_ignored() {
        let body = b"szlahu_szamlaszam=E&szlahu_bruttovegosszeg=1&\
                     szlahu_kifizetettbrutto=1&szlahu_fizetesmod=kp&future_param=x";
        PaymentNotification::from_form_bytes(body).expect("parse");
    }
}
