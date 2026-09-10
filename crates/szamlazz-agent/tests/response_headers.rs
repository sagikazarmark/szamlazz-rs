//! Response headers through public helpers and real operation entry points.
use rust_decimal::dec;
use szamlazz_agent::ops::{
    credit_entry::RegisterCreditEntry, query_pdf::QueryInvoicePdf, storno::StornoInvoice,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{ErrorCode, InvoiceSelector, ResponseError};

#[test]
fn create_storno_and_credit_entry_share_encoded_payment_method_headers() {
    use szamlazz_agent::ops::invoice::{
        Buyer, CreateInvoice, CreatedInvoice, InvoiceHeader, InvoiceKind,
    };
    use szamlazz_agent::{Currency, Language, PaymentMethod};
    let create = CreateInvoice::new(
        InvoiceKind::invoice(),
        InvoiceHeader::new(
            jiff::civil::date(2026, 9, 10),
            jiff::civil::date(2026, 9, 10),
            PaymentMethod::Cash,
            Currency::HUF,
            Language::Hungarian,
        ),
        Buyer::new("Buyer", "1111", "Budapest", "Street 1"),
        vec![],
    );
    for (header, expected) in [
        (None, None),
        (Some(""), None),
        (Some("%C3%A1tutal%C3%A1s"), Some(PaymentMethod::Transfer)),
        (Some("k%C3%A9szp%C3%A9nz"), Some(PaymentMethod::Cash)),
        (
            Some("future%2Bmethod+%252B"),
            Some(PaymentMethod::Other("future+method %2B".into())),
        ),
        (Some("%C3%28"), Some(PaymentMethod::Other("%C3%28".into()))),
    ] {
        let headers: Vec<_> = header
            .map(|h| ("szlahu_fizetesmod", h))
            .into_iter()
            .collect();
        // There is no XML payment-method field in this envelope.
        let raw = RawResponse::new(headers, br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>I-1</szamlaszam><fizetesmod>wrong</fizetesmod></xmlszamlavalasz>"#.to_vec());
        let created = create
            .parse(&raw)
            .expect("create")
            .into_issued()
            .expect("numbered");
        let storno = StornoInvoice::new("I-0").parse(&raw).expect("storno");
        let credit = RegisterCreditEntry::new("I-1")
            .parse(&raw)
            .expect("credit entry");
        assert_eq!(created.payment_method, expected);
        assert_eq!(storno.payment_method, expected);
        assert_eq!(credit.payment_method, expected);
        let mut json = serde_json::to_value(&created).expect("JSON");
        assert_eq!(
            serde_json::from_value::<CreatedInvoice>(json.clone()).expect("roundtrip"),
            created
        );
        json.as_object_mut()
            .expect("object")
            .remove("payment_method");
        assert_eq!(
            serde_json::from_value::<CreatedInvoice>(json)
                .expect("old JSON")
                .payment_method,
            None
        );
    }
}

#[test]
fn session_cookie_requires_an_exact_case_sensitive_cookie_pair() {
    for (cookies, expected) in [
        (
            vec!["JSESSIONIDOTHER=wrong", "JSESSIONID=right; Path=/"],
            Some("JSESSIONID=right"),
        ),
        (vec!["JSESSIONIDOTHER=wrong"], None),
        (vec!["JSESSIONID; Path=/"], None),
        (
            vec!["JSESSIONID", "JSESSIONID=right"],
            Some("JSESSIONID=right"),
        ),
        (vec!["JSESSIONID="], Some("JSESSIONID=")),
        (
            vec!["JSESSIONID=ABC123; HttpOnly"],
            Some("JSESSIONID=ABC123"),
        ),
        (vec!["JSESSIONID=abc==; Path=/"], Some("JSESSIONID=abc==")),
        (
            vec![" \tJSESSIONID \t= \tabc==\t ; Path=/"],
            Some("JSESSIONID=abc=="),
        ),
        (vec!["jsessionid=wrong", "Jsessionid=wrong"], None),
        (
            vec![
                "=wrong",
                "other=1; JSESSIONID=attribute",
                "JSESSIONID=right",
            ],
            Some("JSESSIONID=right"),
        ),
    ] {
        let raw = RawResponse::new(
            cookies.iter().map(|value| ("sEt-CoOkIe", *value)),
            Vec::new(),
        );
        assert_eq!(raw.session_cookie().as_deref(), expected, "{cookies:?}");
    }
}

/// Synthetic HTTP combinations pin the library's policy, not vendor emission.
#[test]
fn down_then_error_header_then_status_then_body() {
    let body = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>7</hibakod><hibauzenet>missing</hibauzenet></xmlszamlavalasz>"#;
    let request = StornoInvoice::new("I-1");
    let raw = RawResponse::new::<&str, &str>([], body.to_vec()).with_status(200);
    assert!(
        matches!(request.parse(&raw), Err(ResponseError::Api(api)) if api.code == ErrorCode::MissingData)
    );

    for headers in [
        vec![],
        vec![("szlahu_szamlaszam", "I-2")],
        vec![("szlahu_id", "42")],
        vec![("szlahu_down", " \t"), ("szlahu_error_code", "")],
    ] {
        let raw = RawResponse::new(headers, body.to_vec()).with_status(500);
        let error = request.parse(&raw).expect_err("status before body");
        assert!(matches!(
            error,
            ResponseError::HttpStatus { status: 500, .. }
        ));
        let message = error.to_string();
        assert!(message.starts_with("HTTP 500 before body interpretation:"));
        #[cfg(feature = "client-reqwest")]
        assert_eq!(
            szamlazz_agent::ClientError::from(error).to_string(),
            message
        );
    }

    let raw = RawResponse::new(
        [("szlahu_error_code", "3"), ("szlahu_error", "login")],
        body.to_vec(),
    )
    .with_status(500);
    assert!(
        matches!(request.parse(&raw), Err(ResponseError::Api(api)) if api.code == ErrorCode::InvalidCredentials)
    );

    let raw = RawResponse::new(
        [
            ("szlahu_down", "maintenance+window"),
            ("szlahu_error_code", "56"),
            ("szlahu_szamlaszam", "I-2"),
        ],
        body.to_vec(),
    )
    .with_status(500);
    assert!(
        matches!(request.parse(&raw), Err(ResponseError::ServiceUnavailable(message)) if message == "maintenance window")
    );
}

#[test]
fn header_56_at_non_2xx_is_judged_by_the_operation() {
    for number in [None, Some("I-2")] {
        let mut headers = vec![("szlahu_error_code", "56")];
        if let Some(number) = number {
            headers.push(("szlahu_szamlaszam", number));
        }
        let raw = RawResponse::new(headers, b"notification failed".to_vec()).with_status(500);
        let issued = StornoInvoice::new("I-1").parse(&raw);
        if let Some(number) = number {
            let issued = issued.expect("numbered 56");
            assert_eq!(issued.invoice_number.as_str(), number);
            assert!(issued.notification_delivery_failed);
        } else {
            assert!(
                matches!(issued, Err(ResponseError::Api(api)) if api.code == ErrorCode::InvoiceNotificationDeliveryFailed)
            );
        }
        assert!(
            matches!(RegisterCreditEntry::new("I-1").parse(&raw), Err(ResponseError::Api(api)) if api.code == ErrorCode::InvoiceNotificationDeliveryFailed)
        );
    }
}

#[test]
fn textual_headers_decode_once_but_codes_numbers_and_xml_urls_stay_raw() {
    let raw = RawResponse::new(
        [
            ("SZLAHU_SZAMLASZAM", "I%2B1+suffix"),
            ("szlahu_vevoifiokurl", "https%3A%2F%2Fexample.test%2F%3Fq%3Da%252Bb%2Bc"),
            ("szlahu_nettovegosszeg", "+1.5"),
            ("szlahu_id", "42"),
        ],
        br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>"#.to_vec(),
    );
    assert_eq!(raw.header("szlahu_szamlaszam"), Some("I%2B1+suffix"));
    assert_eq!(
        raw.szlahu("szlahu_szamlaszam").as_deref(),
        Some("I+1 suffix")
    );
    let issued = StornoInvoice::new("I-1").parse(&raw).expect("headers");
    assert_eq!(issued.invoice_number.as_str(), "I+1 suffix");
    assert_eq!(issued.net_total, Some(dec!(1.5)));
    assert_eq!(issued.document_id, Some(42));
    assert_eq!(
        issued.customer_account_url.as_deref(),
        Some("https://example.test/?q=a%2Bb+c")
    );

    let raw = RawResponse::new(
        [("szlahu_error_code", "%33"), ("szlahu_error", "a%2Bb+c")],
        Vec::new(),
    );
    let error = raw.header_error().expect("raw code");
    assert_eq!(error.code, ErrorCode::Unknown("%33".into()));
    assert_eq!(error.message, "a+b c");

    let raw = response(Some("%2B1.5"), "");
    assert!(
        StornoInvoice::new("I-1").parse(&raw).is_err(),
        "numeric headers are not percent decoded"
    );
    let raw = response(
        None,
        "<vevoifiokurl>https://example.test/?q=a%2Bb+c&amp;x=%252B</vevoifiokurl>",
    );
    for url in [
        StornoInvoice::new("I-1")
            .parse(&raw)
            .expect("invoice")
            .customer_account_url,
        RegisterCreditEntry::new("I-1")
            .parse(&raw)
            .expect("balance")
            .customer_account_url,
    ] {
        assert_eq!(
            url.as_deref(),
            Some("https://example.test/?q=a%2Bb+c&x=%252B")
        );
    }
}

fn response(value: Option<&str>, inner: &str) -> RawResponse {
    let mut headers = vec![("szlahu_szamlaszam", " I-1 ")];
    if let Some(value) = value {
        for name in [
            "szlahu_nettovegosszeg",
            "szlahu_bruttovegosszeg",
            "szlahu_kintlevoseg",
        ] {
            headers.push((name, value));
        }
    }
    RawResponse::new(headers, format!(r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><pdf>JVBERi0=</pdf>{inner}</xmlszamlavalasz>"#).into_bytes())
}

#[test]
fn monetary_headers_are_ungrouped_decimals_across_operations() {
    for (value, expected) in [
        ("100,01", dec!(100.01)),
        ("1,234", dec!(1.234)),
        ("-0,5", dec!(-0.5)),
        ("+1.5", dec!(1.5)),
        ("0", dec!(0)),
        ("-0", dec!(0)),
        (".5", dec!(0.5)),
        ("1.", dec!(1)),
        ("1,25e+2", dec!(125)),
        ("1E-2", dec!(0.01)),
        (" \t1.2345\t ", dec!(1.2345)),
    ] {
        let raw = response(Some(value), "");
        let issued = StornoInvoice::new("I-1")
            .parse(&raw)
            .unwrap_or_else(|e| panic!("{value:?}: {e}"));
        assert_eq!(
            (issued.net_total, issued.gross_total, issued.outstanding),
            (Some(expected), Some(expected), Some(expected))
        );
        assert_eq!(issued.invoice_number.as_str(), "I-1");
        let balance = RegisterCreditEntry::new("I-1")
            .parse(&raw)
            .expect("balance");
        assert_eq!(
            (balance.net_total, balance.gross_total, balance.outstanding),
            (Some(expected), Some(expected), Some(expected))
        );
        let pdf = QueryInvoicePdf::new(InvoiceSelector::OrderNumber("O-1".into()))
            .parse(&raw)
            .expect("PDF");
        assert_eq!(
            (pdf.net_total, pdf.gross_total, pdf.outstanding),
            (Some(expected), Some(expected), Some(expected))
        );
    }
}

#[test]
fn pdf_balance_and_opaque_url_prefer_body_then_headers_and_default_to_none() {
    use szamlazz_agent::ops::query_pdf::InvoicePdf;
    let query = QueryInvoicePdf::new(InvoiceSelector::OrderNumber("O-1".into()));
    for (inner, headers, amount, url) in [
        ("", vec![], None, None),
        (
            "",
            vec![
                ("szlahu_kintlevoseg", "-1,5"),
                ("szlahu_vevoifiokurl", "opaque%3Aa%252Bb+c"),
            ],
            Some(dec!(-1.5)),
            Some("opaque:a%2Bb c"),
        ),
        (
            "<kintlevoseg>0</kintlevoseg><vevoifiokurl>opaque:a%2Bb+c&amp;x=1</vevoifiokurl>",
            vec![
                ("szlahu_kintlevoseg", "bad"),
                ("szlahu_vevoifiokurl", "wrong"),
            ],
            Some(dec!(0)),
            Some("opaque:a%2Bb+c&x=1"),
        ),
        (
            "<kintlevoseg>+2.5</kintlevoseg>",
            vec![("szlahu_kintlevoseg", "99")],
            Some(dec!(2.5)),
            None,
        ),
    ] {
        let body = format!(
            r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>I-1</szamlaszam><pdf>JVBERi0=</pdf>{inner}</xmlszamlavalasz>"#
        );
        let pdf = query
            .parse(&RawResponse::new(headers, body.into_bytes()))
            .expect("PDF");
        assert_eq!(pdf.outstanding, amount);
        assert_eq!(pdf.customer_account_url.as_deref(), url);
        assert_eq!(pdf.pdf.as_bytes(), b"%PDF-");
        let json = serde_json::to_value(&pdf).expect("JSON");
        assert_eq!(
            serde_json::from_value::<InvoicePdf>(json.clone()).expect("roundtrip"),
            pdf
        );
        assert!(json.get("payment_method").is_none());
        assert!(json.get("document_id").is_none());
        assert!(json.get("notification_delivery_failed").is_none());
    }
    let old: InvoicePdf = serde_json::from_str(
        r#"{"invoice_number":"I-1","pdf":"JVBERi0=","net_total":null,"gross_total":null}"#,
    )
    .expect("old JSON");
    assert_eq!((old.outstanding, old.customer_account_url), (None, None));
    for body in ["<pdf>JVBERi0=</pdf>", "<szamlaszam>I-1</szamlaszam>"] {
        let body = format!(
            r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres>{body}</xmlszamlavalasz>"#
        );
        assert!(
            query
                .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
                .is_err()
        );
    }
}

#[test]
fn malformed_headers_and_body_precedence() {
    let request = StornoInvoice::new("I-1");
    assert_eq!(
        request
            .parse(&response(None, ""))
            .expect("absent")
            .gross_total,
        None
    );
    for value in [
        "",
        " \t",
        "1_000",
        "1 000",
        "1,234.56",
        "1.234,56",
        "1,,2",
        "1..2",
        "1e",
        "1e999",
        "79228162514264337593543950336",
        "\n1",
        "1\u{a0}",
    ] {
        assert!(
            request.parse(&response(Some(value), "")).is_err(),
            "{value:?}"
        );
        let body = "<szamlanetto>2</szamlanetto><szamlabrutto>3</szamlabrutto><kintlevoseg>1</kintlevoseg>";
        assert_eq!(
            request
                .parse(&response(Some(value), body))
                .expect("body wins")
                .gross_total,
            Some(dec!(3))
        );
    }
    for body in ["<szamlabrutto/>", "<szamlabrutto> \t </szamlabrutto>"] {
        assert_eq!(
            request
                .parse(&response(Some("1,25"), body))
                .expect("header fallback")
                .gross_total,
            Some(dec!(1.25))
        );
    }
    assert!(
        request
            .parse(&response(Some("1,25"), "<szamlabrutto>bad</szamlabrutto>"))
            .is_err()
    );
    assert!(
        request
            .parse(&response(Some("1,25"), "<szamlabrutto>1,25</szamlabrutto>"))
            .is_err()
    );
}

#[test]
fn numbered_56_does_not_hide_a_body_refusal_behind_malformed_optional_payload() {
    for payload in [
        "<szamlabrutto><bad/></szamlabrutto>",
        "<szamlabrutto>1</szamlabrutto><szamlabrutto>2</szamlabrutto>",
    ] {
        let raw = RawResponse::new(
            [("szlahu_error_code", "56"), ("szlahu_szamlaszam", "I-2")],
            format!(
                r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>3</hibakod><hibauzenet>login refused</hibauzenet>{payload}</xmlszamlavalasz>"#
            ).into_bytes(),
        );
        let error = StornoInvoice::new("I-1")
            .parse(&raw)
            .expect_err("body refusal");
        assert!(matches!(error, szamlazz_agent::ResponseError::Api(ref api)
            if api.code == szamlazz_agent::ErrorCode::InvalidCredentials
                && api.message == "login refused"));
    }
}

#[test]
fn numbered_header_56_cannot_promote_a_malformed_xml_envelope() {
    for extension in [
        "<extension>&unknown;</extension>",
        "<extension a='1'b='2'/>",
        "<extension>A]]>B</extension>",
    ] {
        let body = format!(
            r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>3</hibakod>{extension}</xmlszamlavalasz>"#
        );
        let raw = RawResponse::new(
            [("szlahu_error_code", "56"), ("szlahu_szamlaszam", "I-2")],
            body.into_bytes(),
        );
        let error = StornoInvoice::new("I-1")
            .parse(&raw)
            .expect_err("malformed envelope is not issuance evidence");
        assert_eq!(error.outcome_class(), szamlazz_agent::OutcomeClass::Unknown);
    }
}

#[test]
fn numbered_56_preserves_unique_body_identity_despite_bad_optional_structure() {
    for payload in [
        "<szamlabrutto><bad/></szamlabrutto>",
        "<pdf>JVBERi0=</pdf><pdf>JVBERi0=</pdf>",
    ] {
        for headers in [vec![], vec![("szlahu_error_code", "56")]] {
            let raw = RawResponse::new(headers, format!(
                r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>56</hibakod><szamlaszam>I-2</szamlaszam>{payload}</xmlszamlavalasz>"#
            ).into_bytes());
            let issued = StornoInvoice::new("I-1")
                .parse(&raw)
                .expect("numbered warning");
            assert_eq!(issued.invoice_number.as_str(), "I-2");
            assert!(issued.notification_delivery_failed);
            assert_eq!(issued.gross_total, None);
            assert_eq!(issued.pdf, None);
        }
    }
    for identity in [
        "<szamlaszam>I-2</szamlaszam><szamlaszam>I-3</szamlaszam>",
        "<szamlaszam><bad/></szamlaszam>",
        "<szamlaszam xmlns='urn:foreign'>I-2</szamlaszam>",
    ] {
        let raw = RawResponse::new::<&str, &str>([], format!(
            r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>56</hibakod>{identity}<szamlabrutto><bad/></szamlabrutto></xmlszamlavalasz>"#
        ).into_bytes());
        assert!(StornoInvoice::new("I-1").parse(&raw).is_err(), "{identity}");
    }
}

#[test]
fn numbered_56_retains_comma_metadata_and_drops_malformed_metadata() {
    let raw = RawResponse::new(
        [
            ("szlahu_error_code", "56"),
            ("szlahu_szamlaszam", "I-1"),
            ("szlahu_nettovegosszeg", "100,01"),
            ("szlahu_bruttovegosszeg", "bad"),
            ("szlahu_kintlevoseg", "0,01"),
        ],
        b"notification failed".to_vec(),
    );
    let issued = StornoInvoice::new("I-1").parse(&raw).expect("issued");
    assert!(issued.notification_delivery_failed);
    assert_eq!(
        (issued.net_total, issued.gross_total, issued.outstanding),
        (Some(dec!(100.01)), None, Some(dec!(0.01)))
    );
}
