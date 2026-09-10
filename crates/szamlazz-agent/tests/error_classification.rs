//! Source-derived error catalogue checks through public response parsers.
//! Sources (2026-09-10): <https://docs.szamlazz.hu/agent/basics/error-handling>,
//! <https://docs.szamlazz.hu/agent/generating_receipt/response> and
//! <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency>.
//! These are synthetic exchanges, not observations of server execution.

use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::ops::receipt::{
    CreateReceipt, QueryReceipt, ReceiptSelector, SendReceipt, StornoReceipt,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{Currency, ErrorCode, Language, OutcomeClass, PaymentMethod, ResponseError};

fn invoice() -> CreateInvoice {
    CreateInvoice::new(
        InvoiceKind::invoice(),
        InvoiceHeader::new(
            jiff::civil::date(2026, 9, 10),
            jiff::civil::date(2026, 9, 10),
            PaymentMethod::Cash,
            Currency::HUF,
            Language::Hungarian,
        ),
        Buyer::new("Example", "1111", "Budapest", "Example utca 1."),
        vec![],
    )
}

fn body(root: &str, code: &str, message: &str) -> RawResponse {
    RawResponse::new([] as [(&str, &str); 0], format!(
        "<{root} xmlns=\"http://www.szamlazz.hu/{root}\"><sikeres>false</sikeres><hibakod>{code}</hibakod><hibauzenet>{message}</hibauzenet></{root}>"
    ).into_bytes())
}

fn assert_error(error: ResponseError, code: &ErrorCode, class: OutcomeClass, message: &str) {
    assert_eq!(error.outcome_class(), class);
    let ResponseError::Api(api) = error else {
        panic!("expected code, got {error:?}")
    };
    assert_eq!(&api.code, code);
    assert_eq!(api.message, message);
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "literal source-derived catalogue and its checks"
)]
fn documented_receipt_and_simplified_image_codes_are_settled() {
    use ErrorCode::{
        InvalidReceiptPrefix, ReceiptGrossNotWhole, ReceiptNetPrecision, ReceiptNotFound,
        ReceiptPaymentMismatch, ReceiptPrefixUsedForInvoices, ReceiptVatPrecision,
        SimplifiedImageAccountIncompatible, SimplifiedImageCannotCorrect,
        SimplifiedImageDocumentForbidden, SimplifiedImageItemLimit,
        SimplifiedImagePrepaymentVatMismatch, SimplifiedImageVatInvalid,
    };
    use OutcomeClass::{NotFound, Rejected};

    // Tokens/classes are literal expectations from the sources, not enum iteration.
    // The supplement gives descriptions for 336–340; their Hungarian text below
    // is synthetic. The 363–556 messages are quoted from the general table.
    let cases = [
        (
            "336",
            ReceiptPrefixUsedForInvoices,
            Rejected,
            "Számlához használt előtag",
        ),
        (
            "337",
            InvalidReceiptPrefix,
            Rejected,
            "Érvénytelen nyugta előtag",
        ),
        ("339", ReceiptNotFound, NotFound, "Nem létező nyugtaszám"),
        (
            "340",
            ReceiptPaymentMismatch,
            Rejected,
            "A fizetett összeg eltér a bruttó összegtől",
        ),
        (
            "363",
            ReceiptGrossNotWhole,
            Rejected,
            "A tétel bruttó értékének egész számnak kell lennie. Termék: **",
        ),
        (
            "364",
            ReceiptNetPrecision,
            Rejected,
            "A tétel nettó értéke maximum 2 tizedes jegyet tartalmazhat. Termék: **",
        ),
        (
            "365",
            ReceiptVatPrecision,
            Rejected,
            "A tétel áfa értéke maximum 2 tizedes jegyet tartalmazhat. Termék: **",
        ),
        (
            "551",
            SimplifiedImageAccountIncompatible,
            Rejected,
            "Egyszerűsített számlakép bekapcsolt OSS és nem magyar adószám esetén nem használható.",
        ),
        (
            "552",
            SimplifiedImageItemLimit,
            Rejected,
            "Egyszerűsített számlakép esetén maximum két tétel adható meg",
        ),
        (
            "553",
            SimplifiedImageVatInvalid,
            Rejected,
            "Egyszerűsített számlakép esetén érvénytelen adókulcs.",
        ),
        (
            "554",
            SimplifiedImageCannotCorrect,
            Rejected,
            "Egyszerűsített számlaképet használó számlát nem lehet helyesbíteni.",
        ),
        (
            "555",
            SimplifiedImagePrepaymentVatMismatch,
            Rejected,
            "Egyszerűsített számlakép esetén a tételek áfakulcsai nem különbözhetnek",
        ),
        (
            "556",
            SimplifiedImageDocumentForbidden,
            Rejected,
            "Egyszerűsített számlaképet szállítólevél és helyesbítő számla esetén nem lehet használni",
        ),
    ];
    let receipt = CreateReceipt::new("NY", PaymentMethod::Cash, Currency::HUF, vec![]);
    for (token, code, class, message) in cases {
        assert_eq!(ErrorCode::from(token), code);
        assert_eq!(ErrorCode::from(token.to_owned()), code);
        assert_eq!(
            ErrorCode::from(token.parse::<u16>().expect("numeric source code")),
            code
        );
        assert_eq!(token.parse::<ErrorCode>().unwrap(), code);
        assert_eq!(code.code(), token);
        assert_eq!(code.outcome_class(), class);
        assert!(!code.is_retryable());
        assert!(!code.is_credential_error());
        let error = if token.starts_with('5') {
            invoice()
                .parse(&body("xmlszamlavalasz", token, message))
                .expect_err("documented invoice refusal")
        } else {
            receipt
                .parse(&body("xmlnyugtavalasz", token, message))
                .expect_err("documented receipt refusal")
        };
        assert_error(error, &code, class, message);
    }
}

#[test]
fn receipt_not_found_reaches_query_storno_and_send() {
    let response = body("xmlnyugtavalasz", "339", "Nem létező nyugtaszám");
    let query = QueryReceipt::new(ReceiptSelector::ReceiptNumber("NY-1".into()));
    for error in [
        query.parse(&response).expect_err("receipt not found"),
        StornoReceipt::new("NY-1")
            .parse(&response)
            .expect_err("receipt not found"),
        SendReceipt::new("NY-1")
            .parse(&body("xmlnyugtasendvalasz", "339", "Nem létező nyugtaszám"))
            .expect_err("receipt not found"),
    ] {
        assert_error(
            error,
            &ErrorCode::ReceiptNotFound,
            OutcomeClass::NotFound,
            "Nem létező nyugtaszám",
        );
    }
}

#[test]
fn headers_and_open_code_controls_keep_their_meaning() {
    for (token, class, retryable) in [
        ("336", OutcomeClass::Rejected, false),
        ("339", OutcomeClass::NotFound, false),
        ("554", OutcomeClass::Rejected, false),
        ("7", OutcomeClass::NotFound, false),
        ("335", OutcomeClass::Rejected, false),
        ("338", OutcomeClass::Rejected, false),
        ("55", OutcomeClass::Unknown, true),
        ("56", OutcomeClass::Unknown, false),
        ("366", OutcomeClass::Unknown, false),
        ("557", OutcomeClass::Unknown, false),
        ("999", OutcomeClass::Unknown, false),
        ("NAV_FUTURE", OutcomeClass::Unknown, false),
    ] {
        let code = ErrorCode::from(token);
        assert_eq!(code.is_retryable(), retryable);
        assert!(!code.is_credential_error());
        let header = RawResponse::new(
            [
                ("szlahu_error_code", token),
                ("szlahu_error", "%C3%81rva+%C3%BCzenet"),
            ],
            vec![],
        );
        assert_error(
            invoice().parse(&header).expect_err("header code"),
            &code,
            class,
            "Árva üzenet",
        );
        assert_error(
            invoice()
                .parse(&body("xmlszamlavalasz", token, "Árva üzenet"))
                .expect_err("body code"),
            &code,
            class,
            "Árva üzenet",
        );
    }
    assert_error(
        invoice()
            .parse(&body("xmlszamlavalasz", "", "Nincs kód"))
            .expect_err("absent code"),
        &ErrorCode::Absent,
        OutcomeClass::Unknown,
        "Nincs kód",
    );
    // Code 7 on receipt send can mean a missing subject, not a missing receipt.
    assert_error(
        SendReceipt::new("NY-1")
            .parse(&body("xmlnyugtasendvalasz", "7", "Hiányzó adat"))
            .expect_err("missing subject"),
        &ErrorCode::MissingData,
        OutcomeClass::NotFound,
        "Hiányzó adat",
    );
    let numbered = RawResponse::new(
        [
            ("szlahu_error_code", "56"),
            ("szlahu_szamlaszam", "E-2026-1"),
        ],
        vec![],
    );
    let issued = invoice()
        .parse(&numbered)
        .expect("numbered warning")
        .into_issued()
        .expect("issued document");
    assert_eq!(issued.invoice_number.as_str(), "E-2026-1");
    assert!(issued.notification_delivery_failed);
}
