//! The upstream corpus (szamlazz.hu's own request and response examples,
//! `fixtures/upstream/agent/` reached through the `tests/upstream` symlink)
//! read at run time. The corpus is not redistributed with the crate
//! (`fixtures/SOURCES.md`; `exclude = ["tests/upstream"]` in `Cargo.toml`), so
//! a package built from crates.io has nothing to read here and every test
//! declares itself skipped rather than failing.
//!
//! Feature-free, so it runs in every configuration.

use rust_decimal::dec;
use szamlazz_agent::Credentials;
use szamlazz_agent::wire::{AgentRequest, RawResponse};

/// The corpus on disk, and the runner that walks one of its directories.
mod corpus {
    use std::path::{Path, PathBuf};

    /// Where the `tests/upstream` symlink points: `fixtures/upstream/agent`.
    const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/upstream");

    /// The files of one corpus directory in name order, or `None` (after a
    /// skip message) when the corpus is absent: in a package built from
    /// crates.io the symlink is excluded, so the directory does not exist.
    ///
    /// Only that absence is a skip; any other failure to read is a failure.
    fn files(subdir: &str) -> Option<Vec<(String, Vec<u8>)>> {
        let dir: PathBuf = Path::new(ROOT).join(subdir);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "skipped: {} is not part of the published package ({error})",
                    dir.display()
                );
                return None;
            }
            Err(error) => panic!("reading {}: {error}", dir.display()),
        };
        let mut files: Vec<(String, Vec<u8>)> = entries
            .map(|entry| {
                let entry = entry.expect("directory entry");
                let name = entry
                    .file_name()
                    .into_string()
                    .expect("UTF-8 fixture file name");
                let body = std::fs::read(entry.path())
                    .unwrap_or_else(|error| panic!("reading {}: {error}", entry.path().display()));

                (name, body)
            })
            .collect();
        files.sort_by(|left, right| left.0.cmp(&right.0));

        Some(files)
    }

    /// A fixture's check: given the file's bytes, panics unless the crate
    /// treats them as the corpus says it should.
    pub type Check = fn(&[u8]);

    /// Runs every file of a corpus directory through its check, and fails
    /// when a file has no check, a check has no file, or a check fails:
    /// every failing file is named, so one run reports the whole directory.
    /// Skips (with a message) when the corpus is absent.
    #[track_caller]
    pub fn run(subdir: &str, table: &[(&str, Check)]) {
        let Some(files) = files(subdir) else {
            return;
        };
        assert!(!files.is_empty(), "{subdir}: the corpus directory is empty");

        let mut failures = Vec::new();
        let mut checked: Vec<&str> = Vec::new();

        for (name, body) in &files {
            let Some((_, check)) = table.iter().find(|(covered, _)| covered == name) else {
                failures.push(format!("{subdir}/{name}: no check in the table"));
                continue;
            };
            checked.push(name);
            eprintln!("checking {subdir}/{name}");

            if std::panic::catch_unwind(|| check(body)).is_err() {
                failures.push(format!("{subdir}/{name}: check failed (panic above)"));
            }
        }
        for (name, _) in table {
            if !checked.contains(name) {
                failures.push(format!("{subdir}/{name}: in the table, not in the corpus"));
            }
        }

        assert!(
            failures.is_empty(),
            "upstream {subdir} corpus:\n  {}",
            failures.join("\n  ")
        );
    }
}

/// A response body as szamlazz.hu would have delivered it: HTTP 200, no
/// `szlahu_*` header; every example in the corpus is a body alone.
fn delivered(body: &[u8]) -> RawResponse {
    RawResponse::new::<&str, &str>([], body.to_vec()).with_status(200)
}

/// The one email address the examples carry. The docs site masks addresses
/// as `[email protected]`, with a no-break space (U+00A0) between the words,
/// and the corpus keeps the page's text verbatim, so that is the value a
/// request example sends and a response example returns.
const MASKED_EMAIL: &str = "[email\u{a0}protected]";

/// The official request examples, built with the crate's request types, value
/// for value. Each is what the request-side test serialises, and the request a
/// response example is parsed against.
mod examples {
    use jiff::civil::date;
    use szamlazz_agent::ops::credit_entry::{CreditEntries, CreditEntry, RegisterCreditEntry};
    use szamlazz_agent::ops::invoice::{
        Buyer, CreateInvoice, InvoiceHeader, InvoiceKind, PostalAddress, Seller,
    };
    use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
    use szamlazz_agent::ops::query_pdf::QueryInvoicePdf;
    use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
    use szamlazz_agent::ops::receipt::{
        CreateReceipt, QueryReceipt, ReceiptEmail, ReceiptPayment, ReceiptSelector, SendReceipt,
        StornoReceipt,
    };
    use szamlazz_agent::ops::storno::StornoInvoice;
    use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
    use szamlazz_agent::{
        Currency, ExchangeRate, InvoiceNumber, InvoiceSelector, Language, LineItem, LineItemLedger,
        PaymentMethod, SellerEmail, VatRate,
    };

    use super::*;

    /// `requests/xmlszamla.xml`: an e-invoice with its PDF requested, two
    /// rows, a bank, a notification email, a buyer with a postal address.
    pub fn create_invoice() -> CreateInvoice {
        let header = InvoiceHeader {
            issue_date: Some(date(2020, 1, 20)),
            comment: Some("Invoce comment".to_owned()),
            exchange_rate: Some(ExchangeRate::new("MNB", dec!(0.0))),
            ..InvoiceHeader::new(
                date(2020, 1, 20),
                date(2020, 1, 20),
                PaymentMethod::Transfer,
                Currency::HUF,
                Language::Hungarian,
            )
        };
        let buyer = Buyer {
            email: Some(MASKED_EMAIL.to_owned()),
            send_email: Some(false),
            tax_number: Some("12345678-1-42".to_owned()),
            postal_address: Some(PostalAddress {
                name: Some("Kovács Bt. mailing name".to_owned()),
                country: None,
                zip: Some("2040".to_owned()),
                city: Some("Budaörs".to_owned()),
                address: Some("Szivárvány utca 8.".to_owned()),
            }),
            id: Some("1234".to_owned()),
            phone: Some("Tel:+3630-555-55-55, Fax:+3623-555-555".to_owned()),
            comment: Some("Call extension 214 from the reception".to_owned()),
            ..Buyer::new("Kovacs Bt.", "2030", "Érd", "Tárnoki út 23.")
        };
        let items = vec![
            LineItem {
                comment: Some("lorem ipsum".to_owned()),
                erasure_code_count: Some(123),
                ..LineItem::new(
                    "Elado izé",
                    dec!(1.0),
                    "db",
                    dec!(10000),
                    VatRate::percent(27),
                    dec!(10000.0),
                    dec!(2700.0),
                    dec!(12700.0),
                )
            },
            LineItem {
                comment: Some("lorem ipsum 2".to_owned()),
                ..LineItem::new(
                    "Elado izé 2",
                    dec!(2.0),
                    "db",
                    dec!(10000),
                    VatRate::percent(27),
                    dec!(20000.0),
                    dec!(5400.0),
                    dec!(25400.0),
                )
            },
        ];

        CreateInvoice {
            e_invoice: true,
            download_pdf: true,
            seller: Seller {
                bank: Some("BB".to_owned()),
                bank_account: Some("11111111-22222222-33333333".to_owned()),
                email: Some(SellerEmail {
                    reply_to: None,
                    subject: Some("Invoice notification".to_owned()),
                    body: Some("mail text".to_owned()),
                }),
                signer_name: None,
            },
            ..CreateInvoice::new(InvoiceKind::invoice(), header, buyer, items)
        }
    }

    /// `requests/xmlszamlast.xml`: a storno with its PDF (one copy) and the
    /// notification email's addresses.
    pub fn storno_invoice() -> StornoInvoice {
        StornoInvoice {
            download_pdf: true,
            download_copies: Some(1),
            issue_date: Some(date(2010, 9, 12)),
            seller_email: Some(SellerEmail {
                reply_to: Some(MASKED_EMAIL.to_owned()),
                subject: Some("Email subject".to_owned()),
                body: Some("Lorem ipsum".to_owned()),
            }),
            buyer_email: Some(MASKED_EMAIL.to_owned()),
            ..StornoInvoice::new("E-TST-2011-1")
        }
    }

    /// `requests/xmlszamlakifiz.xml`: two credit entries replacing the
    /// invoice's, with the issuer's tax number.
    pub fn register_credit_entry() -> RegisterCreditEntry {
        RegisterCreditEntry {
            issuer_tax_number: Some("12345678-1-13".to_owned()),
            entries: CreditEntries::try_from(vec![
                CreditEntry::new(date(2012, 1, 1), PaymentMethod::Cash, dec!(1000)),
                CreditEntry {
                    description: Some("Test description".to_owned()),
                    ..CreditEntry::new(date(2012, 1, 15), PaymentMethod::Transfer, dec!(2000))
                },
            ])
            .expect("two entries"),
            ..RegisterCreditEntry::new("E-TST-2011-1")
        }
    }

    /// `requests/xmlszamlapdf.xml`: the PDF of an invoice by number.
    pub fn query_invoice_pdf() -> QueryInvoicePdf {
        QueryInvoicePdf::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(
            "E-TST-2011-1",
        )))
    }

    /// `requests/xmlszamlaxml.xml`: the data of an invoice by number, PDF
    /// included.
    pub fn query_invoice_xml() -> QueryInvoiceXml {
        QueryInvoiceXml {
            include_pdf: true,
            ..QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(
                "E-TST-2011-1",
            )))
        }
    }

    /// `requests/xmlszamladbkdel.xml`: a proforma deleted by number.
    pub fn delete_proforma() -> DeleteProforma {
        DeleteProforma::new(ProformaSelector::InvoiceNumber(InvoiceNumber::new("D-43")))
    }

    /// `requests/xmlszamladbkdel_ordernumber.xml`: a proforma deleted by
    /// order number.
    pub fn delete_proforma_by_order_number() -> DeleteProforma {
        DeleteProforma::new(ProformaSelector::OrderNumber("XXX".to_owned()))
    }

    /// `requests/xmltaxpayer.xml`.
    pub fn query_taxpayer() -> QueryTaxpayer {
        QueryTaxpayer::new("12345678").expect("eight digits")
    }

    /// `requests/xmlnyugtacreate.xml`: a cash receipt in forints (`Ft`) with
    /// two rows (one under a special VAT code) and two payments.
    pub fn create_receipt() -> CreateReceipt {
        // `ReceiptPayment` is read back in responses too, so it stays
        // `#[non_exhaustive]` and is built through its constructor.
        let mut voucher = ReceiptPayment::new("voucher", dec!(30000.0));
        voucher.description = Some("OTP SZÉP kártya".to_owned());

        CreateReceipt {
            exchange_rate: Some(ExchangeRate::new("MNB", dec!(0.0))),
            payments: vec![voucher, ReceiptPayment::new("debit card", dec!(20800.0))],
            ..CreateReceipt::new(
                "NYGTA",
                PaymentMethod::Cash,
                Currency::new("Ft"),
                vec![
                    LineItem {
                        ledger: Some(LineItemLedger {
                            revenue_account: Some("...".to_owned()),
                            vat_account: Some("...".to_owned()),
                            ..LineItemLedger::default()
                        }),
                        comment: Some("Példa megjegyzés".to_owned()),
                        erasure_code_count: Some(123),
                        ..LineItem::new(
                            "Kitten doormat",
                            dec!(2.0),
                            "db",
                            dec!(10000),
                            VatRate::percent(27),
                            dec!(20000.0),
                            dec!(5400.0),
                            dec!(25400.0),
                        )
                    },
                    LineItem::new(
                        "Puppy doormat",
                        dec!(2.0),
                        "db",
                        dec!(10000),
                        VatRate::Akk,
                        dec!(20000.0),
                        dec!(5400.0),
                        dec!(25400.0),
                    ),
                ],
            )
        }
    }

    /// `requests/xmlnyugtaget.xml`: a receipt by number, no PDF.
    pub fn query_receipt() -> QueryReceipt {
        QueryReceipt::new(ReceiptSelector::ReceiptNumber("NYGT-2020-1".into()))
    }

    /// `requests/xmlnyugtast.xml`: a receipt reversed by number, no PDF.
    pub fn storno_receipt() -> StornoReceipt {
        StornoReceipt::new("NYGT-2020-1")
    }

    /// `requests/xmlnyugtasend.xml`: a receipt emailed with every address
    /// and text overridden.
    pub fn send_receipt() -> SendReceipt {
        SendReceipt {
            email: Some(ReceiptEmail {
                to: Some(MASKED_EMAIL.to_owned()),
                reply_to: Some(MASKED_EMAIL.to_owned()),
                subject: Some("Email tárgya".to_owned()),
                body: Some("Text in the e-mail.\nSignature".to_owned()),
            }),
            ..SendReceipt::new("NYGT-2020-1")
        }
    }
}

/// Every official response example, parsed through the operation whose
/// documentation page it comes from (`fixtures/SOURCES.md`), with its decoded
/// values asserted.
mod responses {
    use jiff::civil::date;
    use szamlazz_agent::ops::query_xml::InvoiceAppearance;
    use szamlazz_agent::{
        ApiError, Currency, ErrorCode, OutcomeClass, ParseError, PaymentMethod, ResponseError,
        VatRate, error::BODY_EXCERPT_LEN,
    };

    use super::*;

    const TABLE: &[(&str, corpus::Check)] = &[
        ("xmlszamlavalasz.xml", invoice_created),
        ("xmlszamlavalasz_error.xml", invoice_creation_refused),
        ("xmlszamlavalasz_pdf.xml", invoice_created_with_pdf),
        (
            "generating_invoice_text_error.txt",
            invoice_creation_text_error,
        ),
        ("reversing_invoice_text_error.txt", storno_text_error),
        ("credit_entry_text_error.txt", credit_entry_text_error),
        ("querying_pdf_xmlszamlavalasz.xml", pdf_fetched),
        ("querying_pdf_xmlszamlavalasz_error.xml", pdf_query_refused),
        ("querying_pdf_text_error.txt", pdf_query_text_error),
        ("szamla_query.xml", invoice_document),
        ("xmlszamladbkdelvalasz.xml", proforma_deleted),
        ("xmlszamladbkdelvalasz_error.xml", proforma_deletion_refused),
        ("xmlnyugtavalasz.xml", receipt_created),
        ("xmlnyugtasendvalasz.xml", receipt_sent),
        ("xmlnyugtasendvalasz_error.xml", receipt_sending_refused),
        ("taxpayer.xml", taxpayer_found),
        ("taxpayer_invalid_taxnumber.xml", taxpayer_not_found),
        ("taxpayer_error.xml", taxpayer_query_refused),
    ];

    #[test]
    fn every_response_example_parses_through_its_operation() {
        corpus::run("responses", TABLE);
    }

    /// The error szamlazz.hu's answer carries (`sikeres=false`, a `hibakod`
    /// and its message), and the outcome class the caller acts on.
    fn refused_with<R: AgentRequest>(request: &R, body: &[u8]) -> (ApiError, OutcomeClass)
    where
        R::Response: std::fmt::Debug,
    {
        let error = request
            .parse(&delivered(body))
            .expect_err("sikeres=false is an error");
        let ResponseError::Api(api) = &error else {
            panic!("expected the body's error, got {error:?}");
        };

        (api.clone(), error.outcome_class())
    }

    /// The docs stand in for every PDF's base64 with something that is not
    /// base64: a line of four dots inside it, three dots or prose in its
    /// place. As published, the body is refused: a PDF that does not decode
    /// must not pass as one.
    fn refused_as_base64<R: AgentRequest>(request: &R, body: &[u8])
    where
        R::Response: std::fmt::Debug,
    {
        let error = request
            .parse(&delivered(body))
            .expect_err("a PDF that is not base64 is refused");
        assert!(
            matches!(error, ResponseError::Parse(ParseError::Base64(_))),
            "expected a base64 refusal, got {error:?}"
        );
        assert_eq!(error.outcome_class(), OutcomeClass::Unknown);
    }

    /// The example with its four-dot abbreviation line removed: the remaining
    /// lines (wrapped, once broken by a space) are the docs' PDF entire.
    fn without_abbreviation(body: &[u8]) -> Vec<u8> {
        let text = std::str::from_utf8(body).expect("UTF-8 example");
        let lines: Vec<&str> = text.lines().filter(|line| line.trim() != "....").collect();
        assert_eq!(
            lines.len() + 1,
            text.lines().count(),
            "the example carries exactly one abbreviation line"
        );

        lines.join("\n").into_bytes()
    }

    /// The example with the docs' stand-in for a PDF element replaced by a
    /// PDF, so the rest of the document can be read.
    fn with_pdf_in_place(body: &[u8], placeholder: &str, element: &str) -> Vec<u8> {
        let text = std::str::from_utf8(body).expect("UTF-8 example");
        assert!(
            text.contains(placeholder),
            "the example carries the placeholder {placeholder}"
        );

        text.replace(placeholder, &format!("<{element}>JVBERi0=</{element}>"))
            .into_bytes()
    }

    /// The one PDF the docs show, unabbreviated: 567 bytes of PDF 1.4.
    fn assert_is_the_docs_pdf(pdf: &szamlazz_agent::Pdf) {
        assert!(pdf.as_bytes().starts_with(b"%PDF-1.4"));
        assert!(pdf.as_bytes().ends_with(b"%%EOF"));
        assert_eq!(pdf.as_bytes().len(), 567);
    }

    fn invoice_created(body: &[u8]) {
        let created = examples::create_invoice()
            .parse(&delivered(body))
            .expect("the success example parses")
            .into_issued()
            .expect("an issued document");

        assert_eq!(created.invoice_number.as_str(), "XXX-2012-3");
        assert_eq!(created.net_total, Some(dec!(30000)));
        assert_eq!(created.gross_total, Some(dec!(38100)));
        assert_eq!(created.outstanding, None);
        assert_eq!(
            created.document_id, None,
            "the header is not in the example"
        );
        assert!(created.pdf.is_none());
        assert!(!created.notification_delivery_failed);
    }

    /// The login error: code 3 in the body, its message in CDATA across two
    /// lines, and no `szlahu_*` header to read it from first.
    fn invoice_creation_refused(body: &[u8]) {
        let (api, class) = refused_with(&examples::create_invoice(), body);

        assert_eq!(api.code, ErrorCode::InvalidCredentials);
        assert_eq!(
            api.message,
            "Bejelentkezési hiba - a megadott login név és jelszó pároshoz nem létezik\nfelhasználó"
        );
        assert_eq!(class, OutcomeClass::Rejected);
    }

    fn invoice_created_with_pdf(body: &[u8]) {
        let request = examples::create_invoice();
        refused_as_base64(&request, body);

        let created = request
            .parse(&delivered(&without_abbreviation(body)))
            .expect("the unabbreviated example parses")
            .into_issued()
            .expect("an issued document");
        assert_eq!(created.invoice_number.as_str(), "XXX-2012-3");
        assert_eq!(created.net_total, Some(dec!(30000)));
        assert_eq!(created.gross_total, Some(dec!(38100)));
        assert_is_the_docs_pdf(created.pdf.as_ref().expect("the PDF was requested"));
    }

    /// A response-version-1 error: plain text, a Java stack trace after the
    /// message. The crate always asks for version 2, so this is an unexpected
    /// body, quoted as a bounded excerpt, its outcome unknown. The docs show
    /// the one page for all four operations, so the four fixtures share it.
    fn text_error<R: AgentRequest>(request: &R, body: &[u8])
    where
        R::Response: std::fmt::Debug,
    {
        let error = request
            .parse(&delivered(body))
            .expect_err("a text page is not an XML answer");
        let ResponseError::Parse(ParseError::UnexpectedBody(excerpt)) = &error else {
            panic!("expected an unexpected body, got {error:?}");
        };

        assert!(
            excerpt.starts_with("[ERR] Számla mentés sikertelen. Már létező rendelésszám: XX.")
        );
        let truncation = format!("… [truncated: {} bytes]", body.len());
        assert!(
            excerpt.ends_with(&truncation),
            "a two-kilobyte stack trace is quoted bounded: {excerpt}"
        );
        assert!(excerpt.len() <= BODY_EXCERPT_LEN + truncation.len());
        assert_eq!(error.outcome_class(), OutcomeClass::Unknown);
    }

    fn invoice_creation_text_error(body: &[u8]) {
        text_error(&examples::create_invoice(), body);
    }

    fn storno_text_error(body: &[u8]) {
        text_error(&examples::storno_invoice(), body);
    }

    fn credit_entry_text_error(body: &[u8]) {
        text_error(&examples::register_credit_entry(), body);
    }

    fn pdf_query_text_error(body: &[u8]) {
        text_error(&examples::query_invoice_pdf(), body);
    }

    fn pdf_fetched(body: &[u8]) {
        let request = examples::query_invoice_pdf();
        refused_as_base64(&request, body);

        let fetched = request
            .parse(&delivered(&without_abbreviation(body)))
            .expect("the unabbreviated example parses");
        assert_eq!(fetched.invoice_number.as_str(), "XXX-2012-3");
        assert_eq!(fetched.net_total, Some(dec!(30000)));
        assert_eq!(fetched.gross_total, Some(dec!(38100)));
        assert_is_the_docs_pdf(&fetched.pdf);
    }

    fn pdf_query_refused(body: &[u8]) {
        let (api, class) = refused_with(&examples::query_invoice_pdf(), body);

        assert_eq!(api.code, ErrorCode::InvalidCredentials);
        assert!(api.message.starts_with("Bejelentkezési hiba"));
        assert_eq!(class, OutcomeClass::Rejected);
    }

    /// The `<szamla>` document. Its `<pdf>` holds the docs' prose ("The
    /// receipt .pdf can be found here in BASE64 encoding"), which is refused
    /// as base64; with a PDF in its place every block decodes.
    #[allow(clippy::too_many_lines)]
    fn invoice_document(body: &[u8]) {
        let request = examples::query_invoice_xml();
        refused_as_base64(&request, body);

        let with_pdf = with_pdf_in_place(
            body,
            "<pdf>The receipt .pdf can be found here in BASE64 encoding</pdf>",
            "pdf",
        );
        let document = request
            .parse(&delivered(&with_pdf))
            .expect("the example with a PDF parses");

        let supplier = &document.supplier;
        assert_eq!(supplier.id, Some(201));
        assert_eq!(
            supplier.name,
            "Lorem ipsum dolor sit amet, consectetur cs amet., 512356234"
        );
        assert_eq!(supplier.address.country.as_deref(), Some("Hungary"));
        assert_eq!(supplier.address.zip, "1086");
        assert_eq!(supplier.address.city, "Budapest");
        assert_eq!(supplier.address.address, "Szerdahelyi utca 4-8");
        assert_eq!(
            supplier.tax_number.as_deref(),
            Some("202 336 0856"),
            "a tax number with spaces is kept verbatim"
        );
        assert_eq!(supplier.eu_tax_number.as_deref(), Some("DPH:SK2023360856"));
        let bank = supplier.bank.as_ref().expect("bank");
        assert_eq!(bank.name.as_deref(), Some("UniCredit Bank Hungary Zrt."));
        assert_eq!(bank.account, None, "an empty <bankszamla> is none");

        let info = &document.info;
        assert_eq!(info.id, 529_992);
        assert_eq!(info.invoice_number.as_str(), "D-LOLO-66");
        assert_eq!(info.document_type, szamlazz_agent::DocumentType::Proforma);
        assert_eq!(info.appearance, InvoiceAppearance::NotInvoice);
        assert_eq!(info.issue_date, Some(date(2024, 10, 9)));
        assert_eq!(info.fulfillment_date, Some(date(2024, 10, 9)));
        assert_eq!(info.due_date, Some(date(2024, 10, 9)));
        assert_eq!(
            info.payment_method,
            Some(PaymentMethod::Other("credit_card".to_owned()))
        );
        assert_eq!(info.unified_payment_method.as_deref(), Some("other"));
        assert_eq!(info.language.as_deref(), Some("hu"));
        assert_eq!(info.currency, Some(Currency::HUF));
        assert_eq!(info.exchange_rate, Some(dec!(0)));
        assert_eq!(info.comment, None, "an empty <megjegyzes> is none");
        assert!(!info.cash_accounting);
        assert!(info.kata);
        assert_eq!(info.email.as_deref(), Some(MASKED_EMAIL));
        assert_eq!(info.test, Some(false));
        assert_eq!(info.reversed, None);
        assert_eq!(info.order_number, None);

        let buyer = &document.buyer;
        assert_eq!(buyer.id, Some(221_216));
        assert_eq!(buyer.name, "Customer name");
        let address = buyer.address.as_ref().expect("buyer address");
        assert_eq!(address.country.as_deref(), Some("Hungary"));
        assert_eq!(address.zip, "1324");
        assert_eq!(address.city, "Debrecen");
        assert_eq!(address.address, "1234");
        assert_eq!(buyer.email.as_deref(), Some(MASKED_EMAIL));
        assert_eq!(buyer.tax_number, None, "an empty <adoszam> is none");
        let ledger = buyer
            .ledger
            .as_ref()
            .expect("an empty <fokonyv> is a ledger");
        assert_eq!(ledger.account, None);
        assert_eq!(ledger.buyer_id, None);

        assert_eq!(document.items.len(), 1);
        let item = &document.items[0];
        assert_eq!(item.name, "Apple");
        assert_eq!(item.quantity, dec!(1));
        assert_eq!(item.unit, "pieces");
        assert_eq!(item.unit_price, dec!(380));
        assert_eq!(item.vat_rate_code, "20");
        assert_eq!(item.vat_rate(), VatRate::percent(20));
        assert_eq!(item.net_value, dec!(380));
        assert_eq!(item.margin_vat_base, Some(dec!(0)));
        assert_eq!(item.vat_value, dec!(76));
        assert_eq!(item.gross_value, dec!(456));
        assert_eq!(item.comment.as_deref(), Some("Apple comment"));
        let ledger = item
            .ledger
            .as_ref()
            .expect("an empty <fokonyv> is a ledger");
        assert_eq!(ledger.revenue_account, None);
        assert_eq!(ledger.economic_event, None);
        assert!(document.financial_items.is_empty());
        assert!(document.labels.is_empty());

        assert_eq!(document.totals.by_vat_rate.len(), 1);
        let by_rate = &document.totals.by_vat_rate[0];
        assert_eq!(by_rate.vat_rate(), VatRate::percent(20));
        assert_eq!(by_rate.net, dec!(464));
        assert_eq!(by_rate.vat, dec!(93));
        assert_eq!(by_rate.gross, dec!(557));
        assert_eq!(document.totals.total.net, dec!(464));
        assert_eq!(document.totals.total.vat, dec!(93));
        assert_eq!(document.totals.total.gross, dec!(557));

        assert_eq!(document.credit_entries.len(), 1);
        let entry = &document.credit_entries[0];
        assert_eq!(entry.date, date(2020, 9, 22));
        assert_eq!(entry.title, PaymentMethod::Other("transfer".to_owned()));
        assert_eq!(entry.amount, dec!(15));
        assert_eq!(entry.comment.as_deref(), Some("comment"));
        assert_eq!(entry.bank_account.as_deref(), Some("-"));
        assert_eq!(entry.bank_transaction_id, None);

        assert_eq!(
            document.pdf.expect("the PDF put in its place").as_bytes(),
            b"%PDF-"
        );
    }

    fn proforma_deleted(body: &[u8]) {
        examples::delete_proforma()
            .parse(&delivered(body))
            .expect("the success example parses");
    }

    fn proforma_deletion_refused(body: &[u8]) {
        let (api, class) = refused_with(&examples::delete_proforma(), body);

        assert_eq!(api.code, ErrorCode::ProformaNotFound);
        assert_eq!(
            api.message,
            "nincs ilyen díjbekérő a rendszerben, már törlésre került vagy nem is létezett"
        );
        assert_eq!(class, OutcomeClass::Rejected);
    }

    /// The receipt example is commented element by element and carries
    /// `<nyugtaPdf>...</nyugtaPdf>`, three dots for the base64. As published
    /// it is refused (the dots are not base64), and with a PDF in their
    /// place every block decodes, including the second row, which the docs
    /// spell with the invoice's `nettoErtek`/`afaErtek`/`bruttoErtek` names.
    #[allow(clippy::too_many_lines)]
    fn receipt_created(body: &[u8]) {
        let request = examples::create_receipt();
        refused_as_base64(&request, body);

        let with_pdf = with_pdf_in_place(body, "<nyugtaPdf>...</nyugtaPdf>", "nyugtaPdf");
        let receipt = request
            .parse(&delivered(&with_pdf))
            .expect("the example with a PDF parses");

        assert_eq!(
            receipt
                .pdf
                .as_ref()
                .expect("the PDF put in its place")
                .as_bytes(),
            b"%PDF-"
        );
        assert_eq!(receipt.id, 123_456);
        assert_eq!(receipt.call_id, None, "an empty <hivasAzonosito> is none");
        assert_eq!(receipt.receipt_number.as_str(), "NYGT-2017-123");
        assert_eq!(receipt.document_type, szamlazz_agent::ReceiptType::Receipt);
        assert!(!receipt.reversed);
        assert_eq!(
            receipt
                .reversed_receipt_number
                .as_ref()
                .map(szamlazz_agent::ReceiptNumber::as_str),
            Some("NYGT-2017-100")
        );
        assert_eq!(receipt.issue_date, date(2015, 12, 1));
        assert_eq!(
            receipt.payment_method,
            PaymentMethod::Other("cash".to_owned())
        );
        assert_eq!(receipt.currency, Currency::EUR);
        assert_eq!(receipt.exchange_bank, None);
        assert_eq!(receipt.exchange_rate, Some(dec!(210)));
        assert_eq!(receipt.comment, None);
        assert_eq!(receipt.ledger_customer, None);
        assert_eq!(receipt.test, Some(false));
        assert_eq!(receipt.order_number, None);

        assert_eq!(receipt.items.len(), 2);
        let first = &receipt.items[0];
        assert_eq!(first.name, "Kitten doormat");
        assert_eq!(first.id, None, "an empty <azonosito> is none");
        assert_eq!(first.quantity, dec!(2.0));
        assert_eq!(first.unit, "db");
        assert_eq!(first.unit_price, dec!(10000));
        assert_eq!(first.vat_rate(), VatRate::percent(27));
        assert_eq!(first.net_value, dec!(20000.0));
        assert_eq!(first.vat_value, dec!(5400.0));
        assert_eq!(first.gross_value, dec!(25400.0));
        let ledger = first
            .ledger
            .as_ref()
            .expect("an empty <fokonyv> is a ledger");
        assert_eq!(ledger.revenue_account, None);
        assert_eq!(ledger.vat_account, None);
        let second = &receipt.items[1];
        assert_eq!(second.name, "Puppy doormat");
        assert_eq!(second.net_value, dec!(20000.0));
        assert_eq!(second.vat_value, dec!(5400.0));
        assert_eq!(second.gross_value, dec!(25400.0));
        assert!(second.ledger.is_none());

        assert_eq!(receipt.payments.len(), 2);
        assert_eq!(receipt.payments[0].method, "voucher");
        assert_eq!(receipt.payments[0].amount, dec!(1000.0));
        assert_eq!(
            receipt.payments[0].description.as_deref(),
            Some("OTP SZÉP kártya")
        );
        assert_eq!(receipt.payments[1].method, "debit card");
        assert_eq!(receipt.payments[1].amount, dec!(3000.0));
        assert_eq!(receipt.payments[1].description, None);

        assert_eq!(receipt.totals.by_vat_rate.len(), 1);
        let by_rate = &receipt.totals.by_vat_rate[0];
        assert_eq!(by_rate.vat_type.as_deref(), Some("ÁKK"));
        assert_eq!(by_rate.vat_rate_code, "0");
        assert_eq!(by_rate.vat_rate(), VatRate::Akk);
        assert_eq!(by_rate.net, dec!(200));
        assert_eq!(by_rate.vat, dec!(54));
        assert_eq!(by_rate.gross, dec!(254));
        assert_eq!(receipt.totals.total.net, dec!(200));
        assert_eq!(receipt.totals.total.vat, dec!(54));
        assert_eq!(receipt.totals.total.gross, dec!(254));
    }

    /// The acknowledgement spells its empty `<hibakod>` and `<hibauzenet>`
    /// out; empty is no error.
    fn receipt_sent(body: &[u8]) {
        examples::send_receipt()
            .parse(&delivered(body))
            .expect("the success example parses");
    }

    fn receipt_sending_refused(body: &[u8]) {
        let (api, class) = refused_with(&examples::send_receipt(), body);

        assert_eq!(api.code, ErrorCode::MissingData);
        assert_eq!(api.message, "Hiányzó adat: emailtargy elem.");
        assert_eq!(class, OutcomeClass::NotFound);
    }

    /// NAV's answer for szamlazz.hu's own tax number: the `ns2:`-prefixed
    /// data elements under NAV's default namespace.
    fn taxpayer_found(body: &[u8]) {
        let taxpayer = examples::query_taxpayer()
            .parse(&delivered(body))
            .expect("the success example parses");

        assert!(taxpayer.valid);
        assert_eq!(
            taxpayer.name.as_deref(),
            Some("KBOSS.HU KERESKEDELMI ÉS SZOLGÁLTATÓ KORLÁTOLT FELELŐSSÉGŰ TÁRSASÁG")
        );
        assert_eq!(taxpayer.tax_number.as_deref(), Some("13421739"));
        assert_eq!(taxpayer.vat_code.as_deref(), Some("2"));
        assert_eq!(taxpayer.addresses.len(), 1);
        let address = &taxpayer.addresses[0];
        assert_eq!(address.kind.as_deref(), Some("HQ"));
        assert_eq!(address.country_code.as_deref(), Some("HU"));
        assert_eq!(address.postal_code.as_deref(), Some("1031"));
        assert_eq!(address.city.as_deref(), Some("BUDAPEST"));
        assert_eq!(address.street_name.as_deref(), Some("ZÁHONY"));
        assert_eq!(address.public_place_category.as_deref(), Some("UTCA"));
        assert_eq!(address.number.as_deref(), Some("7."));
        assert_eq!(address.building, None);
    }

    /// A well-formed prefix NAV knows no taxpayer under: a success with
    /// `valid: false` and nothing else, never an error.
    fn taxpayer_not_found(body: &[u8]) {
        let taxpayer = examples::query_taxpayer()
            .parse(&delivered(body))
            .expect("an unknown taxpayer is an answer");

        assert!(!taxpayer.valid);
        assert_eq!(taxpayer.name, None);
        assert_eq!(taxpayer.tax_number, None);
        assert_eq!(taxpayer.vat_code, None);
        assert!(taxpayer.addresses.is_empty());
    }

    /// A seven-digit prefix refused by szamlazz.hu's schema check (57) before
    /// it reached NAV: `funcCode` ERROR with the code and message relayed.
    fn taxpayer_query_refused(body: &[u8]) {
        let (api, class) = refused_with(&examples::query_taxpayer(), body);

        assert_eq!(api.code, ErrorCode::MalformedXml);
        assert!(
            api.message
                .starts_with("XML beolvasási hiba. cvc-pattern-valid: Value '1342173'")
        );
        assert_eq!(class, OutcomeClass::Rejected);
    }
}

/// Every official request example, rebuilt with the crate's request types and
/// serialised: the crate's XML has the [outline](outline::Outline) of the example
/// (root, namespace, element names, element order, text) up to the deviations
/// each check lists, which are the crate's deliberate protocol choices.
mod requests {
    use super::outline::{Deviation, assert_equivalent};
    use super::*;

    const TABLE: &[(&str, corpus::Check)] = &[
        ("xmlszamla.xml", create_invoice),
        ("xmlszamlast.xml", storno_invoice),
        ("xmlszamlakifiz.xml", register_credit_entry),
        ("xmlszamlapdf.xml", query_invoice_pdf),
        ("xmlszamlaxml.xml", query_invoice_xml),
        ("xmlszamladbkdel.xml", delete_proforma),
        (
            "xmlszamladbkdel_ordernumber.xml",
            delete_proforma_by_order_number,
        ),
        ("xmltaxpayer.xml", query_taxpayer),
        ("xmlnyugtacreate.xml", create_receipt),
        ("xmlnyugtaget.xml", query_receipt),
        ("xmlnyugtast.xml", storno_receipt),
        ("xmlnyugtasend.xml", send_receipt),
    ];

    #[test]
    fn every_request_example_has_the_outline_the_crate_writes() {
        corpus::run("requests", TABLE);
    }

    /// The crate's XML for a request the crate itself would send: validated
    /// first, so the example is one the wire contract accepts. `key` is the
    /// placeholder agent key the example carries.
    fn written<R: AgentRequest>(request: &R, key: &str) -> Vec<u8> {
        request
            .validate()
            .expect("the example is a request the crate accepts");

        request.write_xml(&Credentials::agent_key(key))
    }

    /// The placeholder key on every documentation page but one.
    const KEY: &str = "Please fill!";

    /// The crate always asks for the structured response (version 2); the
    /// invoice example asks for the text one, and the storno and credit-entry
    /// examples do not say.
    const RESPONSE_VERSION: &str = "2";

    /// The docs' invoice example: every kind flag spelled out as `false`, which
    /// the crate never writes (it writes the one flag of the kind it issues),
    /// the text response version, and the browser's title-cased payment method
    /// where the crate's `PaymentMethod::Transfer` writes the docs' own
    /// lowercase list value (`xmlnyugtacreate.xml`: "átutalás, készpénz, …").
    fn create_invoice(example: &[u8]) {
        assert_equivalent(
            example,
            &written(&examples::create_invoice(), KEY),
            &[
                Deviation {
                    path: "xmlszamla/beallitasok/valaszVerzio",
                    example: Some("1"),
                    ours: Some(RESPONSE_VERSION),
                },
                Deviation {
                    path: "xmlszamla/fejlec/fizmod",
                    example: Some("Átutalás"),
                    ours: Some("átutalás"),
                },
                Deviation {
                    path: "xmlszamla/fejlec/elolegszamla",
                    example: Some("false"),
                    ours: None,
                },
                Deviation {
                    path: "xmlszamla/fejlec/vegszamla",
                    example: Some("false"),
                    ours: None,
                },
                Deviation {
                    path: "xmlszamla/fejlec/helyesbitoszamla",
                    example: Some("false"),
                    ours: None,
                },
                Deviation {
                    path: "xmlszamla/fejlec/dijbekero",
                    example: Some("false"),
                    ours: None,
                },
            ],
        );
    }

    fn storno_invoice(example: &[u8]) {
        assert_equivalent(
            example,
            &written(&examples::storno_invoice(), KEY),
            &[Deviation {
                path: "xmlszamlast/beallitasok/valaszVerzio",
                example: None,
                ours: Some(RESPONSE_VERSION),
            }],
        );
    }

    fn register_credit_entry(example: &[u8]) {
        assert_equivalent(
            example,
            &written(&examples::register_credit_entry(), KEY),
            &[Deviation {
                path: "xmlszamlakifiz/beallitasok/valaszVerzio",
                example: None,
                ours: Some(RESPONSE_VERSION),
            }],
        );
    }

    fn query_invoice_pdf(example: &[u8]) {
        assert_equivalent(example, &written(&examples::query_invoice_pdf(), KEY), &[]);
    }

    /// The XML query's page fills its key placeholder differently.
    fn query_invoice_xml(example: &[u8]) {
        assert_equivalent(
            example,
            &written(&examples::query_invoice_xml(), "Please fill out!"),
            &[],
        );
    }

    /// The docs' deletion examples carry a username, a password *and* an agent
    /// key; a request carries one credential form (`Credentials`), and the
    /// crate's examples use the key.
    const BOTH_CREDENTIAL_FORMS: [Deviation; 2] = [
        Deviation {
            path: "xmlszamladbkdel/beallitasok/felhasznalo",
            example: Some("Test123"),
            ours: None,
        },
        Deviation {
            path: "xmlszamladbkdel/beallitasok/jelszo",
            example: Some("Test123"),
            ours: None,
        },
    ];

    fn delete_proforma(example: &[u8]) {
        assert_equivalent(
            example,
            &written(&examples::delete_proforma(), KEY),
            &BOTH_CREDENTIAL_FORMS,
        );
    }

    fn delete_proforma_by_order_number(example: &[u8]) {
        assert_equivalent(
            example,
            &written(&examples::delete_proforma_by_order_number(), KEY),
            &BOTH_CREDENTIAL_FORMS,
        );
    }

    fn query_taxpayer(example: &[u8]) {
        assert_equivalent(example, &written(&examples::query_taxpayer(), KEY), &[]);
    }

    fn create_receipt(example: &[u8]) {
        assert_equivalent(example, &written(&examples::create_receipt(), KEY), &[]);
    }

    fn query_receipt(example: &[u8]) {
        assert_equivalent(example, &written(&examples::query_receipt(), KEY), &[]);
    }

    fn storno_receipt(example: &[u8]) {
        assert_equivalent(example, &written(&examples::storno_receipt(), KEY), &[]);
    }

    fn send_receipt(example: &[u8]) {
        assert_equivalent(example, &written(&examples::send_receipt(), KEY), &[]);
    }
}

/// The comparable outline of a request document.
///
/// The official examples are pretty-printed, commented and carry the docs'
/// editor artefacts (`xmlns:xsi`, `xsi:schemaLocation`); the crate writes
/// compact XML. What szamlazz.hu reads is the same in both: the root element
/// and its namespace, and every element with text, in document order.
mod outline {
    use quick_xml::events::Event;
    use quick_xml::name::{Namespace, ResolveResult};
    use quick_xml::reader::NsReader;

    /// One leaf of a request document: its element path from the root
    /// (`xmlszamla/fejlec/fizmod`) and its text, trimmed.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Leaf {
        pub path: String,
        pub text: String,
    }

    /// What two request documents are compared on.
    ///
    /// Not part of the outline: the XML declaration, comments, whitespace
    /// around text, every attribute but the root's default namespace, and
    /// an element with no text and no leaf below it; an empty optional
    /// element and an omitted one are the same request to szamlazz.hu (the
    /// docs say "omit this tag" beside `<aggregator></aggregator>`), and
    /// the crate omits.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Outline {
        pub root: String,
        pub namespace: String,
        pub leaves: Vec<Leaf>,
    }

    /// The outline of a well-formed XML document.
    pub fn outline(xml: &[u8]) -> Outline {
        let mut reader = NsReader::from_reader(xml);
        let mut root: Option<(String, String)> = None;
        let mut path: Vec<String> = Vec::new();
        let mut text = String::new();
        let mut leaves = Vec::new();

        loop {
            let (namespace, event) = reader.read_resolved_event().expect("well-formed XML");

            match event {
                Event::Start(start) => {
                    let name = start.local_name().as_ref().to_owned();

                    if root.is_none() {
                        let namespace = match namespace {
                            ResolveResult::Bound(Namespace(namespace)) => namespace.to_owned(),
                            ResolveResult::Unbound | ResolveResult::Unknown(_) => String::new(),
                        };
                        root = Some((name.clone(), namespace));
                    }
                    path.push(name);
                    text.clear();
                }
                Event::Empty(_) => text.clear(),
                Event::Text(content) => text.push_str(&content.xml10_content()),
                Event::CData(content) => text.push_str(&content.xml10_content()),
                Event::GeneralRef(reference) => {
                    let resolved = reference
                        .resolve_char_ref()
                        .expect("a well-formed character reference");

                    match resolved {
                        Some(character) => text.push(character),
                        None => text.push_str(match reference.as_ref() {
                            "amp" => "&",
                            "lt" => "<",
                            "gt" => ">",
                            "apos" => "'",
                            "quot" => "\"",
                            other => panic!("unknown entity reference &{other};"),
                        }),
                    }
                }
                Event::End(_) => {
                    let trimmed = text.trim();

                    if !trimmed.is_empty() {
                        leaves.push(Leaf {
                            path: path.join("/"),
                            text: trimmed.to_owned(),
                        });
                    }
                    text.clear();
                    path.pop();
                }
                Event::Eof => break,
                _ => {}
            }
        }

        let (root, namespace) = root.expect("a document has a root element");

        Outline {
            root,
            namespace,
            leaves,
        }
    }

    /// One place where the crate's request deliberately differs from the
    /// official example: a leaf only one side writes, or a leaf whose text
    /// differs. Every deviation is asserted to exist, so a stale one fails.
    #[derive(Debug)]
    pub struct Deviation {
        /// The leaf's path from the root, `xmlszamla/beallitasok/valaszVerzio`.
        pub path: &'static str,
        /// The example's text; `None` when the example has no such leaf.
        pub example: Option<&'static str>,
        /// The crate's text; `None` when the crate writes no such leaf.
        pub ours: Option<&'static str>,
    }

    /// Asserts that `ours` has the outline of the official `example`, up to the
    /// listed deviations.
    ///
    /// # Panics
    ///
    /// When the outlines differ beyond the deviations, or a deviation names a
    /// leaf its side does not have.
    #[track_caller]
    pub fn assert_equivalent(example: &[u8], ours: &[u8], deviations: &[Deviation]) {
        let mut expected = outline(example);
        let mut actual = outline(ours);

        for deviation in deviations {
            match (deviation.example, deviation.ours) {
                (Some(example_text), Some(our_text)) => {
                    let leaf = take(
                        &mut expected.leaves,
                        deviation.path,
                        example_text,
                        "example",
                    );
                    expected.leaves.insert(
                        leaf,
                        Leaf {
                            path: deviation.path.to_owned(),
                            text: our_text.to_owned(),
                        },
                    );
                }
                (Some(example_text), None) => {
                    take(
                        &mut expected.leaves,
                        deviation.path,
                        example_text,
                        "example",
                    );
                }
                (None, Some(our_text)) => {
                    take(&mut actual.leaves, deviation.path, our_text, "crate");
                }
                (None, None) => panic!("deviation {} names neither side", deviation.path),
            }
        }

        assert_eq!(
            actual, expected,
            "the crate's request (left) has the outline of the official example (right)"
        );
    }

    /// Removes the leaf a deviation names from one side, returning its
    /// position; a leaf that is not there is a stale deviation.
    #[track_caller]
    fn take(leaves: &mut Vec<Leaf>, path: &str, text: &str, side: &str) -> usize {
        let position = leaves
            .iter()
            .position(|leaf| leaf.path == path && leaf.text == text)
            .unwrap_or_else(|| panic!("stale deviation: the {side} has no {path} = {text:?} leaf"));
        leaves.remove(position);

        position
    }

    mod tests {
        use super::*;

        fn leaf(path: &str, text: &str) -> Leaf {
            Leaf {
                path: path.to_owned(),
                text: text.to_owned(),
            }
        }

        #[test]
        fn a_pretty_printed_commented_example_has_the_outline_of_its_compact_form() {
            let pretty = br#"<?xml version="1.0" encoding="UTF-8"?>
<r xmlns="urn:x" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="urn:x https://example.com/x.xsd">
  <a>                       <!-- REQ -->
    <b>1</b>                <!-- string --> <!-- the first -->
    <c>two words</c>
  </a>
  <d>3</d>
</r>"#;
            let compact = br#"<?xml version="1.0" encoding="UTF-8"?><r xmlns="urn:x"><a><b>1</b><c>two words</c></a><d>3</d></r>"#;

            assert_eq!(outline(pretty), outline(compact));
            assert_eq!(
                outline(compact),
                Outline {
                    root: "r".to_owned(),
                    namespace: "urn:x".to_owned(),
                    leaves: vec![
                        leaf("r/a/b", "1"),
                        leaf("r/a/c", "two words"),
                        leaf("r/d", "3")
                    ],
                }
            );
        }

        /// The docs fill optional elements with nothing (`<aggregator></aggregator>`,
        /// `<fuvarlevel><uticel></uticel></fuvarlevel>`); the crate omits them.
        #[test]
        fn an_empty_element_and_an_omitted_one_have_the_same_outline() {
            let filled = br#"<r xmlns="urn:x"><a></a><b><c></c><d/></b><e>x</e><f>  </f></r>"#;
            let omitted = br#"<r xmlns="urn:x"><e>x</e></r>"#;

            assert_eq!(outline(filled), outline(omitted));
            assert_eq!(outline(omitted).leaves, [leaf("r/e", "x")]);
        }

        /// Element order is fixed in the Számla Agent XML, so it is part of
        /// the outline; so is the namespace, which selects the operation's schema.
        #[test]
        fn element_order_and_the_namespace_tell_outlines_apart() {
            let reference = br#"<r xmlns="urn:x"><a>1</a><b>2</b></r>"#;

            assert_ne!(
                outline(reference),
                outline(br#"<r xmlns="urn:x"><b>2</b><a>1</a></r>"#)
            );
            assert_ne!(
                outline(reference),
                outline(br#"<r xmlns="urn:y"><a>1</a><b>2</b></r>"#)
            );
            assert_ne!(
                outline(reference),
                outline(br#"<r xmlns="urn:x"><a>1</a><b>3</b></r>"#)
            );
        }

        /// The crate escapes text; the docs may write the same characters
        /// as references or in CDATA. The outline holds the characters.
        #[test]
        fn text_is_compared_unescaped() {
            let escaped = br#"<r xmlns="urn:x"><a>x &lt; y &amp; &#233;</a></r>"#;
            let cdata = r#"<r xmlns="urn:x"><a><![CDATA[x < y & é]]></a></r>"#;

            assert_eq!(outline(escaped).leaves, [leaf("r/a", "x < y & é")]);
            assert_eq!(outline(escaped), outline(cdata.as_bytes()));
        }

        const EXAMPLE: &[u8] =
            br#"<r xmlns="urn:x"><v>1</v><a>x</a><flag>false</flag><b>y</b></r>"#;
        const OURS: &[u8] = br#"<r xmlns="urn:x"><v>2</v><a>x</a><b>y</b><extra>z</extra></r>"#;

        /// The three kinds of deviation: a text the crate writes differently,
        /// a leaf only the example has, a leaf only the crate has.
        #[test]
        fn a_request_is_equivalent_to_its_example_up_to_the_listed_deviations() {
            assert_equivalent(
                EXAMPLE,
                OURS,
                &[
                    Deviation {
                        path: "r/v",
                        example: Some("1"),
                        ours: Some("2"),
                    },
                    Deviation {
                        path: "r/flag",
                        example: Some("false"),
                        ours: None,
                    },
                    Deviation {
                        path: "r/extra",
                        example: None,
                        ours: Some("z"),
                    },
                ],
            );
        }

        #[test]
        #[should_panic(expected = "r/flag")]
        fn an_unlisted_difference_fails() {
            assert_equivalent(
                EXAMPLE,
                OURS,
                &[
                    Deviation {
                        path: "r/v",
                        example: Some("1"),
                        ours: Some("2"),
                    },
                    Deviation {
                        path: "r/extra",
                        example: None,
                        ours: Some("z"),
                    },
                ],
            );
        }

        /// A deviation that no longer holds (the crate caught up with the
        /// example, or the example changed) is reported, not silently unused.
        #[test]
        #[should_panic(expected = "r/gone")]
        fn a_stale_deviation_fails() {
            assert_equivalent(
                br#"<r xmlns="urn:x"><a>x</a></r>"#,
                br#"<r xmlns="urn:x"><a>x</a></r>"#,
                &[Deviation {
                    path: "r/gone",
                    example: Some("1"),
                    ours: None,
                }],
            );
        }
    }
}
