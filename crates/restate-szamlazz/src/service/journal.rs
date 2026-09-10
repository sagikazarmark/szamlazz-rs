//! What a `ctx.run` result may hold: the three checks on the journaled types.
//!
//! A journaled type is a type the services write as the result of a
//! `ctx.run`: the `namespace` step's [`Namespace`], the `account` step's
//! [`Resolution`] and the gateway's outcome enums. Restate replays an entry
//! only on the deployment that wrote it (deployments are immutable, ADR
//! 0009), so no entry is ever decoded by a later release's code and the
//! types carry no cross-version compatibility contract. What is checked here
//! is what the entry holds, since the Restate UI shows every entry for the
//! retention period:
//!
//! - every sample of every journaled type **round-trips** through serde
//!   (an ordinary test of the derives; `ctx.run` decodes what it wrote on a
//!   replay within one deployment);
//! - no entry carries the **agent key**, checked with a sentinel key in play
//!   on the one path that has key-adjacent input (a transport failure's text
//!   from a gateway holding the credentials);
//! - no entry carries the **document body**: the seller block, the buyer
//!   block, the line items, the financial items, the labels or the PDF of a
//!   queried document, none of which a handler reads (the reason the
//!   outcomes carry [`FoundDocument`] / [`IssuedDocument`] rather than the
//!   agent crate's types).
//!
//! The samples are built through the agent crate's parsers from wire XML
//! and projected as the gateway projects them, since the agent's types are
//! `#[non_exhaustive]`. Complete by mechanism, because the scan is a privacy
//! invariant and not a convention: only a [`Journaled`] type can be a
//! `ctx.run` result (the sealed marker, implemented through one list in
//! `support`), [`every_journaled_type_is_sampled`] holds [`entries`] to that
//! list, and each enum's samples are checked against its [`variants!`] list,
//! an exhaustive `match`, so a new variant fails to compile until listed and
//! fails the scan by name until sampled.

use std::collections::BTreeSet;
use std::fmt::Debug;

use rust_decimal::dec;
use serde_json::Value;
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::ops::query_xml::{InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::ops::taxpayer::{QueryTaxpayer, TaxpayerPrefix};
use szamlazz_agent::wire::{AgentRequest as _, RawResponse};
use szamlazz_agent::{
    Credentials, Currency, InvoiceNumber, InvoiceSelector, Language, LineItem, PaymentMethod,
    VatRate,
};

use crate::account::{
    Account, AccountResolver as _, CredentialStore as _, Defaults, Endpoint, SellerConfig,
    SellerEmailConfig, StaticConfig, StaticResolver,
};
use crate::contract::{
    CreditEntryInput, PaymentMethod as ContractPaymentMethod, QueryTaxpayerResponse,
};
use crate::gateway::{
    CreateOutcome, DeleteOutcome, FoundDocument, IssuedDocument, LookupOutcome, OwnershipOutcome,
    ProbeOutcome, QueryOutcome, Rejection, SetCreditEntriesOutcome, StornoLookupOutcome,
    StornoOutcome, SzamlazzAnswer, TaxpayerOutcome, Unanswered,
};
use crate::identity::{Namespace, OrderKey};
use crate::test_support::{Doc, open_gateway};

use super::prologue::Resolution;
use super::support::{Journaled, journaled_types};

/// One serialised sample of a journaled type, labelled for messages.
struct Entry {
    type_name: &'static str,
    label: String,
    json: String,
}

/// The variants of a journaled enum: the name of a sample's variant, by an
/// exhaustive `match`, and every variant's name, so [`entries_of`] can name a
/// variant that has no sample.
struct Variants<T> {
    name_of: fn(&T) -> &'static str,
    all: &'static [&'static str],
}

/// The [`Variants`] of an enum, one arm per variant. `name_of` is exhaustive,
/// so a variant added to the enum fails to compile until it is listed here.
macro_rules! variants {
    ($ty:ident { $($variant:ident $( ( $($tuple:tt)* ) )? $( { $($fields:tt)* } )?),+ $(,)? }) => {
        Variants::<$ty> {
            name_of: |value| match value {
                $($ty::$variant $( ( $($tuple)* ) )? $( { $($fields)* } )? => stringify!($variant),)+
            },
            all: &[$(stringify!($variant)),+],
        }
    };
}

/// The [`Variants`] of a type with one shape (a newtype, a struct).
fn single<T>() -> Variants<T> {
    Variants {
        name_of: |_| "value",
        all: &["value"],
    }
}

/// Serialises `samples` of one journaled type, asserting each round-trips
/// (decodes to a value that re-encodes identically) and that every variant
/// `variants` lists has a sample.
fn entries_of<T>(samples: Vec<T>, variants: &Variants<T>) -> Vec<Entry>
where
    T: Journaled + Debug,
{
    let type_name = std::any::type_name::<T>();
    let entries: Vec<Entry> = samples
        .into_iter()
        .map(|sample| {
            let label = format!("{type_name}::{}", (variants.name_of)(&sample));
            let json = serde_json::to_string_pretty(&sample).expect("journaled types serialise");
            let replayed: T =
                serde_json::from_str(&json).unwrap_or_else(|error| panic!("{label}: {error}"));
            let again = serde_json::to_string_pretty(&replayed).expect("serialises again");
            assert_eq!(json, again, "{label}: does not round-trip: {sample:?}");
            Entry {
                type_name,
                label,
                json,
            }
        })
        .collect();
    let unsampled: Vec<&str> = variants
        .all
        .iter()
        .copied()
        .filter(|name| {
            !entries
                .iter()
                .any(|entry| entry.label.ends_with(&format!("::{name}")))
        })
        .collect();
    assert!(
        unsampled.is_empty(),
        "{type_name}: no sample for variant(s) {unsampled:?}; every variant of a journaled type is scanned"
    );
    entries
}

/// A sample of every variant of every journaled type, serialised, each
/// asserted to round-trip on the way.
#[allow(
    clippy::too_many_lines,
    reason = "one table of samples; a variant per line is what makes it reviewable"
)]
fn entries() -> Vec<Entry> {
    let mut all = Vec::new();
    all.extend(entries_of(vec![namespace()], &single()));
    all.extend(entries_of(
        vec![
            Resolution::Account(Box::new(account())),
            Resolution::Unscoped,
            Resolution::Unknown {
                scope: "acme-events".to_owned(),
            },
        ],
        &variants!(Resolution { Account(_), Unscoped, Unknown { .. } }),
    ));
    all.extend(entries_of(
        vec![
            QueryOutcome::Found(document("SZ-1", false)),
            QueryOutcome::NotFound,
            QueryOutcome::CredentialsRejected(CREDENTIALS.answer()),
            QueryOutcome::Api(API.answer()),
        ],
        &variants!(QueryOutcome { Found(_), NotFound, CredentialsRejected(_), Api(_) }),
    ));
    all.extend(entries_of(vec![
            OwnershipOutcome::Absent,
            OwnershipOutcome::Live(document("SZ-1", false)),
            OwnershipOutcome::Reversed(document("SZ-1", true)),
            OwnershipOutcome::Collision(document("SZ-2", false)),
            OwnershipOutcome::CredentialsRejected(CREDENTIALS.answer()),
            OwnershipOutcome::Api(API.answer()),
        ], &variants!(OwnershipOutcome { Absent, Live(_), Reversed(_), Collision(_), CredentialsRejected(_), Api(_) })));
    all.extend(entries_of(vec![
            LookupOutcome::Absent,
            LookupOutcome::Live(document("SZ-1", false)),
            LookupOutcome::Reversed {
                document: document("SZ-1", true),
                storno_number: Some("SS-1".to_owned()),
            },
            LookupOutcome::Collision(document("SZ-1", false)),
            LookupOutcome::Foreign(document("SZ-2", false)),
            LookupOutcome::CredentialsRejected(CREDENTIALS.answer()),
            LookupOutcome::Api(API.answer()),
        ], &variants!(LookupOutcome { Absent, Live(_), Reversed { .. }, Collision(_), Foreign(_), CredentialsRejected(_), Api(_) })));
    all.extend(entries_of(vec![
            CreateOutcome::Issued(issued_document()),
            CreateOutcome::TargetChanged,
            CreateOutcome::Found(document("SZ-1", false)),
            CreateOutcome::Reversed(document("SZ-1", true)),
            CreateOutcome::LiveAgain(document("SZ-1", false)),
            CreateOutcome::Reconciled(document("SZ-1", false)),
            CreateOutcome::Collision(document("SZ-1", false)),
            CreateOutcome::DuplicateOrderNumber {
                answer: SzamlazzAnswer::new("152", "A rendelésszám már szerepel egy számlán: SZ-2"),
                existing_number: Some("SZ-2".to_owned()),
            },
            CreateOutcome::Rejected(Rejection::from(REJECTED.answer())),
            CreateOutcome::CredentialsRejected(CREDENTIALS.answer()),
            CreateOutcome::Api(API.answer()),
            CreateOutcome::Unavailable {
                message: DOWN.to_owned(),
            },
        ], &variants!(CreateOutcome { TargetChanged, Issued(_), Found(_), Reversed(_), LiveAgain(_), Reconciled(_), Collision(_), DuplicateOrderNumber { .. }, Rejected(_), CredentialsRejected(_), Api(_), Unavailable { .. } })));
    all.extend(entries_of(vec![
            StornoLookupOutcome::Absent,
            StornoLookupOutcome::AlreadyReversed {
                storno_number: "SS-1".to_owned(),
            },
            StornoLookupOutcome::CredentialsRejected(CREDENTIALS.answer()),
            StornoLookupOutcome::Api(API.answer()),
        ], &variants!(StornoLookupOutcome { Absent, AlreadyReversed { .. }, CredentialsRejected(_), Api(_) })));
    all.extend(entries_of(vec![
            StornoOutcome::Reversed(storno_document()),
            StornoOutcome::AlreadyReversed {
                storno_number: "SS-1".to_owned(),
            },
            StornoOutcome::NotStornoable,
            StornoOutcome::Rejected(Rejection::from(SzamlazzAnswer::new(
                "221",
                "A számlához helyesbítő számla tartozik.",
            ))),
            StornoOutcome::CredentialsRejected(CREDENTIALS.answer()),
            StornoOutcome::Api(API.answer()),
            StornoOutcome::Unavailable {
                message: DOWN.to_owned(),
            },
        ], &variants!(StornoOutcome { Reversed(_), AlreadyReversed { .. }, NotStornoable, Rejected(_), CredentialsRejected(_), Api(_), Unavailable { .. } })));
    all.extend(entries_of(vec![
            DeleteOutcome::Deleted,
            DeleteOutcome::AlreadyGone,
            DeleteOutcome::TargetChanged,
            DeleteOutcome::Paid,
            DeleteOutcome::Inconclusive(API.answer()),
            DeleteOutcome::Api(API.answer()),
            DeleteOutcome::GuardFailed(Unanswered::Transport(TRANSPORT.to_owned())),
            DeleteOutcome::GuardFailed(Unanswered::Unavailable(DOWN.to_owned())),
            DeleteOutcome::Rejected(Rejection::from(REJECTED.answer())),
            DeleteOutcome::CredentialsRejected(CREDENTIALS.answer()),
            DeleteOutcome::Lost(Unanswered::Transport(TRANSPORT.to_owned())),
            DeleteOutcome::Lost(Unanswered::Unavailable(DOWN.to_owned())),
        ], &variants!(DeleteOutcome { Deleted, AlreadyGone, TargetChanged, Paid, Api(_), GuardFailed(_), Inconclusive(_), Rejected(_), CredentialsRejected(_), Lost(_) })));
    all.extend(entries_of(vec![
            SetCreditEntriesOutcome::Done {
                outstanding: Some(dec!(0)),
                gross: Some(dec!(12700)),
            },
            SetCreditEntriesOutcome::Rejected(Rejection::from(SzamlazzAnswer::new(
                "463",
                "Sztornózott számlára nem rögzíthető kifizetés.",
            ))),
            SetCreditEntriesOutcome::CredentialsRejected(CREDENTIALS.answer()),
            SetCreditEntriesOutcome::Inconclusive(API.answer()),
            SetCreditEntriesOutcome::Lost(Unanswered::Transport(TRANSPORT.to_owned())),
            SetCreditEntriesOutcome::Lost(Unanswered::Unavailable(DOWN.to_owned())),
        ], &variants!(SetCreditEntriesOutcome { Done { .. }, Rejected(_), CredentialsRejected(_), Inconclusive(_), Lost(_) })));
    all.extend(entries_of(
        vec![
            ProbeOutcome::Accepted,
            ProbeOutcome::CredentialsRejected(CREDENTIALS.answer()),
        ],
        &variants!(ProbeOutcome { Accepted, CredentialsRejected(_) }),
    ));
    all.extend(entries_of(
        vec![
            TaxpayerOutcome::Found(taxpayer()),
            TaxpayerOutcome::CredentialsRejected(CREDENTIALS.answer()),
            TaxpayerOutcome::Api(NAV.answer()),
        ],
        &variants!(TaxpayerOutcome { Found(_), CredentialsRejected(_), Api(_) }),
    ));
    all
}

/// A szamlazz.hu code and message, as the answer-carrying variants carry
/// them.
struct Code {
    code: &'static str,
    message: &'static str,
}

impl Code {
    fn answer(&self) -> SzamlazzAnswer {
        SzamlazzAnswer::new(self.code, self.message)
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

/// The NAV code the taxpayer lookup's `Api` sample carries: `funcCode ERROR`
/// relayed by szamlazz.hu.
const NAV: Code = Code {
    code: "OPERATION_FAILED",
    message: "Az adatszolgáltatás jelenleg nem elérhető.",
};

/// The failure text of the `Lost(Transport)` samples of the two one-shot
/// write steps.
const TRANSPORT: &str = "error sending request for url (https://www.szamlazz.hu/szamla/)";

/// The `szlahu_down` header value of the `Unavailable` samples: szamlazz.hu's
/// maintenance notice.
const DOWN: &str = "Karbantartás miatt a szolgáltatás átmenetileg nem elérhető.";

/// The reply of a create, with every field the `xmlszamlavalasz` body and
/// the `szlahu_id` header can carry, parsed the way the gateway parses it
/// and projected the way the create step projects it.
fn issued_document() -> IssuedDocument {
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
        vec![
            LineItem::try_calculated(
                "Eladó izé",
                dec!(1),
                "db",
                dec!(10000),
                VatRate::percent(27),
                szamlazz_agent::Rounding::minor_unit(&Currency::HUF),
            )
            .expect("fits"),
        ],
    );
    create
        .parse(&reply("SZ-1", "10000", "12700", "12700"))
        .expect("xmlszamlavalasz parses")
        .into_issued()
        .expect("a numbered reply")
        .into()
}

/// The reply of a storno of `SZ-1`: the storno invoice `SS-1` with negative
/// totals, parsed and projected the way the storno step does.
fn storno_document() -> IssuedDocument {
    StornoInvoice::new("SZ-1")
        .parse(&reply("SS-1", "-10000", "-12700", "0"))
        .expect("xmlszamlavalasz parses")
        .into()
}

/// NAV's record of a valid taxpayer with every field the `xmltaxpayer`
/// response can carry, projected onto the crate-owned response the way the
/// gateway projects it.
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
/// carry, parsed through the agent crate: a test-account e-invoice of
/// `ORD-1` referencing `SZ-0` and the proforma `D-1`, with two credit entries, a
/// seller block, a buyer block, line items, ledger blocks, labels and a PDF.
fn wire_document(number: &str, reversed: bool) -> InvoiceDocument {
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
            <iktatoszam>IKT-2026-1</iktatoszam><tipus>SZ</tipus><eszamla>1</eszamla><hivszamlaszam>SZ-0</hivszamlaszam>
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
    QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(number)))
        .parse(&RawResponse::new::<&str, &str>([], xml.into_bytes()))
        .expect("szamla XML parses")
}

/// [`wire_document`] projected onto [`FoundDocument`] the way the gateway
/// projects a query answer.
fn document(number: &str, reversed: bool) -> Box<FoundDocument> {
    Box::new(FoundDocument::from(wire_document(number, reversed)))
}

/// The namespace the prologue journals (`namespace` step).
fn namespace() -> Namespace {
    "acct".parse().expect("namespace")
}

/// The account the `account` step journals, with every optional field set.
/// Never the agent key: the type cannot carry it.
fn account() -> Account {
    let mut account = Account::new("acme", "acme-credentials");
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

/// Every [`Journaled`] type (the `journaled!` list in `support`, the only way
/// a type becomes a `ctx.run` result) has samples in [`entries`], and nothing
/// else does; and every sample round-trips ([`entries_of`] asserts it per
/// sample). With the per-enum [`variants!`] check inside `entries_of`, this
/// is what makes "every entry is scanned" a mechanism and not a convention.
#[test]
fn every_journaled_type_is_sampled() {
    let entries = entries();
    let sampled: BTreeSet<&str> = entries.iter().map(|entry| entry.type_name).collect();
    let journaled: BTreeSet<&str> = journaled_types().into_iter().collect();
    let unsampled: Vec<&&str> = journaled.difference(&sampled).collect();
    assert!(
        unsampled.is_empty(),
        "journaled types with no samples in `entries()`: {unsampled:?}"
    );
    let stray: Vec<&&str> = sampled.difference(&journaled).collect();
    assert!(
        stray.is_empty(),
        "sampled types that are not journaled: {stray:?}"
    );
}

/// No journal entry carries the agent key. The key never leaves the
/// credential store as a journalable value (`Credentials` has no serde
/// implementation; the `account` tests' compile-time guard), so the one path
/// with key-adjacent input is a transport failure's text: `DeleteOutcome` and
/// `SetCreditEntriesOutcome` journal one, produced here by a gateway holding a
/// sentinel key against an endpoint that refuses connections. Every other
/// sample is scanned too, so the claim stays "every entry" when a variant
/// gains an input. The e2e scan of every journal byte, which needs a server,
/// is the end-to-end form.
#[tokio::test]
async fn no_journal_entry_carries_the_agent_key() {
    const SENTINEL: &str = "journal-sentinel-agent-key-9c4e1a";
    let contains_sentinel = |json: &str| json.contains(SENTINEL);
    assert!(
        contains_sentinel(&format!("{{\"k\":\"{SENTINEL}\"}}")),
        "the scan reads"
    );

    let config: StaticConfig = serde_json::from_value(serde_json::json!({
        "account": {
            "id": "acme",
            "agent_key": SENTINEL,
            "endpoint": "http://127.0.0.1:1/",
            "defaults": { "currency": "EUR", "language": "en", "number_prefix": "ACME", "aggregator": "aggregator" },
            "seller": { "bank": "Test Bank", "bank_account": "11111111-22222222-33333333", "email": { "reply_to": "billing@acme.test" } },
        },
    }))
    .expect("config");
    let resolver = StaticResolver::try_from(config).expect("resolver");
    let account = resolver.resolve(None).await.expect("account");
    let credentials = resolver
        .fetch(&account.credential_ref)
        .await
        .expect("credentials");
    assert!(
        matches!(&credentials, Credentials::AgentKey(key) if key.expose() == SENTINEL),
        "the key is really in play"
    );
    let resolution = serde_json::to_string(&Resolution::Account(Box::new(account.clone())))
        .expect("resolution serialises");
    assert!(
        resolution.contains("\"id\":\"acme\""),
        "the entry is the account's: {resolution}"
    );
    assert!(
        !contains_sentinel(&resolution),
        "the account step's entry: {resolution}"
    );

    let gateway = open_gateway(account, credentials);
    let delete = gateway
        .delete_proforma(
            &Doc::new("D-1", "D").parse(),
            &OrderKey::parse("ORD-1").expect("order"),
            false,
        )
        .await;
    assert!(
        matches!(delete, DeleteOutcome::GuardFailed(_)),
        "{delete:?}"
    );
    let credit_entries = [CreditEntryInput::new(
        jiff::civil::date(2026, 7, 4),
        ContractPaymentMethod::Transfer,
        dec!(12700),
    )];
    let set_credit_entries = gateway
        .set_credit_entries("SZ-1", &credit_entries, false)
        .await;
    assert!(
        matches!(set_credit_entries, SetCreditEntriesOutcome::Lost(_)),
        "{set_credit_entries:?}"
    );
    for (label, json) in [
        (
            "DeleteOutcome::GuardFailed",
            serde_json::to_string(&delete).expect("serialises"),
        ),
        (
            "SetCreditEntriesOutcome::Lost",
            serde_json::to_string(&set_credit_entries).expect("serialises"),
        ),
    ] {
        assert!(!contains_sentinel(&json), "{label}: {json}");
    }

    for entry in entries() {
        assert!(
            !contains_sentinel(&entry.json),
            "{}: {}",
            entry.label,
            entry.json
        );
    }
}

/// The keys a `szamlazz_agent` response type would bring into a journal entry
/// and the worker's projections leave out: the seller block, the buyer block
/// (the buyer's name, addresses, email and tax numbers under it), the line
/// items, the financial items, the labels and the PDF of a queried document,
/// and the PDF of a create reply. (The `Account`'s seller block carries an
/// `email` block of its own: the operator's configuration, not a document's.)
const NEVER_JOURNALED: &[&str] = &[
    "supplier",
    "buyer",
    "items",
    "financial_items",
    "labels",
    "pdf",
];

/// Every object key in `value`, at any depth.
fn keys_of(value: &Value, into: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                into.insert(key.clone());
                keys_of(value, into);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys_of(item, into)),
        _ => {}
    }
}

/// No journal entry carries the document body: every sample serialises
/// without any [`NEVER_JOURNALED`] key at any depth. The agent crate's own
/// `InvoiceDocument`, serialised from the same wire XML, is the positive
/// control: it carries every one of the keys, so the scan reads.
#[test]
fn no_journal_entry_carries_the_buyer_the_seller_the_items_or_the_pdf() {
    let scan = |json: &str| -> Vec<&'static str> {
        let value: Value = serde_json::from_str(json).expect("json");
        let mut keys = BTreeSet::new();
        keys_of(&value, &mut keys);
        NEVER_JOURNALED
            .iter()
            .copied()
            .filter(|never| keys.contains(*never))
            .collect()
    };

    let unprojected =
        serde_json::to_string(&wire_document("SZ-1", false)).expect("the agent type serialises");
    assert_eq!(
        scan(&unprojected),
        NEVER_JOURNALED,
        "the agent's document carries every key the projection drops: the scan reads"
    );

    for entry in entries() {
        let carried = scan(&entry.json);
        assert!(
            carried.is_empty(),
            "{}: journals {carried:?}: {}",
            entry.label,
            entry.json
        );
    }
}
