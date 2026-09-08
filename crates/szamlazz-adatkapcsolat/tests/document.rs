//! Parse-level tests: `Document::parse` refuses shape only, `parse_strict` /
//! `validate` carry the XSD conformance checks. Feature-free, so they run in
//! every configuration.

use szamlazz_adatkapcsolat::{
    BankTransaction, Document, ParseError, Pdf, ReceiptBatch, TransactionDirection,
};

const OUTGOING_INVOICE: &str = include_str!("synthetic/szamla.xml");

/// An outgoing invoice carrying nothing but what the receiver needs to Ack:
/// the root, `alap/id` and `alap/szamlaszam`.
const IDENTITY_ONLY_INVOICE: &str = r#"<szamla xmlns="http://www.szamlazz.hu/szamla">
  <alap><id>123456</id><szamlaszam>2015-123</szamlaszam></alap>
</szamla>"#;

fn outgoing(body: &str) -> Result<szamlazz_adatkapcsolat::InvoiceDocument, ParseError> {
    match Document::parse(body.as_bytes())? {
        Document::OutgoingInvoice(invoice) => Ok(invoice),
        other => panic!("expected outgoing invoice, got {other:?}"),
    }
}

/// The message `parse_strict` refuses `body` with.
fn strict_refusal(body: &str) -> String {
    let error = Document::parse_strict(body.as_bytes()).expect_err("strict parse must refuse");
    assert!(
        matches!(error, ParseError::Validation(_)),
        "expected a validation error, got {error:?}"
    );
    error.to_string()
}

#[test]
fn identity_only_invoice_parses_with_every_other_field_absent() {
    let invoice = outgoing(IDENTITY_ONLY_INVOICE).expect("identity is enough to Ack");
    assert_eq!(invoice.info.id, 123_456);
    assert_eq!(invoice.info.invoice_number, "2015-123");
    assert_eq!(invoice.info.issue_date, None);
    assert_eq!(invoice.info.kind, None);
    assert_eq!(invoice.info.e_invoice, None);
    assert_eq!(invoice.info.test, None);
    assert_eq!(invoice.supplier.name, None);
    assert_eq!(invoice.supplier.id, None);
    assert!(invoice.supplier.address.is_none());
    assert_eq!(invoice.buyer.name, None);
    assert_eq!(invoice.buyer.location, None);
    assert!(invoice.items.is_empty());
    assert!(invoice.totals.per_vat_rate.is_empty());
    assert!(invoice.totals.grand.is_none());
    assert!(invoice.payments.is_empty());
    assert!(invoice.pdf.is_none());
    assert_eq!(invoice.raw_xml(), Some(IDENTITY_ONLY_INVOICE));
}

/// Removes the first `element` inside the first `<{section}>…</{section}>`
/// block of `body`, so a `<netto>` in `afakulcsossz` can be addressed apart
/// from the one in `tetel`.
fn remove_within(body: &str, section: &str, element: &str) -> String {
    let open = format!("<{section}>");
    let close = format!("</{section}>");
    let start = body
        .find(&open)
        .unwrap_or_else(|| panic!("fixture drifted: no {open}"));
    let end = start
        + body[start..]
            .find(&close)
            .unwrap_or_else(|| panic!("fixture drifted: no {close}"));
    let block = &body[start..end];
    assert!(
        block.contains(element),
        "fixture drifted: {element} not inside {open}"
    );
    format!(
        "{}{}{}",
        &body[..start],
        block.replacen(element, "", 1),
        &body[end..]
    )
}

/// Every element `szamla.xsd` marks `minOccurs="1"` beyond the identity, as
/// the synthetic fixture carries it: the block it sits in, the element, and
/// the message the strict parse refuses its absence with. The lenient parse
/// reads each absence as `None`.
const XSD_REQUIRED_INVOICE_ELEMENTS: &[(&str, &str, &str)] = &[
    (
        "alap",
        "<gazdEsemAzon>1</gazdEsemAzon>",
        "invoice alap/gazdEsemAzon",
    ),
    ("alap", "<tipus>SZ</tipus>", "invoice alap/tipus"),
    ("alap", "<eszamla>1</eszamla>", "invoice alap/eszamla"),
    ("alap", "<kelt>2015-12-01</kelt>", "invoice alap/kelt"),
    ("alap", "<telj>2015-12-02</telj>", "invoice alap/telj"),
    ("alap", "<fizh>2015-12-03</fizh>", "invoice alap/fizh"),
    ("alap", "<fizmod>bankkártya</fizmod>", "invoice alap/fizmod"),
    (
        "alap",
        "<fizmodunified>bankkártya</fizmodunified>",
        "invoice alap/fizmodunified",
    ),
    (
        "alap",
        "<keszpenz>false</keszpenz>",
        "invoice alap/keszpenz",
    ),
    ("alap", "<nyelv>hu</nyelv>", "invoice alap/nyelv"),
    (
        "alap",
        "<devizanem>EUR</devizanem>",
        "invoice alap/devizanem",
    ),
    (
        "alap",
        "<penzforg>false</penzforg>",
        "invoice alap/penzforg",
    ),
    ("alap", "<kata>false</kata>", "invoice alap/kata"),
    (
        "alap",
        "<katafokonyv>false</katafokonyv>",
        "invoice alap/katafokonyv",
    ),
    ("alap", "<teszt>false</teszt>", "invoice alap/teszt"),
    ("szallito", "<id>1</id>", "invoice szallito/id"),
    (
        "szallito",
        "<nev>Synthetic Supplier Kft.</nev>",
        "invoice szallito/nev",
    ),
    (
        "szallito",
        "<cim><irsz>1111</irsz><telepules>Testvaros</telepules><cim>Minta utca 1.</cim></cim>",
        "invoice szallito/cim",
    ),
    ("szallito", "<irsz>1111</irsz>", "invoice szallito/cim/irsz"),
    (
        "szallito",
        "<telepules>Testvaros</telepules>",
        "invoice szallito/cim/telepules",
    ),
    (
        "szallito",
        "<cim>Minta utca 1.</cim>",
        "invoice szallito/cim/cim",
    ),
    (
        "szallito",
        "<adoszam>11111111-1-11</adoszam>",
        "invoice szallito/adoszam",
    ),
    (
        "vevo",
        "<nev>Synthetic Buyer Kft.</nev>",
        "invoice vevo/nev",
    ),
    (
        "vevo",
        "<cim><irsz>2222</irsz><telepules>Mintavaros</telepules><cim>Teszt ter 2.</cim></cim>",
        "invoice vevo/cim",
    ),
    ("vevo", "<irsz>2222</irsz>", "invoice vevo/cim/irsz"),
    (
        "vevo",
        "<telepules>Mintavaros</telepules>",
        "invoice vevo/cim/telepules",
    ),
    ("vevo", "<cim>Teszt ter 2.</cim>", "invoice vevo/cim/cim"),
    (
        "vevo",
        "<adoszam>22222222-2-22</adoszam>",
        "invoice vevo/adoszam",
    ),
    ("vevo", "<lokacio>1</lokacio>", "invoice vevo/lokacio"),
    (
        "vevo",
        "<privatePersonIndicator>false</privatePersonIndicator>",
        "outgoing invoice vevo/privatePersonIndicator",
    ),
    ("tetel", "<nev>Synthetic item</nev>", "invoice tetel/nev"),
    (
        "tetel",
        "<mennyiseg>2</mennyiseg>",
        "invoice tetel/mennyiseg",
    ),
    (
        "tetel",
        "<mennyisegiegyseg>db</mennyisegiegyseg>",
        "invoice tetel/mennyisegiegyseg",
    ),
    (
        "tetel",
        "<nettoegysegar>100</nettoegysegar>",
        "invoice tetel/nettoegysegar",
    ),
    ("tetel", "<afakulcs>0</afakulcs>", "invoice tetel/afakulcs"),
    ("tetel", "<netto>200</netto>", "invoice tetel/netto"),
    ("tetel", "<afa>0</afa>", "invoice tetel/afa"),
    ("tetel", "<brutto>200</brutto>", "invoice tetel/brutto"),
    (
        "tetel",
        "<sztetordering>1</sztetordering>",
        "invoice tetel/sztetordering",
    ),
    (
        "afakulcsossz",
        "<afakulcs>0</afakulcs>",
        "afakulcsossz/afakulcs",
    ),
    ("afakulcsossz", "<netto>200</netto>", "afakulcsossz/netto"),
    ("afakulcsossz", "<afa>0</afa>", "afakulcsossz/afa"),
    (
        "afakulcsossz",
        "<brutto>200</brutto>",
        "afakulcsossz/brutto",
    ),
    (
        "osszegek",
        "<totalossz><netto>200</netto><afa>0</afa><brutto>200</brutto></totalossz>",
        "osszegek/totalossz",
    ),
    ("totalossz", "<netto>200</netto>", "totalossz/netto"),
    ("totalossz", "<afa>0</afa>", "totalossz/afa"),
    ("totalossz", "<brutto>200</brutto>", "totalossz/brutto"),
    (
        "kifizetes",
        "<datum>2015-12-04</datum>",
        "invoice kifizetes/datum",
    ),
    (
        "kifizetes",
        "<jogcim>transfer</jogcim>",
        "invoice kifizetes/jogcim",
    ),
    (
        "kifizetes",
        "<osszeg>200</osszeg>",
        "invoice kifizetes/osszeg",
    ),
];

#[test]
fn each_xsd_required_element_is_optional_to_parse_and_required_by_parse_strict() {
    for (section, element, field) in XSD_REQUIRED_INVOICE_ELEMENTS {
        let body = remove_within(OUTGOING_INVOICE, section, element);

        outgoing(&body).unwrap_or_else(|error| {
            panic!("removing {element} from {section} must parse: {error}")
        });

        assert_eq!(
            strict_refusal(&body),
            format!("invalid document structure: missing required {field}"),
            "removing {element} from {section}"
        );
    }
}

/// A financial item (`qutet`) as `preserves_extended_invoice_fields_in_both_directions`
/// splices it in; the synthetic fixture carries none.
const QUTET: &str = "<qutetek><qutet><nev>Fee</nev><afakulcs>27</afakulcs><netto>10</netto><afa>0</afa><brutto>10</brutto><afalevon>0</afalevon></qutet></qutetek>";

const XSD_REQUIRED_QUTET_ELEMENTS: &[(&str, &str)] = &[
    ("<nev>Fee</nev>", "invoice qutet/nev"),
    ("<afakulcs>27</afakulcs>", "invoice qutet/afakulcs"),
    ("<netto>10</netto>", "invoice qutet/netto"),
    ("<afa>0</afa>", "invoice qutet/afa"),
    ("<brutto>10</brutto>", "invoice qutet/brutto"),
    ("<afalevon>0</afalevon>", "invoice qutet/afalevon"),
];

#[test]
fn each_xsd_required_qutet_element_is_optional_to_parse_and_required_by_parse_strict() {
    let with_qutet = OUTGOING_INVOICE.replacen("<osszegek>", &format!("{QUTET}<osszegek>"), 1);
    Document::parse_strict(with_qutet.as_bytes()).expect("the enriched fixture conforms");

    for (element, field) in XSD_REQUIRED_QUTET_ELEMENTS {
        let body = remove_within(&with_qutet, "qutet", element);
        let invoice = outgoing(&body)
            .unwrap_or_else(|error| panic!("removing {element} must parse: {error}"));
        assert_eq!(invoice.financial_items.len(), 1);
        assert_eq!(
            strict_refusal(&body),
            format!("invalid document structure: missing required {field}"),
            "removing {element}"
        );
    }
}

#[test]
fn strict_parse_requires_line_items_and_totals() {
    let start = OUTGOING_INVOICE.find("<tetelek>").expect("tetelek");
    let end = OUTGOING_INVOICE.find("</tetelek>").expect("tetelek") + "</tetelek>".len();
    let without_items = format!("{}{}", &OUTGOING_INVOICE[..start], &OUTGOING_INVOICE[end..]);
    assert!(outgoing(&without_items).expect("parses").items.is_empty());
    assert_eq!(
        strict_refusal(&without_items),
        "invalid document structure: invoice tetelek must contain at least one tetel"
    );

    let without_vat_totals = OUTGOING_INVOICE.replacen(
        "<afakulcsossz><afatipus>ÁKK</afatipus><afakulcs>0</afakulcs><netto>200</netto><afa>0</afa><brutto>200</brutto></afakulcsossz>",
        "",
        1,
    );
    assert!(
        outgoing(&without_vat_totals)
            .expect("parses")
            .totals
            .per_vat_rate
            .is_empty()
    );
    assert_eq!(
        strict_refusal(&without_vat_totals),
        "invalid document structure: invoice osszegek must contain at least one afakulcsossz"
    );
}

#[test]
fn negative_vat_rate_parses_and_fails_strict() {
    let body = OUTGOING_INVOICE.replacen("<afakulcs>0</afakulcs>", "<afakulcs>-5</afakulcs>", 1);
    let invoice = outgoing(&body).expect("a negative rate is content");
    assert_eq!(invoice.items[0].vat_rate, Some(rust_decimal::dec!(-5)));
    assert_eq!(
        strict_refusal(&body),
        "invalid document structure: invoice tetel/afakulcs must not be negative"
    );
}

#[test]
fn incoming_invoice_does_not_require_private_person_indicator() {
    let incoming = OUTGOING_INVOICE
        .replace(
            "http://www.szamlazz.hu/szamla",
            "http://www.szamlazz.hu/szamlabe",
        )
        .replace("<szamla xmlns=", "<szamlabe xmlns=")
        .replace("</szamla>", "</szamlabe>")
        .replacen(
            "<privatePersonIndicator>false</privatePersonIndicator>",
            "",
            1,
        );
    let document = Document::parse_strict(incoming.as_bytes()).expect("conforms to szamlabe.xsd");
    assert!(matches!(document, Document::IncomingInvoice(_)));

    let without_location = incoming.replacen("<lokacio>1</lokacio>", "<lokacio></lokacio>", 1);
    assert_eq!(
        strict_refusal(&without_location),
        "invalid document structure: missing required invoice vevo/lokacio"
    );
}

#[test]
fn validate_on_a_parsed_document_reports_the_same_verdict_as_parse_strict() {
    let body = OUTGOING_INVOICE.replacen("<kelt>2015-12-01</kelt>", "", 1);
    let document = Document::parse(body.as_bytes()).expect("parses");
    let verdict = document
        .validate()
        .expect_err("kelt is required by the XSD");
    assert_eq!(verdict.to_string(), "missing required invoice alap/kelt");

    Document::parse(OUTGOING_INVOICE.as_bytes())
        .expect("parses")
        .validate()
        .expect("the fixture conforms");
}

const BANK_TRANSACTION: &str = r#"<banktranz xmlns="http://www.szamlazz.hu/banktranz">
  <id>987</id>
  <bankszamla>11111111-22222222-33333333</bankszamla>
  <erteknap>2026-07-03</erteknap>
  <irany>BE</irany>
  <technikai>false</technikai>
  <osszeg>12700.0</osszeg>
  <devizanem>HUF</devizanem>
</banktranz>"#;

fn bank_transaction(body: &str) -> Result<BankTransaction, ParseError> {
    match Document::parse(body.as_bytes())? {
        Document::BankTransaction(transaction) => Ok(transaction),
        other => panic!("expected bank transaction, got {other:?}"),
    }
}

#[test]
fn identity_only_bank_transaction_parses_with_every_other_field_absent() {
    let body = r#"<banktranz xmlns="http://www.szamlazz.hu/banktranz"><id>987</id></banktranz>"#;
    let transaction = bank_transaction(body).expect("the id is enough to Ack");
    assert_eq!(transaction.id, 987);
    assert_eq!(transaction.bank_account, None);
    assert_eq!(transaction.value_date, None);
    assert_eq!(transaction.direction, None);
    assert_eq!(transaction.technical, None);
    assert_eq!(transaction.amount, None);
    assert_eq!(transaction.currency, None);
    assert_eq!(transaction.raw_xml(), Some(body));
}

/// Every element `banktranz.xsd` marks `minOccurs="1"` beyond the id.
const XSD_REQUIRED_TRANSACTION_ELEMENTS: &[(&str, &str)] = &[
    (
        "<bankszamla>11111111-22222222-33333333</bankszamla>",
        "bank transaction bankszamla",
    ),
    (
        "<erteknap>2026-07-03</erteknap>",
        "bank transaction erteknap",
    ),
    ("<irany>BE</irany>", "bank transaction irany"),
    ("<technikai>false</technikai>", "bank transaction technikai"),
    ("<osszeg>12700.0</osszeg>", "bank transaction osszeg"),
    ("<devizanem>HUF</devizanem>", "bank transaction devizanem"),
];

#[test]
fn each_xsd_required_transaction_element_is_optional_to_parse_and_required_by_parse_strict() {
    Document::parse_strict(BANK_TRANSACTION.as_bytes()).expect("the fixture conforms");

    for (element, field) in XSD_REQUIRED_TRANSACTION_ELEMENTS {
        assert!(
            BANK_TRANSACTION.contains(element),
            "fixture drifted: {element}"
        );
        let body = BANK_TRANSACTION.replacen(element, "", 1);
        bank_transaction(&body)
            .unwrap_or_else(|error| panic!("removing {element} must parse: {error}"));
        assert_eq!(
            strict_refusal(&body),
            format!("invalid document structure: missing required {field}"),
            "removing {element}"
        );
    }

    // An empty element reads as absent, like every other optional element.
    let body = BANK_TRANSACTION.replacen("<technikai>false</technikai>", "<technikai/>", 1);
    assert_eq!(bank_transaction(&body).expect("parses").technical, None);
}

#[test]
fn unknown_transaction_direction_is_kept_as_other() {
    let body = BANK_TRANSACTION.replacen("<irany>BE</irany>", "<irany>XX</irany>", 1);
    let transaction = bank_transaction(&body).expect("an unknown token is content");
    assert_eq!(
        transaction.direction,
        Some(TransactionDirection::Other("XX".to_owned()))
    );
    assert_eq!(
        transaction
            .direction
            .as_ref()
            .map(TransactionDirection::code),
        Some("XX")
    );
    assert_eq!(
        strict_refusal(&body),
        "invalid document structure: bank transaction irany has unknown value XX"
    );

    let known = bank_transaction(BANK_TRANSACTION).expect("parses");
    assert_eq!(known.direction, Some(TransactionDirection::Incoming));
    assert_eq!(
        known.direction.as_ref().map(TransactionDirection::code),
        Some("BE")
    );
    let outgoing =
        bank_transaction(&BANK_TRANSACTION.replacen("<irany>BE</irany>", "<irany>KI</irany>", 1))
            .expect("parses");
    assert_eq!(outgoing.direction, Some(TransactionDirection::Outgoing));
}

#[test]
fn transaction_direction_serializes_as_a_plain_string_for_every_variant() {
    for (token, expected_json) in [
        ("BE", "\"Incoming\""),
        ("KI", "\"Outgoing\""),
        ("XX", "\"XX\""),
    ] {
        let body =
            BANK_TRANSACTION.replacen("<irany>BE</irany>", &format!("<irany>{token}</irany>"), 1);
        let transaction = bank_transaction(&body).expect("parses");
        let json = serde_json::to_string(&transaction.direction).expect("json");
        assert_eq!(json, expected_json, "token {token}");
    }
}

fn with_pdf(encoded: &str) -> String {
    OUTGOING_INVOICE.replacen("<pdf></pdf>", &format!("<pdf>{encoded}</pdf>"), 1)
}

#[test]
fn well_formed_pdf_still_decodes() {
    let body = with_pdf("JVBERi0=");
    let invoice = outgoing(&body).expect("parses");
    assert_eq!(
        invoice.pdf.as_ref().map(Pdf::as_bytes),
        Some(b"%PDF-".as_slice())
    );

    // Whitespace inside the element is not content.
    let body = with_pdf("\n    JVBE\n    Ri0=\n  ");
    let invoice = outgoing(&body).expect("parses");
    assert_eq!(
        invoice.pdf.as_ref().map(Pdf::as_bytes),
        Some(b"%PDF-".as_slice())
    );
}

#[test]
fn non_canonical_base64_decodes_when_it_can_and_reads_as_absent_when_it_cannot() {
    // Missing padding and set trailing bits are encoder sloppiness, not a
    // different PDF: both decode.
    for sloppy in ["JVBERi0", "JVBERi1"] {
        let invoice = outgoing(&with_pdf(sloppy)).expect("parses");
        assert_eq!(
            invoice.pdf.as_ref().map(Pdf::as_bytes),
            Some(b"%PDF-".as_slice()),
            "{sloppy}"
        );
    }

    // What no decoder can read leaves `pdf` absent: the invoice itself is
    // still delivered, and the element's text is in the raw XML.
    for garbage in ["JVBERi0===", "not base64!"] {
        let body = with_pdf(garbage);
        let invoice = outgoing(&body).expect("a PDF that does not decode must not fail the push");
        assert!(invoice.pdf.is_none(), "{garbage}");
        assert_eq!(invoice.raw_xml(), Some(body.as_str()));
        assert_eq!(invoice.info.id, 123_456);
    }
}

const RECEIPT_BATCH: &str = r#"<xmlnyugtaarchiv xmlns="http://www.szamlazz.hu/xmlnyugtaarchiv">
  <nyugta>
    <alap><id>1</id><nyugtaszam>NYGTA-2026-1</nyugtaszam><tipus>NY</tipus><stornozott>false</stornozott><kelt>2026-07-03</kelt><fizmod>készpénz</fizmod><penznem>HUF</penznem><teszt>false</teszt><adoszam>12345678-1-42</adoszam></alap>
    <tetelek><tetel><megnevezes>Service</megnevezes><nettoEgysegar>100</nettoEgysegar><mennyiseg>1</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><netto>100</netto><afakulcs>27</afakulcs><afa>27</afa><brutto>127</brutto></tetel></tetelek>
    <kifizetesek><kifizetes><fizetoeszkoz>készpénz</fizetoeszkoz><osszeg>127</osszeg></kifizetes></kifizetesek>
    <osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>100</netto><afa>27</afa><brutto>127</brutto></afakulcsossz><totalossz><netto>100</netto><afa>27</afa><brutto>127</brutto></totalossz></osszegek>
  </nyugta>
</xmlnyugtaarchiv>"#;

fn receipts(body: &str) -> Result<ReceiptBatch, ParseError> {
    match Document::parse(body.as_bytes())? {
        Document::Receipts(batch) => Ok(batch),
        other => panic!("expected receipts, got {other:?}"),
    }
}

#[test]
fn identity_only_receipt_parses_with_every_other_field_absent() {
    let body = r#"<xmlnyugtaarchiv xmlns="http://www.szamlazz.hu/xmlnyugtaarchiv"><nyugta><alap><id>1</id></alap></nyugta></xmlnyugtaarchiv>"#;
    let batch = receipts(body).expect("the id is enough to Ack");
    assert_eq!(batch.receipts.len(), 1);
    let receipt = &batch.receipts[0];
    assert_eq!(receipt.info.id, 1);
    assert_eq!(receipt.info.receipt_number, None);
    assert_eq!(receipt.info.kind, None);
    assert_eq!(receipt.info.issue_date, None);
    assert!(receipt.items.is_empty());
    assert!(receipt.totals.grand.is_none());
    assert_eq!(batch.raw_xml(), Some(body));
}

#[test]
fn empty_receipt_batch_parses_and_fails_strict() {
    let body = r#"<xmlnyugtaarchiv xmlns="http://www.szamlazz.hu/xmlnyugtaarchiv"/>"#;
    assert!(
        receipts(body)
            .expect("an empty batch is content, not shape")
            .receipts
            .is_empty()
    );
    assert_eq!(
        strict_refusal(body),
        "invalid document structure: receipt archive must contain at least one nyugta"
    );
}

/// Every element `xmlnyugtaarchiv.xsd` marks `minOccurs="1"` beyond the id,
/// except `alap/adoszam`, which official batches omit (so the strict parse
/// does not require it either).
const XSD_REQUIRED_RECEIPT_ELEMENTS: &[(&str, &str)] = &[
    (
        "<nyugtaszam>NYGTA-2026-1</nyugtaszam>",
        "receipt alap/nyugtaszam",
    ),
    ("<tipus>NY</tipus>", "receipt alap/tipus"),
    ("<stornozott>false</stornozott>", "receipt alap/stornozott"),
    ("<kelt>2026-07-03</kelt>", "receipt alap/kelt"),
    ("<fizmod>készpénz</fizmod>", "receipt alap/fizmod"),
    ("<penznem>HUF</penznem>", "receipt alap/penznem"),
    ("<teszt>false</teszt>", "receipt alap/teszt"),
    (
        "<megnevezes>Service</megnevezes>",
        "receipt tetel/megnevezes",
    ),
    (
        "<nettoEgysegar>100</nettoEgysegar>",
        "receipt tetel/nettoEgysegar",
    ),
    ("<mennyiseg>1</mennyiseg>", "receipt tetel/mennyiseg"),
    (
        "<mennyisegiEgyseg>db</mennyisegiEgyseg>",
        "receipt tetel/mennyisegiEgyseg",
    ),
    (
        "<fizetoeszkoz>készpénz</fizetoeszkoz>",
        "receipt kifizetes/fizetoeszkoz",
    ),
    (
        "<totalossz><netto>100</netto><afa>27</afa><brutto>127</brutto></totalossz>",
        "osszegek/totalossz",
    ),
];

#[test]
fn each_xsd_required_receipt_element_is_optional_to_parse_and_required_by_parse_strict() {
    Document::parse_strict(RECEIPT_BATCH.as_bytes()).expect("the fixture conforms");
    Document::parse_strict(
        RECEIPT_BATCH
            .replacen("<adoszam>12345678-1-42</adoszam>", "", 1)
            .as_bytes(),
    )
    .expect("official batches omit adoszam");

    for (element, field) in XSD_REQUIRED_RECEIPT_ELEMENTS {
        assert!(
            RECEIPT_BATCH.contains(element),
            "fixture drifted: {element}"
        );
        let body = RECEIPT_BATCH.replacen(element, "", 1);
        receipts(&body).unwrap_or_else(|error| panic!("removing {element} must parse: {error}"));
        assert_eq!(
            strict_refusal(&body),
            format!("invalid document structure: missing required {field}"),
            "removing {element}"
        );
    }
}

/// The vendor's own annotated example (`szamla_example.xml`), read at run
/// time: the official corpus is not redistributed with the crate, so a
/// package built from crates.io has nothing to read here and the test
/// declares itself skipped rather than failing.
#[test]
fn vendor_example_parses_leniently_and_strictly() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/upstream/szamla_example.xml"
    );
    let body = match std::fs::read(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipped: {path} is not part of the published package ({error})");
            return;
        }
        Err(error) => panic!("reading {path}: {error}"),
    };

    let document = Document::parse_strict(&body).expect("the vendor example conforms");
    let Document::OutgoingInvoice(invoice) = document else {
        panic!("expected outgoing invoice");
    };
    assert_eq!(invoice.info.id, 123_456);
    assert_eq!(invoice.info.invoice_number, "2015-123");
    assert_eq!(invoice.supplier.name.as_deref(), Some("Példa Kft."));
    // The example leaves the buyer's name and address empty (present, but
    // carrying nothing), and its PDF element empty.
    assert_eq!(invoice.buyer.name.as_deref(), Some(""));
    assert!(invoice.pdf.is_none());
    assert_eq!(invoice.items.len(), 1);
    assert_eq!(invoice.payments[0].bank_transaction_id, Some(1234));
}
