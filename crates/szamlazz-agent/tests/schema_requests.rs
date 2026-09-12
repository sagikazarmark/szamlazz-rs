//! Synthetic generated requests for the required workspace XSD check.
//! Run `cargo xtask check-agent-schemas`; this ignored test only exports
//! inputs, it does not claim XSD validation. No vendor credentials or client.

use jiff::civil::date;
use rust_decimal::dec;
use serde_json::{Value, json};
use szamlazz_agent::ops::credit_entry::{ClearCreditEntries, CreditEntry, RegisterCreditEntry};
use szamlazz_agent::ops::invoice::{
    Buyer, BuyerLedger, CreateInvoice, InvoiceHeader, InvoiceKind, Mpl, PickPackPoint,
    PostalAddress, Seller, Sprinter, TransOFlex, Waybill,
};
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
use szamlazz_agent::ops::query_pdf::QueryInvoicePdf;
use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
use szamlazz_agent::ops::receipt::{
    CreateReceipt, QueryReceipt, ReceiptEmail, ReceiptPayment, ReceiptSelector, ReceiptTemplate,
    SendReceipt, StornoReceipt,
};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::wire::AgentRequest;
use szamlazz_agent::{
    Credentials, Currency, ExchangeRate, InvoiceSelector, InvoiceTemplate, Language, LineItem,
    LineItemLedger, PaymentMethod, SellerEmail,
};

#[derive(Default)]
struct Matrix(Vec<Value>);

impl Matrix {
    fn add<R: AgentRequest + serde::Serialize>(&mut self, root: &str, name: &str, request: &R) {
        // Expectations derive from requested options, not observed XML: an
        // accidentally omitted supported field must change the verdict.
        let input = serde_json::to_value(request).expect("serialize request options");
        let mut inline_errors = Vec::new();
        let mut download_errors = Vec::new();
        if root == "xmlszamla" {
            if !input["header"]["preview_pdf"].is_null()
                && !input["header"]["simple_items"].is_null()
            {
                inline_errors.push("simpleItems");
            }
            if !input["buyer"]["group_id"].is_null() {
                download_errors.push("csoportazonosito");
            }
        }
        if root == "xmlszamla" || root == "xmlnyugtacreate" {
            for item in input["items"].as_array().expect("create request items") {
                if !item["erasure_code_count"].is_null() {
                    download_errors.push("torloKod");
                }
            }
        }
        if root == "xmlnyugtaget" && input["selector"].get("order_number").is_some() {
            download_errors.push("rendelesSzam");
        }
        for (auth, credentials) in [
            ("key", Credentials::agent_key("dummy-schema-key<&")),
            (
                "password",
                Credentials::user_password("dummy<&", "password<&"),
            ),
        ] {
            // Validate the exact first multipart file emitted by the public
            // checked boundary, including for cases with email attachments.
            let wire = request
                .to_wire(&credentials)
                .expect("valid synthetic request");
            let boundary = wire
                .content_type
                .split("boundary=")
                .nth(1)
                .expect("multipart boundary");
            let start = wire
                .body
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .expect("XML part headers")
                + 4;
            let delimiter = format!("\r\n--{boundary}");
            let end = wire.body[start..]
                .windows(delimiter.len())
                .position(|w| w == delimiter.as_bytes())
                .expect("XML part boundary")
                + start;
            let xml = std::str::from_utf8(&wire.body[start..end]).expect("UTF-8 request XML");
            self.0
                .push(json!({"root": root, "name": name, "auth": auth, "xml": xml,
                "errors": {"en-inline": inline_errors, "download": download_errors}}));
        }
    }

    // The same explicit populated data in isolation and together catches
    // optional-field ordering both with and without intervening siblings.
    // Only top-level Option fields are removed here; nested leaves are covered
    // by the full blocks, empty blocks, and the explicit combinations below.
    fn options<R>(&mut self, root: &str, minimal: &R, full: &R)
    where
        R: AgentRequest + serde::Serialize + serde::de::DeserializeOwned,
    {
        let base = serde_json::to_value(minimal).expect("minimal request JSON");
        let populated = serde_json::to_value(full).expect("populated request JSON");
        for (key, value) in populated.as_object().expect("request object") {
            if base[key].is_null() && !value.is_null() {
                let mut one = base.clone();
                one[key] = value.clone();
                self.add(
                    root,
                    &format!("only-{key}"),
                    &serde_json::from_value::<R>(one).expect("single-option request"),
                );
            }
        }
    }
}

#[expect(clippy::unnecessary_wraps, reason = "populated optional text fixture")]
fn text() -> Option<String> {
    Some("Árvíz <&> \" ' \r\ntext".into())
}

fn item() -> LineItem {
    LineItem::new(
        "Áru <&>",
        dec!(1.25),
        "db",
        dec!(80),
        szamlazz_agent::VatRate::percent(27),
        dec!(100),
        dec!(27),
        dec!(127),
    )
}

fn invoice() -> CreateInvoice {
    CreateInvoice::new(
        InvoiceKind::invoice(),
        InvoiceHeader::new(
            date(2024, 2, 29),
            date(2024, 3, 8),
            PaymentMethod::Transfer,
            Currency::HUF,
            Language::Hungarian,
        ),
        Buyer::new("Vevő <&>", "0123", "Budapest", "Utca 1."),
        vec![item()],
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "complete source-derived invoice field inventory"
)]
fn full_invoice() -> CreateInvoice {
    let mut r = invoice();
    r.e_invoice = true;
    r.download_pdf = true;
    r.download_copies = Some(2);
    r.aggregator = text();
    r.guardian = Some(true);
    r.item_identifiers_on_invoice = Some(true);
    r.external_id = text();
    r.header = InvoiceHeader {
        issue_date: Some(date(2024, 2, 29)),
        comment: text(),
        exchange_rate: Some(ExchangeRate::new("MNB", dec!(390.125))),
        order_number: text(),
        extra_logo: text(),
        number_prefix: text(),
        payable_adjustment: Some(dec!(-0.5)),
        paid: Some(true),
        margin_vat: Some(true),
        eu_vat: Some(true),
        template: Some(InvoiceTemplate::Default),
        preview_pdf: Some(true),
        simple_items: Some(true),
        ..r.header
    };
    r.seller = Seller {
        bank: text(),
        bank_account: text(),
        signer_name: text(),
        email: Some(SellerEmail {
            reply_to: text(),
            subject: text(),
            body: text(),
        }),
    };
    r.buyer = Buyer {
        country: text(),
        email: text(),
        send_email: Some(true),
        taxpayer_status: Some(szamlazz_agent::TaxpayerStatus::HasTaxNumber),
        tax_number: text(),
        group_id: text(),
        eu_tax_number: text(),
        postal_address: Some(PostalAddress {
            name: text(),
            country: text(),
            zip: text(),
            city: text(),
            address: text(),
        }),
        ledger: Some(BuyerLedger {
            accounting_date: Some(date(2024, 2, 29)),
            buyer_id: text(),
            buyer_account: text(),
            continuous_fulfillment: Some(true),
            settlement_from: Some(date(2024, 2, 1)),
            settlement_to: Some(date(2024, 2, 29)),
        }),
        id: text(),
        signer_name: text(),
        phone: text(),
        comment: text(),
        ..r.buyer
    };
    r.items[0] = LineItem {
        id: text(),
        margin_vat_base: Some(dec!(12.5)),
        comment: text(),
        erasure_code_count: Some(400),
        ledger: Some(LineItemLedger {
            economic_event: text(),
            vat_economic_event: text(),
            revenue_account: text(),
            vat_account: text(),
            settlement_from: Some(date(2024, 2, 1)),
            settlement_to: Some(date(2024, 2, 29)),
        }),
        ..item()
    };
    r.waybill = Some(Waybill {
        destination: text(),
        carrier: text(),
        barcode: text(),
        comment: text(),
        trans_o_flex: Some(TransOFlex {
            id: text(),
            shipment_id: text(),
            parcel_count: Some(2),
            country_code: text(),
            zip: text(),
            service: text(),
        }),
        pick_pack_point: Some(PickPackPoint {
            barcode_prefix: text(),
            barcode_suffix: text(),
        }),
        sprinter: Some(Sprinter {
            id: text(),
            sender_code: text(),
            routing_code: text(),
            parcel_count: Some(3),
            barcode_suffix: text(),
            delivery_time: text(),
        }),
        mpl: Some(Mpl {
            extra_services: text(),
            declared_value: Some(dec!(127)),
            ..Mpl::new("0123", "barcode", "1.25")
        }),
    });
    r
}

#[expect(
    clippy::too_many_lines,
    reason = "invoice matrix kept together for coverage review"
)]
fn invoices(m: &mut Matrix) {
    let root = "xmlszamla";
    let base = invoice();
    let full = full_invoice();
    m.add(root, "minimal", &base);
    m.add(root, "all-options", &full);
    m.options(root, &base, &full);
    // Source-conflicting fields are independently exercised, never removed
    // from a schema. These passing controls expose errors masked by an XSD
    // validator stopping at an unsupported child in the all-options case.
    let mut common = full.clone();
    common.buyer.group_id = None;
    common.items[0].erasure_code_count = None;
    common.header.simple_items = None;
    m.add(root, "all-common-options", &common);
    let mut inline = full.clone();
    inline.header.preview_pdf = None;
    m.add(root, "all-inline-options", &inline);
    for (name, value) in [
        ("header", json!(common.header)),
        ("seller", json!(full.seller)),
        ("buyer", json!(full.buyer)),
        ("items", json!(full.items)),
        ("waybill", json!(full.waybill)),
    ] {
        let mut one = json!(base);
        one[name] = value;
        m.add(
            root,
            &format!("block-{name}"),
            &serde_json::from_value::<CreateInvoice>(one).expect("single-block invoice"),
        );
    }
    for preview in [None, Some(false), Some(true)] {
        for simple in [None, Some(false), Some(true)] {
            let mut r = base.clone();
            r.header.preview_pdf = preview;
            r.header.simple_items = simple;
            r.header.template.clone_from(&full.header.template);
            m.add(root, &format!("tail-{preview:?}-{simple:?}"), &r);
        }
    }
    for (name, kind) in [
        (
            "invoice-link",
            InvoiceKind::Invoice {
                proforma_number: Some("D-1".into()),
            },
        ),
        ("proforma", InvoiceKind::Proforma),
        ("delivery-note", InvoiceKind::DeliveryNote),
        ("prepayment", InvoiceKind::prepayment()),
        (
            "prepayment-link",
            InvoiceKind::Prepayment {
                proforma_number: Some("D-1".into()),
            },
        ),
        (
            "final-links",
            InvoiceKind::Final {
                prepayment_number: Some("ES-1".into()),
                proforma_number: Some("D-1".into()),
            },
        ),
        (
            "final-order",
            InvoiceKind::Final {
                prepayment_number: None,
                proforma_number: None,
            },
        ),
        (
            "corrective",
            InvoiceKind::Corrective {
                corrected_number: "I-1".into(),
            },
        ),
    ] {
        let mut r = common.clone();
        r.kind = kind;
        m.add(root, name, &r);
    }
    let mut r = common.clone();
    r.guardian = Some(false);
    r.item_identifiers_on_invoice = Some(false);
    r.buyer.send_email = Some(false);
    r.buyer
        .ledger
        .as_mut()
        .expect("full buyer ledger")
        .continuous_fulfillment = Some(false);
    r.header.margin_vat = Some(false);
    r.header.eu_vat = Some(false);
    r.header.preview_pdf = Some(false);
    r.header.paid = Some(false);
    m.add(root, "explicit-false-options", &r);
    r = base.clone();
    r.buyer.ledger = Some(BuyerLedger::default());
    r.buyer.postal_address = Some(PostalAddress::default());
    r.seller.email = Some(SellerEmail::default());
    r.items[0].ledger = Some(LineItemLedger::default());
    r.waybill = Some(Waybill {
        trans_o_flex: Some(TransOFlex::default()),
        pick_pack_point: Some(PickPackPoint::default()),
        sprinter: Some(Sprinter::default()),
        ..Waybill::default()
    });
    m.add(root, "empty-optional-containers", &r);
    for erasure in [0, 400] {
        let mut r = base.clone();
        r.items[0].erasure_code_count = Some(erasure);
        m.add(root, &format!("erasure-{erasure}"), &r);
    }
    r = base.clone();
    r.buyer.group_id = text();
    m.add(root, "buyer-group", &r);
    for automatic in [false, true] {
        r = common.clone();
        r.header.currency = Currency::EUR;
        if automatic {
            r.header.exchange_rate = Some(ExchangeRate::automatic_mnb());
        }
        m.add(root, &format!("eur-automatic-{automatic}"), &r);
    }
    r = common;
    r.items.push(LineItem::new(
        "Deduction",
        dec!(1),
        "db",
        dec!(-50),
        szamlazz_agent::VatRate::percent(27),
        dec!(-50),
        dec!(-13.5),
        dec!(-63.5),
    ));
    for n in 1..=5 {
        r.attachments
            .push(szamlazz_agent::ops::invoice::EmailAttachment::new(
                format!("attachment-{n}.txt"),
                b"synthetic attachment".to_vec(),
                "text/plain",
            ))
            .expect("at most five small attachments");
    }
    m.add(root, "multiple-items-and-five-attachments", &r);
}

fn mutations(m: &mut Matrix) {
    let base = StornoInvoice::new("I-1<&");
    m.add("xmlszamlast", "minimal", &base);
    let full = StornoInvoice {
        e_invoice: true,
        download_pdf: true,
        download_copies: Some(2),
        aggregator: text(),
        guardian: Some(true),
        external_id: text(),
        issue_date: Some(date(2024, 2, 29)),
        fulfillment_date: Some(date(2024, 2, 1)),
        comment: text(),
        template: Some(InvoiceTemplate::Default),
        seller_email: Some(SellerEmail {
            reply_to: text(),
            subject: text(),
            body: text(),
        }),
        buyer_email: text(),
        buyer_tax_number: text(),
        buyer_eu_tax_number: text(),
        ..base.clone()
    };
    m.add("xmlszamlast", "all-options", &full);
    m.options("xmlszamlast", &base, &full);
    m.add(
        "xmlszamlast",
        "guardian-false-empty-email",
        &StornoInvoice {
            guardian: Some(false),
            seller_email: Some(SellerEmail::default()),
            ..base
        },
    );
    for additive in [false, true] {
        for count in [1, 5] {
            let mut r = RegisterCreditEntry::new("I-1<&");
            r.additive = additive;
            if count == 5 {
                r.aggregator = text();
                r.issuer_tax_number = text();
            }
            r.entries = (0..count)
                .map(|n| CreditEntry {
                    description: if n % 2 == 0 { text() } else { None },
                    ..CreditEntry::new(date(2024, 2, 29), PaymentMethod::Transfer, dec!(12.5))
                })
                .collect::<Vec<_>>()
                .try_into()
                .expect("one or five credit entries");
            m.add(
                "xmlszamlakifiz",
                &format!("additive-{additive}-entries-{count}"),
                &r,
            );
        }
    }
    m.add(
        "xmlszamlakifiz",
        "empty-additive",
        &RegisterCreditEntry {
            additive: true,
            ..RegisterCreditEntry::new("I-1")
        },
    );
    m.add(
        "xmlszamlakifiz",
        "clear-minimal",
        &ClearCreditEntries::new("I-1"),
    );
    m.add(
        "xmlszamlakifiz",
        "clear-options",
        &ClearCreditEntries {
            issuer_tax_number: text(),
            aggregator: text(),
            ..ClearCreditEntries::new("I-1")
        },
    );
    for (name, selector) in [
        ("number", ProformaSelector::InvoiceNumber("D-1<&".into())),
        ("order", ProformaSelector::OrderNumber("O-1<&".into())),
    ] {
        m.add("xmlszamladbkdel", name, &DeleteProforma::new(selector));
    }
}

fn invoice_tokens(m: &mut Matrix) {
    for token in [
        "hu", "en", "de", "it", "ro", "sk", "hr", "fr", "es", "cz", "pl", "bg", "nl", "ru", "si",
    ] {
        let mut r = invoice();
        r.header.language = token.parse().expect("published language token");
        m.add("xmlszamla", &format!("language-{token}"), &r);
    }
    for template in [
        InvoiceTemplate::Most,
        InvoiceTemplate::Default,
        InvoiceTemplate::NoEnvelope,
        InvoiceTemplate::EightCentimeter,
        InvoiceTemplate::Continuous,
        InvoiceTemplate::DeliveryNote,
        InvoiceTemplate::Other("future<&".into()),
    ] {
        let mut r = invoice();
        let name = format!("template-{}", template.as_wire());
        r.header.template = Some(template);
        m.add("xmlszamla", &name, &r);
    }
    for token in ["7", "6", "1", "0", "-1"] {
        let mut r = invoice();
        r.buyer.taxpayer_status = Some(token.parse().expect("published taxpayer status"));
        m.add("xmlszamla", &format!("taxpayer-status-{token}"), &r);
    }
}

fn queries(m: &mut Matrix) {
    for (name, selector) in [
        ("number", InvoiceSelector::InvoiceNumber("I-1<&".into())),
        ("order", InvoiceSelector::OrderNumber("O-1<&".into())),
        ("external", InvoiceSelector::ExternalId("X-1<&".into())),
    ] {
        m.add(
            "xmlszamlapdf",
            name,
            &QueryInvoicePdf::new(selector.clone()),
        );
        for include_pdf in [false, true] {
            m.add(
                "xmlszamlaxml",
                &format!("{name}-pdf-{include_pdf}"),
                &QueryInvoiceXml {
                    include_pdf,
                    ..QueryInvoiceXml::new(selector.clone())
                },
            );
        }
    }
    for prefix in ["01234567", "12345678"] {
        m.add(
            "xmltaxpayer",
            prefix,
            &QueryTaxpayer::new(prefix).expect("eight-digit stem"),
        );
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "receipt operation matrices and shared templates"
)]
fn receipts(m: &mut Matrix) {
    let base = CreateReceipt::new("NY", PaymentMethod::Cash, Currency::HUF, vec![item()]);
    m.add("xmlnyugtacreate", "minimal", &base);
    let mut full = CreateReceipt {
        download_pdf: true,
        call_id: text(),
        exchange_rate: Some(ExchangeRate::new("MNB", dec!(390.125))),
        comment: text(),
        template: Some(ReceiptTemplate::TicketWithLogo),
        ledger_customer: text(),
        order_number: text(),
        ..base.clone()
    };
    full.items[0].id = text();
    full.items[0].comment = text();
    full.items[0].erasure_code_count = Some(400);
    full.items[0].ledger = Some(LineItemLedger {
        revenue_account: text(),
        vat_account: text(),
        ..LineItemLedger::default()
    });
    let mut payment = ReceiptPayment::new("cash<&", dec!(100));
    payment.description = text();
    full.payments = vec![payment, ReceiptPayment::new("card", dec!(27))];
    m.add("xmlnyugtacreate", "all-options", &full);
    m.options("xmlnyugtacreate", &base, &full);
    full.items[0].erasure_code_count = None;
    m.add("xmlnyugtacreate", "all-common-options", &full);
    full.items.push(item());
    m.add("xmlnyugtacreate", "multiple-items", &full);
    for erasure in [0, 400] {
        let mut r = base.clone();
        r.items[0].erasure_code_count = Some(erasure);
        m.add("xmlnyugtacreate", &format!("erasure-{erasure}"), &r);
    }
    let mut r = base.clone();
    r.items[0].ledger = Some(LineItemLedger::default());
    m.add("xmlnyugtacreate", "empty-ledger", &r);
    for automatic in [false, true] {
        r = full.clone();
        r.currency = Currency::EUR;
        if automatic {
            r.exchange_rate = Some(ExchangeRate::automatic_mnb());
        }
        m.add("xmlnyugtacreate", &format!("eur-automatic-{automatic}"), &r);
    }
    let templates = [
        None,
        Some(ReceiptTemplate::A4Default),
        Some(ReceiptTemplate::Ticket),
        Some(ReceiptTemplate::TicketWithLogo),
        Some(ReceiptTemplate::Roll80mm),
    ];
    for (index, template) in templates.iter().enumerate() {
        let mut r = base.clone();
        r.template.clone_from(template);
        m.add("xmlnyugtacreate", &format!("template-{index}"), &r);
        for download_pdf in [false, true] {
            for call_id in [None, text()] {
                let name = format!(
                    "template-{index}-pdf-{download_pdf}-call-{}",
                    call_id.is_some()
                );
                m.add(
                    "xmlnyugtast",
                    &name,
                    &StornoReceipt {
                        download_pdf,
                        template: template.clone(),
                        call_id: call_id.clone(),
                        ..StornoReceipt::new("NY-1<&")
                    },
                );
                for (selector_name, selector) in [
                    ("number", ReceiptSelector::ReceiptNumber("NY-1<&".into())),
                    ("order", ReceiptSelector::OrderNumber("O-1<&".into())),
                ] {
                    m.add(
                        "xmlnyugtaget",
                        &format!("{selector_name}-{name}"),
                        &QueryReceipt {
                            download_pdf,
                            template: template.clone(),
                            call_id: call_id.clone(),
                            ..QueryReceipt::new(selector)
                        },
                    );
                }
            }
        }
    }
    m.add("xmlnyugtasend", "resend-none", &SendReceipt::new("NY-1"));
    // All presence combinations, and present-empty strings separately: XSD
    // validation must see empty containers and strings, unlike an outline.
    for empty in [false, true] {
        for mask in 0..16 {
            let value = |bit| {
                if mask & (1 << bit) == 0 {
                    None
                } else if empty {
                    Some(String::new())
                } else {
                    text()
                }
            };
            m.add(
                "xmlnyugtasend",
                &format!("email-mask-{mask}-empty-{empty}"),
                &SendReceipt {
                    email: Some(ReceiptEmail {
                        to: value(0),
                        reply_to: value(1),
                        subject: value(2),
                        body: value(3),
                    }),
                    ..SendReceipt::new("NY-1<&")
                },
            );
        }
    }
}

#[test]
#[ignore = "exports inputs only; run cargo xtask check-agent-schemas for XSD validation"]
fn emit_request_matrix() {
    let output = std::env::var_os("SZAMLAZZ_SCHEMA_OUTPUT")
        .expect("run cargo xtask check-agent-schemas (required output path)");
    let mut m = Matrix::default();
    invoices(&mut m);
    invoice_tokens(&mut m);
    mutations(&mut m);
    queries(&mut m);
    receipts(&mut m);
    std::fs::write(output, serde_json::to_vec(&m.0).expect("serialize matrix"))
        .expect("write generated matrix");
}
