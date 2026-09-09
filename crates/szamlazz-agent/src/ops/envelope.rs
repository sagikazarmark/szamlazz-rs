//! The `xmlszamlavalasz` reply (response version 2): the one envelope every
//! operation that issues or registers on an invoice answers with (invoice
//! creation, storno, credit-entry registration, the PDF query), and the
//! document it names.
//!
//! Crate-private: the issuing operations read it through [`parse_reply`] and
//! [`parse_issued`], the credit-entry registration through its [`Body`] under
//! the shared verdict (`xml::valasz`); the public face is [`CreatedInvoice`],
//! re-exported from [`ops::invoice`](crate::ops::invoice).

use rust_decimal::Decimal;

use crate::error::{ErrorCode, ParseError, ResponseError};
use crate::types::{InvoiceNumber, Pdf};
use crate::wire::RawResponse;
use crate::xml;

/// The envelope's root element.
pub(crate) const ROOT: &str = "xmlszamlavalasz";
/// The envelope's namespace.
pub(crate) const NAMESPACE: &str = "http://www.szamlazz.hu/xmlszamlavalasz";

/// A successfully issued numbered invoice: the answer to a create, a storno,
/// and (with the PDF) a PDF query.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct CreatedInvoice {
    /// The assigned invoice number (`szamlaszam`).
    pub invoice_number: InvoiceNumber,
    /// szamlazz.hu's internal document identifier (`szlahu_id` header).
    ///
    /// The same value the XML query returns as
    /// [`InvoiceInfo::id`](crate::ops::query_xml::InvoiceInfo::id); for a
    /// storno invoice, the original's identifier reappears as the storno's
    /// [`economic_event_id`](crate::ops::query_xml::InvoiceInfo::economic_event_id).
    /// A document identifier, not an account or supplier identifier. `None`
    /// when the header is absent or not a number.
    pub document_id: Option<u64>,
    /// Net total (`szamlanetto`).
    pub net_total: Option<Decimal>,
    /// Gross total (`szamlabrutto`).
    pub gross_total: Option<Decimal>,
    /// Outstanding amount (`kintlevoseg`).
    #[doc(alias = "kintlévőség")]
    pub outstanding: Option<Decimal>,
    /// Buyer-facing account/payment URL (`vevoifiokurl`).
    pub customer_account_url: Option<String>,
    /// The document PDF, when requested.
    pub pdf: Option<Pdf>,
    /// Whether the invoice was issued but Számlázz.hu could not deliver its
    /// notification (`56`). The issued invoice must not be retried.
    pub notification_delivery_failed: bool,
}

impl CreatedInvoice {
    /// Whether this document is a reversal of `original`: a *different*
    /// invoice number with a gross total that is not positive.
    ///
    /// The check every caller must make after a
    /// [`StornoInvoice`](crate::ops::storno::StornoInvoice): szamlazz.hu
    /// answers a storno request for a proforma or a delivery note with a
    /// success-shaped response that merely echoes the requested document
    /// (same number, positive totals) and reverses nothing. A repeat storno of
    /// an already reversed invoice also passes this check: it echoes the
    /// existing storno invoice, which is a genuine reversal. So does the
    /// storno of a zero-total invoice, whose storno document carries a gross
    /// of `0`.
    ///
    /// Returns `false` when the gross total is unknown.
    #[must_use]
    pub fn reverses(&self, original: &InvoiceNumber) -> bool {
        self.invoice_number != *original
            && self.gross_total.is_some_and(|gross| gross <= Decimal::ZERO)
    }
}

/// What a successful `xmlszamlavalasz` reply names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Reply {
    /// A numbered document.
    Issued(CreatedInvoice),
    /// A success without a document number: a PDF preview (`elonezetpdf`),
    /// which issues nothing; the PDF it carries, if any.
    Unnumbered { pdf: Option<Pdf> },
}

/// The payload of the `xmlszamlavalasz` envelope after the verdict, every
/// element as its raw text: the reply of a document that was issued although
/// its notification failed (56) is read leniently, so the values are parsed
/// by the caller, strictly or not.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct Body {
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub(crate) szamlaszam: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub(crate) szamlanetto: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub(crate) szamlabrutto: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub(crate) kintlevoseg: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub(crate) vevoifiokurl: Option<String>,
    #[serde(default, deserialize_with = "xml::de::empty_as_none")]
    pub(crate) pdf: Option<String>,
}

impl Body {
    /// The invoice number from the body, else from the `szlahu_szamlaszam`
    /// header; trimmed on both channels, and a blank one is none.
    pub(crate) fn invoice_number(&self, response: &RawResponse) -> Option<InvoiceNumber> {
        self.szamlaszam
            .as_deref()
            .and_then(nonblank_invoice_number)
            .or_else(|| {
                response
                    .szlahu("szlahu_szamlaszam")
                    .as_deref()
                    .and_then(nonblank_invoice_number)
            })
    }

    /// The customer account URL from the body, else from the
    /// `szlahu_vevoifiokurl` header.
    pub(crate) fn customer_account_url(&self, response: &RawResponse) -> Option<String> {
        self.vevoifiokurl.clone().or_else(|| {
            response
                .szlahu("szlahu_vevoifiokurl")
                .filter(|url| !url.is_empty())
        })
    }

    /// The decoded PDF, when the body carries one.
    fn pdf(&self) -> Result<Option<Pdf>, ParseError> {
        self.pdf.as_deref().map(Pdf::from_base64).transpose()
    }
}

/// A decimal from the body element `field`, else from the response header
/// `header`; `None` when neither is present.
///
/// # Errors
///
/// [`ParseError::Invalid`] naming the element or header whose text is not a
/// decimal.
pub(crate) fn decimal_body_or_header(
    body_value: Option<&str>,
    field: &'static str,
    response: &RawResponse,
    header: &'static str,
) -> Result<Option<Decimal>, ParseError> {
    match body_value {
        Some(value) => parse_decimal(value, field).map(Some),
        None => parse_decimal_header(response, header),
    }
}

/// Parses the `xmlszamlavalasz` reply of an operation that issues a
/// document.
///
/// Tolerates exactly one error: 56 (issued, notification not delivered), in
/// the headers or the body, when the reply also names the document; the
/// document is then reported with `notification_delivery_failed` and the
/// optional values read leniently (a malformed total or PDF is `None`, never
/// a parse failure hiding a successful issuance). Every other error the
/// headers, the status or the verdict report is final.
pub(crate) fn parse_reply(response: &RawResponse) -> Result<Reply, ResponseError> {
    let header_error = response.header_verdict()?;

    if let Some(error) = &header_error
        && error.code != ErrorCode::InvoiceNotificationDeliveryFailed
    {
        return Err(error.clone().into());
    }

    let (verdict, body) = match parse_envelope(response.body()) {
        Ok((verdict, body)) => (Some(verdict), body),
        // A 56 in the headers may come with a body that is not the envelope
        // ("notification failed" as text): the headers alone name the document.
        Err(_) if header_error.is_some() => (None, Body::default()),
        Err(parse_error) => return Err(parse_error.into()),
    };
    let body_error = verdict.as_ref().and_then(xml::Verdict::api_error);

    if let Some(error) = &body_error
        && error.code != ErrorCode::InvoiceNotificationDeliveryFailed
    {
        return Err(error.clone().into());
    }

    let notification_delivery_failed = header_error.is_some() || body_error.is_some();

    let Some(invoice_number) = body.invoice_number(response) else {
        // 56 without a number: an error after all.
        if let Some(error) = header_error.or(body_error) {
            return Err(error.into());
        }
        return Ok(Reply::Unnumbered { pdf: body.pdf()? });
    };

    // Issued with a failed notification: the document is what matters, so
    // a malformed optional value is dropped rather than reported.
    let read = |value| lenient(notification_delivery_failed, value);

    Ok(Reply::Issued(CreatedInvoice {
        invoice_number,
        document_id: parse_document_id_header(response),
        net_total: read(decimal_body_or_header(
            body.szamlanetto.as_deref(),
            "szamlanetto",
            response,
            "szlahu_nettovegosszeg",
        ))?,
        gross_total: read(decimal_body_or_header(
            body.szamlabrutto.as_deref(),
            "szamlabrutto",
            response,
            "szlahu_bruttovegosszeg",
        ))?,
        outstanding: read(decimal_body_or_header(
            body.kintlevoseg.as_deref(),
            "kintlevoseg",
            response,
            "szlahu_kintlevoseg",
        ))?,
        customer_account_url: body.customer_account_url(response),
        pdf: lenient(notification_delivery_failed, body.pdf())?,
        notification_delivery_failed,
    }))
}

/// `value` as read, or `None` in place of a failure when `lenient`.
fn lenient<T>(
    lenient: bool,
    value: Result<Option<T>, ParseError>,
) -> Result<Option<T>, ParseError> {
    if lenient {
        Ok(value.unwrap_or(None))
    } else {
        value
    }
}

/// Parses an operation that must issue a numbered document (a storno, a PDF
/// query).
///
/// # Errors
///
/// Everything [`parse_reply`] refuses, and a success without a number
/// (`ParseError::Missing("szamlaszam")`).
pub(crate) fn parse_issued(response: &RawResponse) -> Result<CreatedInvoice, ResponseError> {
    match parse_reply(response)? {
        Reply::Issued(created) => Ok(created),
        Reply::Unnumbered { .. } => Err(ParseError::Missing("szamlaszam").into()),
    }
}

/// The envelope's verdict and payload, read from the same text.
fn parse_envelope(body: &[u8]) -> Result<(xml::Verdict, Body), ParseError> {
    let text = xml::response_text(body, ROOT, NAMESPACE)?;
    let verdict = quick_xml::de::from_str(text)?;
    let payload = quick_xml::de::from_str(text)?;

    Ok((verdict, payload))
}

fn nonblank_invoice_number(value: &str) -> Option<InvoiceNumber> {
    let value = value.trim();
    (!value.is_empty()).then(|| InvoiceNumber::new(value))
}

/// The document identifier from the `szlahu_id` header.
///
/// Lenient on purpose: the identifier is auxiliary, and a successful issuance
/// must never be reported as a parse failure because of it. An absent, blank,
/// or non-numeric header is `None`.
fn parse_document_id_header(response: &RawResponse) -> Option<u64> {
    response
        .header("szlahu_id")
        .and_then(|value| value.trim().parse().ok())
}

fn parse_decimal(value: &str, field: &'static str) -> Result<Decimal, ParseError> {
    value
        .trim()
        .parse()
        .map_err(|error: rust_decimal::Error| ParseError::Invalid {
            field,
            message: error.to_string(),
        })
}

fn parse_decimal_header(
    response: &RawResponse,
    name: &'static str,
) -> Result<Option<Decimal>, ParseError> {
    response
        .header(name)
        .map(|value| parse_decimal(value, name))
        .transpose()
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    fn xml(inner: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><{ROOT} xmlns="{NAMESPACE}">{inner}</{ROOT}>"#
        )
        .into_bytes()
    }

    /// What one row of the table expects of `parse_issued`.
    #[derive(Debug)]
    enum Expect {
        Issued {
            number: &'static str,
            gross: Option<Decimal>,
            notification_failed: bool,
        },
        Api(ErrorCode),
        Missing(&'static str),
        Invalid(&'static str),
        HttpStatus(u16),
        Unavailable,
    }

    /// One row of the table: a label, the response headers, its status when
    /// the client supplied one, its body, and what `parse_issued` answers.
    struct Case {
        label: &'static str,
        headers: Vec<(&'static str, &'static str)>,
        status: Option<u16>,
        body: Vec<u8>,
        expected: Expect,
    }

    /// The direct table of the envelope parser: the success shapes, the one
    /// tolerated error (56, in the headers or the body, with a body that is
    /// or is not the envelope, with malformed optional values), and the
    /// refusals (a real error in either channel, 56 without a number, a
    /// non-2xx with no szamlazz.hu answer, `szlahu_down`, a malformed total
    /// on a plain success).
    #[test]
    #[expect(clippy::too_many_lines, reason = "one table, every row of it")]
    fn parse_issued_table() {
        let cases: Vec<Case> = vec![
            Case {
                label: "plain success",
                headers: vec![("szlahu_id", "924307402")],
                status: None,
                body: xml(
                    "<sikeres>true</sikeres><szamlaszam>E-1</szamlaszam><szamlanetto>1000</szamlanetto><szamlabrutto>1270</szamlabrutto><kintlevoseg>1270</kintlevoseg>",
                ),
                expected: Expect::Issued {
                    number: "E-1",
                    gross: Some(dec!(1270)),
                    notification_failed: false,
                },
            },
            Case {
                label: "success with totals from the headers only",
                headers: vec![
                    ("szlahu_szamlaszam", "E-2"),
                    ("szlahu_bruttovegosszeg", "12700"),
                ],
                status: Some(200),
                body: xml("<sikeres>true</sikeres>"),
                expected: Expect::Issued {
                    number: "E-2",
                    gross: Some(dec!(12700)),
                    notification_failed: false,
                },
            },
            Case {
                label: "56 in the headers, non-XML body",
                headers: vec![
                    ("szlahu_error_code", "56"),
                    ("szlahu_error", "notification failed"),
                    ("szlahu_szamlaszam", "E-3"),
                    ("szlahu_bruttovegosszeg", "38100"),
                ],
                status: Some(500),
                body: b"notification failed".to_vec(),
                expected: Expect::Issued {
                    number: "E-3",
                    gross: Some(dec!(38100)),
                    notification_failed: true,
                },
            },
            Case {
                label: "56 in the body only, malformed total dropped",
                headers: vec![],
                status: None,
                body: xml(
                    "<sikeres>false</sikeres><hibakod> 56 </hibakod><hibauzenet>x</hibauzenet><szamlaszam>E-4</szamlaszam><szamlabrutto>not-a-number</szamlabrutto>",
                ),
                expected: Expect::Issued {
                    number: "E-4",
                    gross: None,
                    notification_failed: true,
                },
            },
            Case {
                label: "56 in both, malformed header and PDF dropped",
                headers: vec![
                    ("szlahu_error_code", "56"),
                    ("szlahu_szamlaszam", "E-5"),
                    ("szlahu_bruttovegosszeg", ""),
                ],
                status: None,
                body: xml("<sikeres>false</sikeres><hibakod>56</hibakod><pdf>not-base64</pdf>"),
                expected: Expect::Issued {
                    number: "E-5",
                    gross: None,
                    notification_failed: true,
                },
            },
            Case {
                label: "56 without a number is the error",
                headers: vec![("szlahu_szamlaszam", "%20")],
                status: None,
                body: xml(
                    "<sikeres>false</sikeres><hibakod>56</hibakod><hibauzenet>x</hibauzenet>",
                ),
                expected: Expect::Api(ErrorCode::InvoiceNotificationDeliveryFailed),
            },
            Case {
                label: "header error takes precedence over a success body",
                headers: vec![("szlahu_error_code", "202"), ("szlahu_error", "prefix")],
                status: None,
                body: xml("<sikeres>true</sikeres><szamlaszam>E-6</szamlaszam>"),
                expected: Expect::Api(ErrorCode::UnregisteredPrefix),
            },
            Case {
                label: "body error without a header",
                headers: vec![],
                status: None,
                body: xml(
                    "<sikeres>false</sikeres><hibakod>3</hibakod><hibauzenet>login</hibauzenet>",
                ),
                expected: Expect::Api(ErrorCode::InvalidCredentials),
            },
            Case {
                label: "body error without a code",
                headers: vec![],
                status: None,
                body: xml("<sikeres>false</sikeres><hibauzenet>no code</hibauzenet>"),
                expected: Expect::Api(ErrorCode::Absent),
            },
            Case {
                label: "success without a number",
                headers: vec![],
                status: None,
                body: xml("<sikeres>true</sikeres><pdf>JVBERi0=</pdf>"),
                expected: Expect::Missing("szamlaszam"),
            },
            Case {
                label: "a malformed total on a plain success is a parse failure",
                headers: vec![],
                status: None,
                body: xml(
                    "<sikeres>true</sikeres><szamlaszam>E-7</szamlaszam><szamlanetto>junk</szamlanetto>",
                ),
                expected: Expect::Invalid("szamlanetto"),
            },
            Case {
                label: "a malformed header total on a plain success is a parse failure",
                headers: vec![("szlahu_kintlevoseg", "junk")],
                status: None,
                body: xml("<sikeres>true</sikeres><szamlaszam>E-8</szamlaszam>"),
                expected: Expect::Invalid("szlahu_kintlevoseg"),
            },
            Case {
                label: "a proxy's page is refused by status",
                headers: vec![("content-type", "text/html")],
                status: Some(502),
                body: b"<html/>".to_vec(),
                expected: Expect::HttpStatus(502),
            },
            Case {
                label: "szlahu_down",
                headers: vec![("szlahu_down", "maintenance")],
                status: Some(200),
                body: xml("<sikeres>true</sikeres><szamlaszam>E-9</szamlaszam>"),
                expected: Expect::Unavailable,
            },
        ];

        for Case {
            label,
            headers,
            status,
            body,
            expected,
        } in cases
        {
            let mut response = RawResponse::new(headers, body);
            if let Some(status) = status {
                response = response.with_status(status);
            }
            let result = parse_issued(&response);
            match (&expected, result) {
                (
                    Expect::Issued {
                        number,
                        gross,
                        notification_failed,
                    },
                    Ok(created),
                ) => {
                    assert_eq!(created.invoice_number.as_str(), *number, "{label}");
                    assert_eq!(created.gross_total, *gross, "{label}");
                    assert_eq!(
                        created.notification_delivery_failed, *notification_failed,
                        "{label}"
                    );
                }
                (Expect::Api(code), Err(ResponseError::Api(api))) => {
                    assert_eq!(api.code, *code, "{label}");
                }
                (Expect::Missing(field), Err(ResponseError::Parse(ParseError::Missing(got))))
                | (
                    Expect::Invalid(field),
                    Err(ResponseError::Parse(ParseError::Invalid { field: got, .. })),
                ) => {
                    assert_eq!(got, *field, "{label}");
                }
                (
                    Expect::HttpStatus(status),
                    Err(ResponseError::HttpStatus { status: got, .. }),
                ) => {
                    assert_eq!(got, *status, "{label}");
                }
                (Expect::Unavailable, Err(ResponseError::ServiceUnavailable(_))) => {}
                (expected, got) => panic!("{label}: expected {expected:?}, got {got:?}"),
            }
        }
    }

    /// The reply tells an issued document from a preview: a success without
    /// a number is `Unnumbered` with whatever PDF it carries.
    #[test]
    fn a_success_without_a_number_is_unnumbered() {
        let preview =
            RawResponse::new::<&str, &str>([], xml("<sikeres>true</sikeres><pdf>JVBERi0=</pdf>"));
        match parse_reply(&preview).expect("reply") {
            Reply::Unnumbered { pdf } => assert_eq!(pdf.expect("pdf").as_bytes(), b"%PDF-"),
            other @ Reply::Issued(_) => panic!("expected an unnumbered reply, got {other:?}"),
        }

        let bare = RawResponse::new::<&str, &str>([], xml("<sikeres>true</sikeres>"));
        assert_eq!(
            parse_reply(&bare).expect("reply"),
            Reply::Unnumbered { pdf: None }
        );

        // A malformed PDF on a plain success is a parse failure, as on an
        // issued document.
        let malformed =
            RawResponse::new::<&str, &str>([], xml("<sikeres>true</sikeres><pdf>not-base64</pdf>"));
        assert!(matches!(
            parse_reply(&malformed),
            Err(ResponseError::Parse(ParseError::Base64(_)))
        ));
    }

    /// Every optional value comes from the body first and the header second,
    /// and the document id from its header alone, leniently.
    #[test]
    fn body_values_take_precedence_over_headers() {
        let response = RawResponse::new(
            [
                ("szlahu_szamlaszam", "HEADER-1"),
                ("szlahu_id", " 42 "),
                ("szlahu_nettovegosszeg", "1"),
                ("szlahu_bruttovegosszeg", "2"),
                ("szlahu_kintlevoseg", "3"),
                ("szlahu_vevoifiokurl", "https%3A%2F%2Fheader.example"),
            ],
            xml(
                "<sikeres>true</sikeres><szamlaszam>BODY-1</szamlaszam><szamlanetto>10</szamlanetto><szamlabrutto>20</szamlabrutto><kintlevoseg>30</kintlevoseg><vevoifiokurl>https://body.example</vevoifiokurl>",
            ),
        );
        let created = parse_issued(&response).expect("issued");
        assert_eq!(created.invoice_number.as_str(), "BODY-1");
        assert_eq!(created.document_id, Some(42));
        assert_eq!(created.net_total, Some(dec!(10)));
        assert_eq!(created.gross_total, Some(dec!(20)));
        assert_eq!(created.outstanding, Some(dec!(30)));
        assert_eq!(
            created.customer_account_url.as_deref(),
            Some("https://body.example")
        );

        let headers_only = RawResponse::new(
            [
                ("szlahu_szamlaszam", "HEADER-1"),
                ("szlahu_id", "not-a-number"),
                ("szlahu_kintlevoseg", "3"),
                ("szlahu_vevoifiokurl", "https%3A%2F%2Fheader.example"),
            ],
            xml("<sikeres>true</sikeres>"),
        );
        let created = parse_issued(&headers_only).expect("issued");
        assert_eq!(created.invoice_number.as_str(), "HEADER-1");
        assert_eq!(created.document_id, None, "auxiliary: never a failure");
        assert_eq!(created.net_total, None);
        assert_eq!(created.outstanding, Some(dec!(3)));
        assert_eq!(
            created.customer_account_url.as_deref(),
            Some("https://header.example")
        );
    }

    fn created(number: &str, gross: Option<Decimal>) -> CreatedInvoice {
        CreatedInvoice {
            invoice_number: InvoiceNumber::new(number),
            document_id: None,
            net_total: None,
            gross_total: gross,
            outstanding: None,
            customer_account_url: None,
            pdf: None,
            notification_delivery_failed: false,
        }
    }

    #[test]
    fn reverses_requires_a_new_number_with_a_non_positive_gross() {
        let original = InvoiceNumber::new("CTEST-2026-40");

        // A genuine storno invoice (also what a repeat storno echoes).
        assert!(created("CTEST-2026-42", Some(dec!(-1270))).reverses(&original));
        // The storno of a zero-total invoice: a new number, a gross of 0.
        assert!(created("CTEST-2026-42", Some(dec!(0))).reverses(&original));

        // Storno of a proforma or delivery note: the requested document is
        // echoed unchanged.
        assert!(!created("CTEST-2026-40", Some(dec!(1270))).reverses(&original));
        // A different number with positive totals reversed nothing either.
        assert!(!created("CTEST-2026-41", Some(dec!(1270))).reverses(&original));
        // Same number, negative gross (not observed) is not a reversal.
        assert!(!created("CTEST-2026-40", Some(dec!(-1270))).reverses(&original));
        // Same number, zero gross: the echo of a zero-total proforma.
        assert!(!created("CTEST-2026-40", Some(dec!(0))).reverses(&original));
        // Unknown totals cannot prove a reversal.
        assert!(!created("CTEST-2026-42", None).reverses(&original));
    }
}
