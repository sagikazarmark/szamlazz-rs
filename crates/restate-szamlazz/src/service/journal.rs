//! Journal-compatibility fixtures: one pinned JSON document per variant of
//! every type the services journal as a `ctx.run` result, under
//! `tests/journal/<type>/`.
//!
//! An in-flight invocation replays the entries the *previous* deployment
//! wrote; an entry the new code cannot decode is a retryable SDK error that
//! kills the invocation once its attempts are spent, holding the order key
//! for the duration (ADR 0005, journal compatibility). So every journaled type
//! is **additive-only** — a new field defaults, a new variant may be added,
//! nothing is renamed, removed or retyped — and this module makes a violation
//! fail CI instead of a deploy:
//!
//! - the **generator** test pins every variant of every journaled type: the
//!   JSON the current code writes must equal `tests/journal/<type>/<variant>.json`
//!   byte for byte. A missing or differing fixture fails it with the
//!   instructions below; it never writes on its own.
//! - the **compatibility** test replays every fixture in every type's
//!   directory — the current ones and every shape archived before them —
//!   through the current type: each must decode, and re-encode to a superset
//!   of itself (so a renamed `Option` field that silently decodes to `None`
//!   is caught, not only a missing required one).
//!
//! # Regenerating
//!
//! Run the generator with `UPDATE_JOURNAL_FIXTURES=1`:
//!
//! ```sh
//! UPDATE_JOURNAL_FIXTURES=1 cargo test -p restate-szamlazz journal
//! ```
//!
//! It writes a missing fixture (a new variant or type), and for a fixture that
//! *differs* it keeps the committed shape beside the new one as
//! `<variant>.<n>.json` before writing `<variant>.json`. The archived shape
//! stays in the compatibility test forever, so an additive change (a defaulted
//! field) regenerates cleanly while a rename or removal keeps failing on the
//! archived file — the only way to make that pass is to delete the file, which
//! is the explicit acknowledgement that in-flight invocations of the previous
//! deployment will be killed on upgrade. Review every regenerated diff as a
//! contract change. A generator failure while the compatibility test passes
//! is a formatting change and not a journal break — a dependency upgrade that
//! prints a number or a date differently — and regenerates the same way.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rust_decimal::dec;
use serde_json::Value;
use szamlazz_agent::ops::invoice::{
    Buyer, CreateInvoice, CreatedInvoice, InvoiceCreationResult, InvoiceHeader, InvoiceKind,
};
use szamlazz_agent::ops::query_pdf::InvoiceSelector;
use szamlazz_agent::ops::query_xml::{InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::ops::taxpayer::{QueryTaxpayer, TaxpayerPrefix};
use szamlazz_agent::wire::{AgentRequest as _, RawResponse};
use szamlazz_agent::{Currency, InvoiceNumber, Language, LineItem, PaymentMethod, VatRate};

use crate::account::{Account, Endpoint};
use crate::config::{AccountMode, Defaults, Namespace, SellerConfig, SellerEmailConfig};
use crate::contract::QueryTaxpayerResponse;
use crate::gateway::{
    CreateOutcome, DeleteOutcome, LookupOutcome, ProbeOutcome, QueryOutcome, SetPaymentsOutcome,
    StornoLookupOutcome, StornoOutcome, TaxpayerOutcome,
};

use super::prologue::Resolution;
use super::support::Journaled;

/// The fixture root: `crates/restate-szamlazz/tests/journal`.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/journal")
}

/// `path` relative to the crate root, for messages.
fn rel(path: &Path) -> String {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or(path)
        .display()
        .to_string()
}

/// The pins of one journaled type: its directory under `tests/journal/`, the
/// JSON the current code writes for each variant, and how to replay a fixture
/// through the current type.
struct Pins {
    dir: &'static str,
    variants: Vec<Variant>,
    replay: fn(&str) -> Result<Value, serde_json::Error>,
}

/// One variant's pin: the fixture's file stem (`<stem>.json`) and the JSON
/// the current code writes for it.
struct Variant {
    stem: &'static str,
    json: String,
}

/// Pins `samples` as the fixtures of `T` under `tests/journal/<dir>/`, one
/// per variant, each filed under the stem `variant` gives it. `variant` is an
/// exhaustive `match` in every enum's pins, so a variant added to a journaled
/// enum fails to compile until it is named — and then the generator asks for
/// its fixture. Only a [`Journaled`] type can be pinned, and only a
/// `Journaled` type can be the result of a run: the trait is the link from
/// the `ctx.run` sites to this directory.
fn pins<T: Journaled>(dir: &'static str, samples: &[T], variant: fn(&T) -> &'static str) -> Pins {
    let mut variants: Vec<Variant> = Vec::with_capacity(samples.len());
    for sample in samples {
        let stem = variant(sample);
        assert!(
            !variants.iter().any(|seen| seen.stem == stem),
            "{dir}: two samples of variant {stem}"
        );
        let mut json = serde_json::to_string_pretty(sample).expect("journaled types serialize");
        json.push('\n');
        variants.push(Variant { stem, json });
    }
    Pins {
        dir,
        variants,
        replay: |text| {
            serde_json::from_str::<T>(text).and_then(|value| serde_json::to_value(value))
        },
    }
}

/// Every type the services journal as a `ctx.run` result, with a sample of
/// every variant.
fn registry() -> Vec<Pins> {
    vec![
        namespace_pins(),
        resolution_pins(),
        query_outcome_pins(),
        lookup_outcome_pins(),
        create_outcome_pins(),
        storno_lookup_outcome_pins(),
        storno_outcome_pins(),
        delete_outcome_pins(),
        set_payments_outcome_pins(),
        probe_outcome_pins(),
        taxpayer_outcome_pins(),
    ]
}

/// The namespace the prologue pins (`namespace` step).
fn namespace_pins() -> Pins {
    pins("namespace", &[namespace()], |_| "value")
}

/// The `account` step: the resolved account, or why the request names none.
fn resolution_pins() -> Pins {
    pins(
        "resolution",
        &[
            Resolution::Account(Box::new(account())),
            Resolution::Unscoped,
            Resolution::Unknown {
                scope: "acme-events".to_owned(),
            },
        ],
        |resolution| match resolution {
            Resolution::Account(_) => "account",
            Resolution::Unscoped => "unscoped",
            Resolution::Unknown { .. } => "unknown",
        },
    )
}

/// Every read of a document by number, external id or order number: the
/// verifies, the proforma link, the exclusivity checks, `get`, the storno
/// hint and `Szamlazz.Agent.query`.
fn query_outcome_pins() -> Pins {
    pins(
        "query-outcome",
        &[
            QueryOutcome::Found(document("SZ-1", false)),
            QueryOutcome::NotFound,
            QueryOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
            QueryOutcome::Api {
                code: API.code(),
                message: API.message(),
            },
        ],
        |outcome| match outcome {
            QueryOutcome::Found(_) => "found",
            QueryOutcome::NotFound => "not-found",
            QueryOutcome::CredentialsRejected { .. } => "credentials-rejected",
            QueryOutcome::Api { .. } => "api",
        },
    )
}

/// The lookup step of issuing (`lookup-{kind}`).
fn lookup_outcome_pins() -> Pins {
    pins(
        "lookup-outcome",
        &[
            LookupOutcome::Absent,
            LookupOutcome::Live(document("SZ-1", false)),
            LookupOutcome::Reversed {
                document: document("SZ-1", true),
                storno_number: Some("SS-1".to_owned()),
            },
            LookupOutcome::Collision(document("SZ-1", false)),
            LookupOutcome::Foreign(document("SZ-2", false)),
            LookupOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
            LookupOutcome::Api {
                code: API.code(),
                message: API.message(),
            },
        ],
        |outcome| match outcome {
            LookupOutcome::Absent => "absent",
            LookupOutcome::Live(_) => "live",
            LookupOutcome::Reversed { .. } => "reversed",
            LookupOutcome::Collision(_) => "collision",
            LookupOutcome::Foreign(_) => "foreign",
            LookupOutcome::CredentialsRejected { .. } => "credentials-rejected",
            LookupOutcome::Api { .. } => "api",
        },
    )
}

/// The create step of issuing (`create-{kind}`).
fn create_outcome_pins() -> Pins {
    pins(
        "create-outcome",
        &[
            CreateOutcome::Issued(creation_result()),
            CreateOutcome::Found(document("SZ-1", false)),
            CreateOutcome::Reversed(document("SZ-1", true)),
            CreateOutcome::LiveAgain(document("SZ-1", false)),
            CreateOutcome::Reconciled(document("SZ-1", false)),
            CreateOutcome::Collision(document("SZ-1", false)),
            CreateOutcome::DuplicateOrderNumber {
                code: "152".to_owned(),
                message: "A rendelésszám már szerepel egy számlán: SZ-2".to_owned(),
                existing_number: Some("SZ-2".to_owned()),
            },
            CreateOutcome::Rejected {
                code: REJECTED.code(),
                message: REJECTED.message(),
            },
            CreateOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
        ],
        |outcome| match outcome {
            CreateOutcome::Issued(_) => "issued",
            CreateOutcome::Found(_) => "found",
            CreateOutcome::Reversed(_) => "reversed",
            CreateOutcome::LiveAgain(_) => "live-again",
            CreateOutcome::Reconciled(_) => "reconciled",
            CreateOutcome::Collision(_) => "collision",
            CreateOutcome::DuplicateOrderNumber { .. } => "duplicate-order-number",
            CreateOutcome::Rejected { .. } => "rejected",
            CreateOutcome::CredentialsRejected { .. } => "credentials-rejected",
        },
    )
}

/// The storno lookup step (`lookup-storno-{number}`).
fn storno_lookup_outcome_pins() -> Pins {
    pins(
        "storno-lookup-outcome",
        &[
            StornoLookupOutcome::Absent,
            StornoLookupOutcome::AlreadyReversed {
                storno_number: "SS-1".to_owned(),
            },
            StornoLookupOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
            StornoLookupOutcome::Api {
                code: API.code(),
                message: API.message(),
            },
        ],
        |outcome| match outcome {
            StornoLookupOutcome::Absent => "absent",
            StornoLookupOutcome::AlreadyReversed { .. } => "already-reversed",
            StornoLookupOutcome::CredentialsRejected { .. } => "credentials-rejected",
            StornoLookupOutcome::Api { .. } => "api",
        },
    )
}

/// The storno step (`storno-{number}`).
fn storno_outcome_pins() -> Pins {
    pins(
        "storno-outcome",
        &[
            StornoOutcome::Reversed(created_invoice()),
            StornoOutcome::AlreadyReversed {
                storno_number: "SS-1".to_owned(),
            },
            StornoOutcome::NotStornoable,
            StornoOutcome::Rejected {
                code: "221".to_owned(),
                message: "A számlához helyesbítő számla tartozik.".to_owned(),
            },
            StornoOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
        ],
        |outcome| match outcome {
            StornoOutcome::Reversed(_) => "reversed",
            StornoOutcome::AlreadyReversed { .. } => "already-reversed",
            StornoOutcome::NotStornoable => "not-stornoable",
            StornoOutcome::Rejected { .. } => "rejected",
            StornoOutcome::CredentialsRejected { .. } => "credentials-rejected",
        },
    )
}

/// The proforma deletion (`delete-proforma-{number}`).
fn delete_outcome_pins() -> Pins {
    pins(
        "delete-outcome",
        &[
            DeleteOutcome::Deleted,
            DeleteOutcome::AlreadyGone,
            DeleteOutcome::Rejected {
                code: REJECTED.code(),
                message: REJECTED.message(),
            },
            DeleteOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
            DeleteOutcome::Transport(TRANSPORT.to_owned()),
        ],
        |outcome| match outcome {
            DeleteOutcome::Deleted => "deleted",
            DeleteOutcome::AlreadyGone => "already-gone",
            DeleteOutcome::Rejected { .. } => "rejected",
            DeleteOutcome::CredentialsRejected { .. } => "credentials-rejected",
            DeleteOutcome::Transport(_) => "transport",
        },
    )
}

/// The credit-entry registration (`set-payments-{number}`).
fn set_payments_outcome_pins() -> Pins {
    pins(
        "set-payments-outcome",
        &[
            SetPaymentsOutcome::Done {
                outstanding: Some(dec!(0)),
                gross: Some(dec!(12700)),
            },
            SetPaymentsOutcome::Rejected {
                code: "463".to_owned(),
                message: "Sztornózott számlára nem rögzíthető kifizetés.".to_owned(),
            },
            SetPaymentsOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
            SetPaymentsOutcome::Transport(TRANSPORT.to_owned()),
        ],
        |outcome| match outcome {
            SetPaymentsOutcome::Done { .. } => "done",
            SetPaymentsOutcome::Rejected { .. } => "rejected",
            SetPaymentsOutcome::CredentialsRejected { .. } => "credentials-rejected",
            SetPaymentsOutcome::Transport(_) => "transport",
        },
    )
}

/// The `check_account` probe (`probe`).
fn probe_outcome_pins() -> Pins {
    pins(
        "probe-outcome",
        &[
            ProbeOutcome::Accepted,
            ProbeOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
        ],
        |outcome| match outcome {
            ProbeOutcome::Accepted => "accepted",
            ProbeOutcome::CredentialsRejected { .. } => "credentials-rejected",
        },
    )
}

/// The taxpayer lookup (`taxpayer-{prefix}`).
fn taxpayer_outcome_pins() -> Pins {
    pins(
        "taxpayer-outcome",
        &[
            TaxpayerOutcome::Found(taxpayer()),
            TaxpayerOutcome::CredentialsRejected {
                code: CREDENTIALS.code(),
                message: CREDENTIALS.message(),
            },
            TaxpayerOutcome::Api {
                code: NAV.code(),
                message: NAV.message(),
            },
        ],
        |outcome| match outcome {
            TaxpayerOutcome::Found(_) => "found",
            TaxpayerOutcome::CredentialsRejected { .. } => "credentials-rejected",
            TaxpayerOutcome::Api { .. } => "api",
        },
    )
}

/// A szamlazz.hu code and message, as the code-and-message variants carry
/// them.
struct Code {
    code: &'static str,
    message: &'static str,
}

impl Code {
    fn code(&self) -> String {
        self.code.to_owned()
    }

    fn message(&self) -> String {
        self.message.to_owned()
    }
}

/// The credential code every `CredentialsRejected` sample carries.
const CREDENTIALS: Code = Code {
    code: "3",
    message: "Sikertelen bejelentkezés.",
};

/// The other API code every `Api` sample carries.
const API: Code = Code {
    code: "57",
    message: "Hibás XML.",
};

/// The refusal the `Rejected` samples carry.
const REJECTED: Code = Code {
    code: "202",
    message: "Nem regisztrált számlaszám előtag: ACME",
};

/// The NAV code the taxpayer lookup's `Api` sample carries — `funcCode ERROR`
/// relayed by szamlazz.hu.
const NAV: Code = Code {
    code: "OPERATION_FAILED",
    message: "Az adatszolgáltatás jelenleg nem elérhető.",
};

/// The failure text of the `Transport` samples of the two write steps that
/// keep one.
const TRANSPORT: &str = "error sending request for url (https://www.szamlazz.hu/szamla/)";

/// The reply of a create, with every field the `xmlszamlavalasz` body and
/// the `szlahu_id` header can carry: `SZ-1`, its totals, the buyer's account
/// URL and a PDF. Parsed the way the gateway parses it, since the agent's
/// types are `#[non_exhaustive]`.
fn creation_result() -> InvoiceCreationResult {
    let create = CreateInvoice::new(
        InvoiceKind::invoice(),
        InvoiceHeader::new(
            jiff::civil::date(2026, 7, 4),
            jiff::civil::date(2026, 7, 12),
            PaymentMethod::Transfer,
            Currency::HUF,
            Language::Hungarian,
        ),
        Buyer::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23."),
        vec![LineItem::calculated_for_currency(
            "Eladó izé",
            dec!(1),
            "db",
            dec!(10000),
            VatRate::percent(27),
            &Currency::HUF,
        )],
    );
    create
        .parse(&reply("SZ-1", "10000", "12700", "12700"))
        .expect("xmlszamlavalasz parses")
}

/// The reply of a storno of `SZ-1`: the storno invoice `SS-1` with negative
/// totals, parsed the way the gateway parses it.
fn created_invoice() -> CreatedInvoice {
    StornoInvoice::new("SZ-1")
        .parse(&reply("SS-1", "-10000", "-12700", "0"))
        .expect("xmlszamlavalasz parses")
}

/// NAV's record of a valid taxpayer with every field the `xmltaxpayer`
/// response can carry — the registered name, the tax number detail and one
/// detailed and one simple address — projected onto the crate-owned response
/// the way the gateway projects it, since the response types are
/// `#[non_exhaustive]`.
fn taxpayer() -> QueryTaxpayerResponse {
    let prefix: TaxpayerPrefix = "12345678".parse().expect("prefix");
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api" xmlns:d="http://schemas.nav.gov.hu/OSA/2.0/data">
  <result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity>
  <taxpayerData><taxpayerName>ACME KFT.</taxpayerName>
    <taxNumberDetail><d:taxpayerId>12345678</d:taxpayerId><d:vatCode>2</d:vatCode></taxNumberDetail>
    <taxpayerAddressList>
      <taxpayerAddressItem><taxpayerAddressType>HQ</taxpayerAddressType><taxpayerAddress>
        <d:countryCode>HU</d:countryCode><d:region>Budapest</d:region><d:postalCode>1111</d:postalCode><d:city>BUDAPEST</d:city>
        <d:streetName>FŐ</d:streetName><d:publicPlaceCategory>UTCA</d:publicPlaceCategory><d:number>1.</d:number>
        <d:building>A</d:building><d:staircase>B</d:staircase><d:floor>3</d:floor><d:door>12</d:door><d:lotNumber>4242/1</d:lotNumber>
      </taxpayerAddress></taxpayerAddressItem>
      <taxpayerAddressItem><taxpayerAddressType>SITE</taxpayerAddressType><taxpayerAddress>
        <d:countryCode>HU</d:countryCode><d:postalCode>2030</d:postalCode><d:city>ÉRD</d:city>
        <d:additionalAddressDetail>Tárnoki út 23.</d:additionalAddressDetail>
      </taxpayerAddress></taxpayerAddressItem>
    </taxpayerAddressList>
  </taxpayerData>
</QueryTaxpayerResponse>"#;
    QueryTaxpayer::from(prefix)
        .parse(&RawResponse::new::<&str, &str>([], xml.as_bytes().to_vec()))
        .expect("xmltaxpayer parses")
        .into()
}

/// A successful `xmlszamlavalasz` reply with the `szlahu_id` header and a PDF.
fn reply(number: &str, net: &str, gross: &str, outstanding: &str) -> RawResponse {
    RawResponse::new(
        [("szlahu_id", "924307747")],
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>{number}</szamlaszam><szamlanetto>{net}</szamlanetto><szamlabrutto>{gross}</szamlabrutto><kintlevoseg>{outstanding}</kintlevoseg><vevoifiokurl>https://www.szamlazz.hu/szamla/fiok/example</vevoifiokurl><pdf>JVBERi0=</pdf></xmlszamlavalasz>"#
        )
        .into_bytes(),
    )
}

/// A queried invoice (`SZ`) with every element szamlazz.hu's `szamla` XML can
/// carry, so that a rename anywhere in [`InvoiceDocument`] and its nested
/// types is caught: a test-account e-invoice of `ORD-1` from supplier 972720
/// with postal addresses, ledger blocks, a financial item, labels, two
/// payments and a PDF. Parsed the way the gateway parses a query answer, since
/// the agent's types are `#[non_exhaustive]`. The kind does not change the
/// shape, so one kind is enough.
fn document(number: &str, reversed: bool) -> Box<InvoiceDocument> {
    let sztornozott = if reversed {
        "<sztornozott>true</sztornozott>"
    } else {
        ""
    };
    let xml = format!(
        r#"<szamla xmlns="http://www.szamlazz.hu/szamla">
          <szallito><id>972720</id><nev>Acme Kft.</nev><cim><orszag>Magyarország</orszag><irsz>1111</irsz><telepules>Budapest</telepules><cim>Fő u. 1.</cim></cim>
            <postacim><orszag>Magyarország</orszag><irsz>1112</irsz><telepules>Budapest</telepules><cim>Pf. 12.</cim></postacim>
            <adoszam>12345678-2-42</adoszam><csoportazonosito>17777777-5-44</csoportazonosito><adoszameu>HU12345678</adoszameu>
            <bank><nev>Test Bank</nev><bankszamla>11111111-22222222-33333333</bankszamla></bank></szallito>
          <alap><id>924307747</id><szamlaszam>{number}</szamlaszam><gazdEsemAzon>924307700</gazdEsemAzon><forras>34</forras>
            <iktatoszam>IKT-2026-1</iktatoszam><tipus>SZ</tipus><eszamla>2</eszamla><hivszamlaszam>SZ-0</hivszamlaszam>
            <hivdijbekszam>D-1</hivdijbekszam><kelt>2026-07-04</kelt><telj>2026-07-04</telj><fizh>2026-07-12</fizh>
            <fizmod>Átutalás</fizmod><fizmodunified>transfer</fizmodunified><keszpenz>false</keszpenz>
            <rendelesszam>ORD-1</rendelesszam><nyelv>hu</nyelv><devizanem>HUF</devizanem><devizabank>MNB</devizabank>
            <devizaarf>0</devizaarf><megjegyzes>thanks</megjegyzes><afatipus>TEHK</afatipus><penzforg>false</penzforg><kata>false</kata>
            <katafokonyv>false</katafokonyv><email>buyer@example.com</email><teszt>true</teszt>{sztornozott}</alap>
          <vevo><id>4242</id><nev>Kovács Bt.</nev><azonosito>BUYER-1</azonosito><cim><orszag>Magyarország</orszag><irsz>2030</irsz><telepules>Érd</telepules><cim>Tárnoki út 23.</cim></cim>
            <postacim><nev>Kovács Bt. (levelezés)</nev><orszag>Magyarország</orszag><irsz>2031</irsz><telepules>Érd</telepules><cim>Pf. 3.</cim></postacim>
            <email>buyer@example.com</email><adoszam>87654321-1-42</adoszam><csoportazonosito>18888888-5-44</csoportazonosito>
            <adoszameu>HU87654321</adoszameu><lokacio>1</lokacio><privatePersonIndicator>false</privatePersonIndicator>
            <fokonyv><vevo>311</vevo><vevoazon>B-7</vevoazon><datum>2026-07-04</datum><folyamatostelj>false</folyamatostelj>
              <elszDatTol>2026-07-01</elszDatTol><elszDatIg>2026-07-31</elszDatIg></fokonyv></vevo>
          <tetelek><tetel><nev>Eladó izé</nev><azonosito>SKU-1</azonosito><mennyiseg>1</mennyiseg><mennyisegiegyseg>db</mennyisegiegyseg>
            <nettoegysegar>10000</nettoegysegar><afatipus>TEHK</afatipus><afakulcs>27</afakulcs><netto>10000</netto><arresafaalap>0</arresafaalap><afa>2700</afa><brutto>12700</brutto>
            <megjegyzes>row</megjegyzes><sztetordering>1</sztetordering><fokonyv><arbevetel>911</arbevetel><afa>467</afa><gazdasagiesemeny>SALE</gazdasagiesemeny>
              <gazdasagiesemenyafa>VAT</gazdasagiesemenyafa><elszdattol>2026-07-01</elszdattol><elszdatig>2026-07-31</elszdatig></fokonyv></tetel></tetelek>
          <qutetek><qutet><nev>Kezelési költség</nev><afatipus>TEHK</afatipus><afakulcs>27</afakulcs><netto>0</netto><afa>0</afa><brutto>0</brutto>
            <elszdattol>2026-07-01</elszdattol><elszdatig>2026-07-31</elszdatig><afalevon>100</afalevon><cimkek><cimke>fee</cimke></cimkek></qutet></qutetek>
          <cimkek><cimke>webshop</cimke></cimkek>
          <osszegek><afakulcsossz><afatipus>TEHK</afatipus><afakulcs>27</afakulcs><netto>10000</netto><afa>2700</afa><brutto>12700</brutto></afakulcsossz>
            <totalossz><netto>10000</netto><afa>2700</afa><brutto>12700</brutto></totalossz></osszegek>
          <kifizetesek><kifizetes><datum>2026-07-04</datum><jogcim>transfer</jogcim><osszeg>5000</osszeg><megjegyzes>first</megjegyzes>
            <bankszamlaszam>11111111-22222222-33333333</bankszamlaszam><banktranzid>99</banktranzid><devizaarf>1</devizaarf></kifizetes>
            <kifizetes><datum>2026-07-05</datum><jogcim>transfer</jogcim><osszeg>7700</osszeg><megjegyzes>rest</megjegyzes>
            <bankszamlaszam>11111111-22222222-33333333</bankszamlaszam><banktranzid>100</banktranzid><devizaarf>1</devizaarf></kifizetes></kifizetesek>
          <pdf>JVBERi0=</pdf></szamla>"#
    );
    Box::new(
        QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(number)))
            .parse(&RawResponse::new::<&str, &str>([], xml.into_bytes()))
            .expect("szamla XML parses"),
    )
}

/// The namespace the prologue pins (`namespace` step).
fn namespace() -> Namespace {
    "acct".parse().expect("namespace")
}

/// The account the `account` step journals, with every optional field set so
/// that a rename anywhere in [`Account`], [`Defaults`], [`SellerConfig`] or
/// [`SellerEmailConfig`] is caught. Never the agent key — the type cannot
/// carry it.
fn account() -> Account {
    let mut account = Account::new("acme", "acme-credentials");
    account.mode = AccountMode::Test;
    account.supplier_id = Some(972_720);
    account.endpoint = Endpoint::parse("https://szamlazz.example.test/szamla/").expect("endpoint");
    account.defaults = Defaults {
        e_invoice: true,
        language: "en".to_owned(),
        currency: "EUR".to_owned(),
        exchange_rate_bank: "MNB".to_owned(),
        template: Some("SzlaMost".to_owned()),
        send_email: Some(false),
        number_prefix: Some("ACME".to_owned()),
        extra_logo: Some("logo.png".to_owned()),
        aggregator: Some("aggregator".to_owned()),
        guardian: Some(true),
    };
    account.seller = SellerConfig {
        bank: Some("Test Bank".to_owned()),
        bank_account: Some("11111111-22222222-33333333".to_owned()),
        signer_name: Some("Signer".to_owned()),
        email: SellerEmailConfig {
            reply_to: Some("billing@acme.test".to_owned()),
            subject: Some("Your invoice".to_owned()),
            body: Some("[b]Thank you[/b]".to_owned()),
        },
    };
    account
}

/// Whether `fixture` is covered by `current`: every object key of the fixture
/// is present in `current` with a covered value, arrays match element for
/// element, scalars are equal. Keys only `current` has — fields added since
/// the fixture was written — are allowed; that is what "additive" means.
fn is_covered_by(fixture: &Value, current: &Value) -> bool {
    match (fixture, current) {
        (Value::Object(fixture), Value::Object(current)) => fixture.iter().all(|(key, value)| {
            current
                .get(key)
                .is_some_and(|seen| is_covered_by(value, seen))
        }),
        (Value::Array(fixture), Value::Array(current)) => {
            fixture.len() == current.len()
                && fixture
                    .iter()
                    .zip(current)
                    .all(|(value, seen)| is_covered_by(value, seen))
        }
        (fixture, current) => fixture == current,
    }
}

/// The first line on which `committed` and `current` differ, for the
/// generator's message.
fn describe_first_difference(committed: &str, current: &str) -> String {
    let mut committed_lines = committed.lines();
    let mut current_lines = current.lines();
    let mut line = 1;
    loop {
        match (committed_lines.next(), current_lines.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (Some(a), Some(b)) => {
                return format!(
                    "line {line}: fixture `{}`, current `{}`",
                    a.trim(),
                    b.trim()
                );
            }
            (Some(a), None) => return format!("line {line}: fixture `{}`, current ends", a.trim()),
            (None, Some(b)) => return format!("line {line}: fixture ends, current `{}`", b.trim()),
            (None, None) => return "identical".to_owned(),
        }
    }
}

/// The instructions every generator failure ends with.
const HOW_TO_REGENERATE: &str = "\
A journaled shape changed. If the change is additive — a new variant, or a new field with a serde default — \
regenerate with

    UPDATE_JOURNAL_FIXTURES=1 cargo test -p restate-szamlazz journal

which writes the missing fixtures and keeps a differing one beside the new shape as <variant>.<n>.json, \
then run the tests again and review the diff as a contract change. If a field or variant was renamed, \
removed or retyped, every in-flight invocation of the previous deployment will be killed on upgrade: \
do not regenerate; keep the old name (see the gateway module docs and ADR 0005).";

/// The generator: the JSON the current code writes for every variant of every
/// journaled type equals its committed fixture byte for byte. Never writes
/// unless `UPDATE_JOURNAL_FIXTURES=1` (see the module docs).
#[test]
fn every_variant_of_every_journaled_type_is_pinned() {
    let mode = Mode::from_env();
    let mut problems = Vec::new();
    for pins in registry() {
        for Variant {
            stem,
            json: current,
        } in &pins.variants
        {
            let path = fixtures().join(pins.dir).join(format!("{stem}.json"));
            match check(&path, current, mode).expect("fixture io") {
                Verdict::Pinned => {}
                Verdict::Written => eprintln!("wrote {}", rel(&path)),
                Verdict::Archived(archive) => eprintln!(
                    "kept the committed shape of {} as {}; wrote the new shape",
                    rel(&path),
                    rel(&archive)
                ),
                Verdict::Missing => problems.push(format!("{}: missing", rel(&path))),
                Verdict::Differs => {
                    let committed = fs::read_to_string(&path).expect("fixture io");
                    problems.push(format!(
                        "{}: differs from what the current code writes ({})",
                        rel(&path),
                        describe_first_difference(&committed, current)
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "journaled shapes are not pinned:\n  {}\n\n{HOW_TO_REGENERATE}",
        problems.join("\n  ")
    );
}

/// The compatibility test: every fixture under every journaled type's
/// directory — the current shape and every shape archived before it —
/// decodes through the current type and re-encodes to a superset of itself.
/// What a replay of an in-flight invocation needs from the new code.
#[test]
fn every_pinned_fixture_replays_through_the_current_types() {
    let registry = registry();

    let claimed: BTreeSet<&str> = registry.iter().map(|pins| pins.dir).collect();
    let unclaimed: Vec<String> = fs::read_dir(fixtures())
        .expect("tests/journal exists")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| !claimed.contains(name.as_str()))
        .collect();
    assert!(
        unclaimed.is_empty(),
        "tests/journal/ holds fixtures of no journaled type: {unclaimed:?} — a type that is no longer \
         journaled is removed knowingly, together with its fixtures"
    );

    let mut failures = Vec::new();
    let mut replayed = 0;
    for pins in &registry {
        let dir = fixtures().join(pins.dir);
        let mut files: Vec<PathBuf> = match fs::read_dir(&dir) {
            Ok(entries) => entries
                .map(|entry| entry.expect("entry").path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect(),
            Err(_) => Vec::new(),
        };
        files.sort();
        if files.is_empty() {
            failures.push(format!("{}: no fixtures", rel(&dir)));
        }
        for path in files {
            let text = fs::read_to_string(&path).expect("fixture io");
            let fixture: Value = match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!("{}: not JSON: {error}", rel(&path)));
                    continue;
                }
            };
            match (pins.replay)(&text) {
                Err(error) => failures.push(format!("{}: does not decode: {error}", rel(&path))),
                Ok(current) if !is_covered_by(&fixture, &current) => failures.push(format!(
                    "{}: decodes, but re-encodes without part of the fixture — a field was renamed \
                     or retyped and decoded to its default",
                    rel(&path)
                )),
                Ok(_) => replayed += 1,
            }
        }
    }
    assert!(
        failures.is_empty(),
        "journal entries of the previous deployment would not replay:\n  {}\n\n\
         A renamed, removed or retyped field or variant kills every in-flight invocation on upgrade. \
         Keep the old name (a new field defaults, a new variant is added; nothing is renamed or removed) — \
         see the gateway module docs and ADR 0005.",
        failures.join("\n  ")
    );
    assert!(replayed > 0, "no fixture was replayed");
}

/// How the generator runs: `Verify` never writes; `Update`
/// (`UPDATE_JOURNAL_FIXTURES=1`) writes missing fixtures and archives
/// differing ones before writing the new shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Verify,
    Update,
}

impl Mode {
    /// The mode the environment asks for.
    fn from_env() -> Self {
        match std::env::var_os(UPDATE_VAR) {
            Some(value) if !value.is_empty() && value != "0" => Self::Update,
            _ => Self::Verify,
        }
    }
}

/// The environment variable that switches the generator to [`Mode::Update`].
const UPDATE_VAR: &str = "UPDATE_JOURNAL_FIXTURES";

/// The generator's verdict on one variant's fixture file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    /// The committed fixture is byte for byte what the current code writes.
    Pinned,
    /// There was no fixture; `Update` wrote it.
    Written,
    /// The fixture differed; `Update` kept it under the given path and wrote
    /// the new shape in its place.
    Archived(PathBuf),
    /// There is no fixture (`Verify`).
    Missing,
    /// The fixture differs from what the current code writes (`Verify`).
    Differs,
}

/// Compares the fixture at `path` with `current`, the JSON the current code
/// writes for the variant, and — under [`Mode::Update`] — writes or archives.
fn check(path: &Path, current: &str, mode: Mode) -> io::Result<Verdict> {
    let committed = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    match (committed, mode) {
        (Some(committed), _) if committed == current => Ok(Verdict::Pinned),
        (Some(_), Mode::Verify) => Ok(Verdict::Differs),
        (None, Mode::Verify) => Ok(Verdict::Missing),
        (None, Mode::Update) => {
            write(path, current)?;
            Ok(Verdict::Written)
        }
        (Some(committed), Mode::Update) => {
            let archive = archive_path(path)?;
            write(&archive, &committed)?;
            write(path, current)?;
            Ok(Verdict::Archived(archive))
        }
    }
}

/// The first free `<variant>.<n>.json` beside `path` (`<variant>.json`).
fn archive_path(path: &Path) -> io::Result<PathBuf> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| io::Error::other(format!("not a fixture path: {}", path.display())))?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    (1..=u32::MAX)
        .map(|n| dir.join(format!("{stem}.{n}.json")))
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| io::Error::other("no free archive number"))
}

/// Writes `text` to `path`, creating the type's directory on first use.
fn write(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, text)
}

/// The harness itself: the superset check, and the verify / update / archive
/// behaviour of [`check`] on a scratch directory.
mod harness_tests {
    use serde_json::json;

    use super::*;

    /// The superset check behind the compatibility test: what "additive"
    /// admits and what it refuses.
    #[test]
    fn a_fixture_is_covered_by_a_re_encoding_that_adds_fields_but_not_by_one_that_drops_or_changes_them()
     {
        let fixture =
            json!({"Live": {"info": {"number": "SZ-1", "tags": ["a", "b"], "pdf": null}}});

        assert!(is_covered_by(&fixture, &fixture), "identical");
        assert!(
            is_covered_by(
                &fixture,
                &json!({"Live": {"info": {"number": "SZ-1", "tags": ["a", "b"], "pdf": null, "added": 1}}})
            ),
            "a field added since the fixture was written is additive"
        );
        assert!(
            !is_covered_by(
                &fixture,
                &json!({"Live": {"info": {"tags": ["a", "b"], "pdf": null}}})
            ),
            "a dropped field is not"
        );
        assert!(
            !is_covered_by(
                &fixture,
                &json!({"Live": {"info": {"number": "SZ-2", "tags": ["a", "b"], "pdf": null}}})
            ),
            "a changed scalar is not"
        );
        assert!(
            !is_covered_by(
                &fixture,
                &json!({"Live": {"info": {"number": "SZ-1", "tags": ["a"], "pdf": null}}})
            ),
            "an array that lost an element is not"
        );
        assert!(
            !is_covered_by(
                &fixture,
                &json!({"Live": {"info": {"number": "SZ-1", "tags": ["a", "b"], "pdf": "x"}}})
            ),
            "a null that became a value is not"
        );
        assert!(
            !is_covered_by(&fixture, &json!({"Reversed": {}})),
            "another variant is not"
        );
        assert!(
            !is_covered_by(&json!("acct"), &json!("other")),
            "scalars compare"
        );
    }

    /// A fresh, empty directory under the system temp dir for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "restate-szamlazz-journal-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// The file names in `dir`, sorted.
    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// The default run never writes; `UPDATE_JOURNAL_FIXTURES=1` writes a
    /// fixture that does not exist yet.
    #[test]
    fn a_missing_fixture_is_missing_under_verify_and_written_under_update() {
        let dir = scratch("missing");
        let path = dir.join("absent.json");

        assert_eq!(
            check(&path, "\"acct\"\n", Mode::Verify).expect("io"),
            Verdict::Missing
        );
        assert!(!path.exists(), "verify never writes");

        assert_eq!(
            check(&path, "\"acct\"\n", Mode::Update).expect("io"),
            Verdict::Written
        );
        assert_eq!(fs::read_to_string(&path).expect("written"), "\"acct\"\n");
        assert_eq!(
            check(&path, "\"acct\"\n", Mode::Verify).expect("io"),
            Verdict::Pinned
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A changed shape is archived beside the new one, never overwritten, and
    /// every regeneration takes the next free archive number.
    #[test]
    fn a_differing_fixture_is_kept_beside_the_new_one_under_update_and_never_overwritten() {
        let dir = scratch("differs");
        let path = dir.join("live.json");
        fs::write(&path, "{\"v\": 1}\n").expect("seed");

        assert_eq!(
            check(&path, "{\"v\": 2}\n", Mode::Verify).expect("io"),
            Verdict::Differs
        );
        assert_eq!(fs::read_to_string(&path).expect("kept"), "{\"v\": 1}\n");

        assert_eq!(
            check(&path, "{\"v\": 2}\n", Mode::Update).expect("io"),
            Verdict::Archived(dir.join("live.1.json"))
        );
        assert_eq!(files(&dir), ["live.1.json", "live.json"]);
        assert_eq!(
            fs::read_to_string(dir.join("live.1.json")).expect("archived"),
            "{\"v\": 1}\n"
        );
        assert_eq!(fs::read_to_string(&path).expect("new"), "{\"v\": 2}\n");

        // A second regeneration takes the next free number; the first archive
        // is untouched.
        assert_eq!(
            check(&path, "{\"v\": 3}\n", Mode::Update).expect("io"),
            Verdict::Archived(dir.join("live.2.json"))
        );
        assert_eq!(files(&dir), ["live.1.json", "live.2.json", "live.json"]);
        assert_eq!(
            fs::read_to_string(dir.join("live.1.json")).expect("archived"),
            "{\"v\": 1}\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("live.2.json")).expect("archived"),
            "{\"v\": 2}\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
