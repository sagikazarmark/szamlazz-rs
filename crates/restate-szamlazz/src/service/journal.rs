//! Journal-compatibility fixtures: one pinned JSON document per variant of
//! every type the services journal as a `ctx.run` result, under
//! `tests/journal/<type>/`.
//!
//! An in-flight invocation replays the entries the *previous* deployment
//! wrote; an entry the new code cannot decode is a retryable SDK error that
//! kills the invocation once its attempts are spent, holding the order key
//! for the duration. So every journaled type is **additive-only** (a new field
//! defaults, a new variant may be added, nothing is renamed, removed or
//! retyped, save the one widening `T` → `Option<T>` whose old values all
//! decode to `Some` and re-encode unchanged; the compatibility test below is
//! its proof), and this module makes a violation fail CI instead of a deploy:
//!
//! - the **generator** test pins every variant of every journaled type: the
//!   JSON the current code writes must equal `tests/journal/<type>/<variant>.json`
//!   byte for byte. A missing or differing fixture fails it with the
//!   instructions below; it never writes on its own.
//! - the **compatibility** test replays every fixture in every type's
//!   directory (the current ones and every shape archived before them)
//!   through the current type: each must decode, and re-encode to a superset
//!   of itself (so a renamed `Option` field that silently decodes to `None`
//!   is caught, not only a missing required one).
//! - the **leak guard** serialises every variant of every journaled type
//!   built around an account whose agent key is a sentinel and asserts the
//!   sentinel is in none of them: the cheap, server-less complement to the
//!   e2e scan of every journal byte.
//! - the **registry** test holds the list of pinned types to the trait's
//!   implementors: the `journaled!` list beside `Journaled` (the one place
//!   the trait is implemented) is exactly what [`registry`] pins, so a type
//!   journaled without pins fails CI by name, and [`pins`] refuses a variant
//!   named in its `variants!` list without a sample, so a variant that
//!   compiles (the exhaustive match) but has no fixture fails the same way.
//!   Completeness is a mechanism here, not a discipline.
//!
//! The sequence of run *names* a handler journals is pinned separately, by
//! the e2e suite's run-name pin (`tests/e2e/harness/run_names.rs`, `RUN_NAMES`).
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
//! stays in the compatibility test for as long as the type is journaled, so an
//! additive change (a defaulted field) regenerates cleanly while a rename or
//! removal keeps failing on the archived file. That failure is the answer,
//! not an obstacle: keep the old name (add the new field with a default, add
//! the new variant), or, for a shape that must change beyond that, retire the
//! type (the archive rule below); before the first production deployment a
//! break may instead be listed in [`DELIBERATE_BREAKS`], with its archives
//! kept. Deleting the archived file is never the way: it would make the test
//! pass while every in-flight invocation of the previous deployment is killed
//! on upgrade. Review every regenerated diff as a contract change. A generator
//! failure while the compatibility test passes is a formatting change and not
//! a journal break (a dependency upgrade that prints a number or a date
//! differently), and regenerates the same way.
//!
//! The one exception on disk is the pre-go-live break of #127, when the
//! document outcomes went from the agent crate's types to the worker's
//! projections: its archives are kept as the record of the shape that was
//! replaced and listed in [`DELIBERATE_BREAKS`], which the compatibility test
//! skips and asserts still fail to replay (and one of which the data guard
//! reads as its positive control). Nothing was in flight to be killed. The
//! list is not a way to break the journal again: after go-live a shape that
//! must break is a retired type, never a deleted archive (the archive rule).
//!
//! # The archive rule
//!
//! **Once the first production deployment exists, an archived shape of a type
//! the code still journals is never deleted**, and a fixture is never
//! regenerated without its archive: an archive under
//! `tests/journal/<type>/<variant>.<n>.json` is the only record of a shape a
//! running deployment may have journaled, and the compatibility test can hold
//! the current code to it only while the file is there. The mechanism cannot
//! tell a legitimate deletion from an illegitimate one: `5ea51f9`
//! (2026-09-07) regenerated `resolution/account.json` without `mode` and
//! `supplier_id` and committed no `account.1.json`, a removal the rule
//! forbids, admitted because nothing had been deployed to replay the old
//! shape; the same commit after go-live would have been a killed invocation
//! for every order in flight across the upgrade. So the rule is the
//! reviewer's, stated here where the generator's instructions are: before
//! go-live, a regeneration without an archive, or a break listed in
//! [`DELIBERATE_BREAKS`], is a judgement call recorded in the commit message;
//! after it, the archive is committed with the new shape and stays for as
//! long as the type is journaled.
//!
//! The one way a directory goes is **retirement**: a shape that must change
//! beyond what additive allows is a *new* journaled type under a new
//! directory (and a new run-name row), and the old type is dropped from the
//! `journaled!` list, its directory, archives included, removed in the same
//! commit, which the unclaimed-directory check below demands. Safe because
//! that deploy drains first (the flag-day script): once nothing of the
//! previous deployment is in flight, no invocation can replay the retired
//! type, and a completed invocation's journal is read by the UI, never
//! replayed. Deleting an archive of a type still journaled, to make the
//! compatibility test pass, is never the way (ADR 0005, the #125 amendment).

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rust_decimal::dec;
use serde_json::Value;
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::ops::query_pdf::InvoiceSelector;
use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::ops::taxpayer::{QueryTaxpayer, TaxpayerPrefix};
use szamlazz_agent::wire::{AgentRequest as _, RawResponse};
use szamlazz_agent::{Currency, InvoiceNumber, Language, LineItem, PaymentMethod, VatRate};

use szamlazz_agent::Credentials;

use crate::account::{
    Account, AccountResolver as _, CredentialStore as _, Endpoint, StaticConfig, StaticResolver,
};
use crate::config::{Defaults, Namespace, SellerConfig, SellerEmailConfig};
use crate::contract::{
    PaymentEntry, PaymentMethod as ContractPaymentMethod, QueryTaxpayerResponse,
};
use crate::gateway::{
    CreateOutcome, DeleteOutcome, FoundDocument, IssuedDocument, LookupOutcome, ProbeOutcome,
    QueryOutcome, SetPaymentsOutcome, StornoLookupOutcome, StornoOutcome, TaxpayerOutcome,
};
use crate::test_support::open_gateway;

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
    /// The type, as [`std::any::type_name`] writes it: what the registry
    /// test matches against the `Journaled` implementors.
    type_name: &'static str,
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

/// The variants of a journaled type: how to file a sample (its variant's
/// stem) and every variant the type has, by name and stem, so [`pins`] can
/// name a variant that has no sample.
struct Variants<T> {
    /// The stem a sample is filed under.
    stem_of: fn(&T) -> &'static str,
    /// Every variant, `(name, stem)`, in the list's order; the stems are
    /// distinct, so a stem names its variant.
    all: &'static [(&'static str, &'static str)],
}

/// The [`Variants`] of an enum: one arm per variant, `Pattern => "stem"`.
/// `stem_of` is one exhaustive `match` over the arms, so a variant added to
/// the enum fails to compile until it is listed here, and `all` is what
/// [`pins`] checks the samples against, so a variant listed without a sample
/// fails the generator by name instead of going unpinned.
macro_rules! variants {
    ($ty:ident { $($variant:ident $( ( $($tuple:tt)* ) )? $( { $($fields:tt)* } )? => $stem:literal),+ $(,)? }) => {
        Variants::<$ty> {
            stem_of: |value| match value {
                $($ty::$variant $( ( $($tuple)* ) )? $( { $($fields)* } )? => $stem,)+
            },
            all: &[$((stringify!($variant), $stem)),+],
        }
    };
}

/// The [`Variants`] of a type with one shape (a newtype, a struct), filed as
/// `value`.
fn single<T>() -> Variants<T> {
    Variants {
        stem_of: |_| "value",
        all: &[("value", "value")],
    }
}

/// Pins `samples` as the fixtures of `T` under `tests/journal/<dir>/`, one
/// per variant, each filed under the stem `variants` gives it. Every variant
/// `variants` lists must have exactly one sample: a variant without one is
/// refused here by name, a second sample of one stem too. Only a
/// [`Journaled`] type can be pinned, and only a `Journaled` type can be the
/// result of a run: the trait is the link from the `ctx.run` sites to this
/// directory.
fn pins<T: Journaled>(dir: &'static str, samples: &[T], variants: &Variants<T>) -> Pins {
    for (index, (name, stem)) in variants.all.iter().enumerate() {
        assert!(
            !variants.all[..index].iter().any(|(_, seen)| seen == stem),
            "{dir}: variant {name} shares its stem {stem} with another variant"
        );
    }
    let mut pinned: Vec<Variant> = Vec::with_capacity(samples.len());
    for sample in samples {
        let stem = (variants.stem_of)(sample);
        assert!(
            !pinned.iter().any(|seen| seen.stem == stem),
            "{dir}: two samples of variant {stem}"
        );
        let mut json = serde_json::to_string_pretty(sample).expect("journaled types serialize");
        json.push('\n');
        pinned.push(Variant { stem, json });
    }
    let unsampled: Vec<&str> = variants
        .all
        .iter()
        .filter(|(_, stem)| !pinned.iter().any(|seen| seen.stem == *stem))
        .map(|(name, _)| *name)
        .collect();
    assert!(
        unsampled.is_empty(),
        "{dir}: no sample for variant(s) {unsampled:?}; every variant of a journaled type has a fixture"
    );
    Pins {
        type_name: std::any::type_name::<T>(),
        dir,
        variants: pinned,
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
    pins("namespace", &[namespace()], &single())
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
        &variants!(Resolution {
            Account(_) => "account",
            Unscoped => "unscoped",
            Unknown { .. } => "unknown",
        }),
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
        &variants!(QueryOutcome {
            Found(_) => "found",
            NotFound => "not-found",
            CredentialsRejected { .. } => "credentials-rejected",
            Api { .. } => "api",
        }),
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
        &variants!(LookupOutcome {
            Absent => "absent",
            Live(_) => "live",
            Reversed { .. } => "reversed",
            Collision(_) => "collision",
            Foreign(_) => "foreign",
            CredentialsRejected { .. } => "credentials-rejected",
            Api { .. } => "api",
        }),
    )
}

/// The create step of issuing (`create-{kind}`).
fn create_outcome_pins() -> Pins {
    pins(
        "create-outcome",
        &[
            CreateOutcome::Issued(issued_document()),
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
            CreateOutcome::Api {
                code: API.code(),
                message: API.message(),
            },
            CreateOutcome::Unavailable {
                message: DOWN.to_owned(),
            },
        ],
        &variants!(CreateOutcome {
            Issued(_) => "issued",
            Found(_) => "found",
            Reversed(_) => "reversed",
            LiveAgain(_) => "live-again",
            Reconciled(_) => "reconciled",
            Collision(_) => "collision",
            DuplicateOrderNumber { .. } => "duplicate-order-number",
            Rejected { .. } => "rejected",
            CredentialsRejected { .. } => "credentials-rejected",
            Api { .. } => "api",
            Unavailable { .. } => "unavailable",
        }),
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
        &variants!(StornoLookupOutcome {
            Absent => "absent",
            AlreadyReversed { .. } => "already-reversed",
            CredentialsRejected { .. } => "credentials-rejected",
            Api { .. } => "api",
        }),
    )
}

/// The storno step (`storno-{number}`).
fn storno_outcome_pins() -> Pins {
    pins(
        "storno-outcome",
        &[
            StornoOutcome::Reversed(storno_document()),
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
            StornoOutcome::Api {
                code: API.code(),
                message: API.message(),
            },
            StornoOutcome::Unavailable {
                message: DOWN.to_owned(),
            },
        ],
        &variants!(StornoOutcome {
            Reversed(_) => "reversed",
            AlreadyReversed { .. } => "already-reversed",
            NotStornoable => "not-stornoable",
            Rejected { .. } => "rejected",
            CredentialsRejected { .. } => "credentials-rejected",
            Api { .. } => "api",
            Unavailable { .. } => "unavailable",
        }),
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
        &variants!(DeleteOutcome {
            Deleted => "deleted",
            AlreadyGone => "already-gone",
            Rejected { .. } => "rejected",
            CredentialsRejected { .. } => "credentials-rejected",
            Transport(_) => "transport",
        }),
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
        &variants!(SetPaymentsOutcome {
            Done { .. } => "done",
            Rejected { .. } => "rejected",
            CredentialsRejected { .. } => "credentials-rejected",
            Transport(_) => "transport",
        }),
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
        &variants!(ProbeOutcome {
            Accepted => "accepted",
            CredentialsRejected { .. } => "credentials-rejected",
        }),
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
        &variants!(TaxpayerOutcome {
            Found(_) => "found",
            CredentialsRejected { .. } => "credentials-rejected",
            Api { .. } => "api",
        }),
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

/// The NAV code the taxpayer lookup's `Api` sample carries: `funcCode ERROR`
/// relayed by szamlazz.hu.
const NAV: Code = Code {
    code: "OPERATION_FAILED",
    message: "Az adatszolgáltatás jelenleg nem elérhető.",
};

/// The failure text of the `Transport` samples of the two write steps that
/// keep one.
const TRANSPORT: &str = "error sending request for url (https://www.szamlazz.hu/szamla/)";

/// The `szlahu_down` header value of the `Unavailable` samples: szamlazz.hu's
/// maintenance notice.
const DOWN: &str = "Karbantartás miatt a szolgáltatás átmenetileg nem elérhető.";

/// The reply of a create, with every field the `xmlszamlavalasz` body and
/// the `szlahu_id` header can carry: `SZ-1`, its totals, the buyer's account
/// URL and a PDF (which the projection drops). Parsed the way the gateway
/// parses it and projected the way the create step projects it, since the
/// agent's types are `#[non_exhaustive]`.
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
        .try_into()
        .expect("a numbered reply")
}

/// The reply of a storno of `SZ-1`: the storno invoice `SS-1` with negative
/// totals, parsed the way the gateway parses it and projected the way the
/// storno step projects it.
fn storno_document() -> IssuedDocument {
    StornoInvoice::new("SZ-1")
        .parse(&reply("SS-1", "-10000", "-12700", "0"))
        .expect("xmlszamlavalasz parses")
        .into()
}

/// NAV's record of a valid taxpayer with every field the `xmltaxpayer`
/// response can carry (the registered name, the tax number detail and one
/// detailed and one simple address), projected onto the crate-owned response
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
/// carry, projected onto [`FoundDocument`] the way the gateway projects a
/// query answer, so that the fixture pins every field the projection reads
/// with a value (a test-account e-invoice of `ORD-1` referencing `SZ-0` and
/// the proforma `D-1`, with two payments) and shows what it drops (the
/// seller block with `szallito/id` 972720, the buyer block, the line items,
/// the ledger blocks, the labels, the PDF). Parsed through the agent crate,
/// since its types are `#[non_exhaustive]`. The kind does not change the
/// shape, so one kind is enough.
fn document(number: &str, reversed: bool) -> Box<FoundDocument> {
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
    Box::new(FoundDocument::from(
        QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(InvoiceNumber::new(number)))
            .parse(&RawResponse::new::<&str, &str>([], xml.into_bytes()))
            .expect("szamla XML parses"),
    ))
}

/// The namespace the prologue pins (`namespace` step).
fn namespace() -> Namespace {
    "acct".parse().expect("namespace")
}

/// The account the `account` step journals, with every optional field set so
/// that a rename anywhere in [`Account`], [`Defaults`], [`SellerConfig`] or
/// [`SellerEmailConfig`] is caught. Never the agent key: the type cannot
/// carry it.
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

/// Whether `fixture` is covered by `current`: every object key of the fixture
/// is present in `current` with a covered value, arrays match element for
/// element, scalars are equal. Keys only `current` has (fields added since
/// the fixture was written) are allowed; that is what "additive" means.
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
A journaled shape changed. If the change is additive (a new variant, or a new field with a serde default), \
regenerate with

    UPDATE_JOURNAL_FIXTURES=1 cargo test -p restate-szamlazz journal

which writes the missing fixtures and keeps a differing one beside the new shape as <variant>.<n>.json, \
then run the tests again and review the diff as a contract change. If a field or variant was renamed, \
removed or retyped, every in-flight invocation of the previous deployment will be killed on upgrade: \
do not regenerate; keep the old name (see the gateway module docs).";

/// The archived shapes the current types deliberately do **not** replay, as
/// `(directory, file)` under `tests/journal/`: the record of the one break
/// the pre-go-live window allowed (#127; ADR 0005, the crate-owned
/// projection amendment). Before it, the document outcomes journaled the
/// Számla Agent crate's `InvoiceDocument`, `InvoiceCreationResult` and
/// `CreatedInvoice` as they were; since it they journal the worker's own
/// [`FoundDocument`] and [`IssuedDocument`], a flat shape the nested one
/// does not decode into. Nothing was in flight to be killed: there was no
/// production deployment before the change. The compatibility test skips
/// these and asserts each still fails to replay, so an entry cannot outlive
/// its reason; the archives stay committed as the shape that was replaced
/// (and `lookup-outcome/live.1.json` is the data guard's positive control).
///
/// A break after go-live is not listed here: it is a new journaled type and a
/// drained deploy that retires the old one with its directory, never a
/// deleted archive (the archive rule in the module docs).
const DELIBERATE_BREAKS: &[(&str, &str)] = &[
    ("lookup-outcome", "live.1.json"),
    ("lookup-outcome", "reversed.1.json"),
    ("lookup-outcome", "collision.1.json"),
    ("lookup-outcome", "foreign.1.json"),
    ("create-outcome", "issued.1.json"),
    ("create-outcome", "found.1.json"),
    ("create-outcome", "reversed.1.json"),
    ("create-outcome", "live-again.1.json"),
    ("create-outcome", "reconciled.1.json"),
    ("create-outcome", "collision.1.json"),
    ("query-outcome", "found.1.json"),
    ("storno-outcome", "reversed.1.json"),
];

/// Whether `path` (a fixture under `tests/journal/<dir>/`) is one of the
/// [`DELIBERATE_BREAKS`].
fn is_deliberate_break(dir: &str, path: &Path) -> bool {
    let file = path.file_name().and_then(|name| name.to_str());
    DELIBERATE_BREAKS
        .iter()
        .any(|(broken_dir, broken_file)| *broken_dir == dir && Some(*broken_file) == file)
}

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

/// The registry is complete by mechanism, not discipline: the types it pins
/// are exactly the `Journaled` implementors (the `journaled!` list beside the
/// trait, which is the one place the trait is implemented), so a type
/// journaled without pins fails here, before the generator can ask for a
/// fixture it does not know about. The other half, that every variant of a
/// pinned enum has a sample, is [`pins`]' own check, which every registry
/// entry runs through on construction.
#[test]
fn the_registry_pins_every_journaled_type_and_nothing_else() {
    let registry = registry();
    let pinned: BTreeSet<&str> = registry.iter().map(|pins| pins.type_name).collect();
    let journaled: BTreeSet<&str> = super::support::journaled_types().into_iter().collect();
    let unpinned: Vec<&&str> = journaled.difference(&pinned).collect();
    let not_journaled: Vec<&&str> = pinned.difference(&journaled).collect();
    assert!(
        unpinned.is_empty(),
        "journaled types without pins in `registry()`: {unpinned:?}; a `Journaled` type is pinned \
         under tests/journal/<type>/ before it is journaled"
    );
    assert!(
        not_journaled.is_empty(),
        "`registry()` pins types that are not journaled: {not_journaled:?}"
    );
    assert_eq!(
        registry.len(),
        journaled.len(),
        "one pins entry per journaled type, no type pinned twice"
    );
}

/// The compatibility test: every fixture under every journaled type's
/// directory (the current shape and every shape archived before it)
/// decodes through the current type and re-encodes to a superset of itself.
/// What a replay of an in-flight invocation needs from the new code. The
/// verdict on an archived shape is explicit either way: it replays, or it is
/// one of the [`DELIBERATE_BREAKS`], which must still fail to replay and is
/// otherwise skipped.
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
        "tests/journal/ holds fixtures of no journaled type: {unclaimed:?}; a type that is no longer \
         journaled is removed knowingly, together with its fixtures"
    );

    let mut failures = Vec::new();
    let mut replayed = 0;
    let mut broken = 0;
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
            let replays = match (pins.replay)(&text) {
                Err(error) => Err(format!("does not decode: {error}")),
                Ok(current) if !is_covered_by(&fixture, &current) => Err(
                    "decodes, but re-encodes without part of the fixture; a field was renamed or \
                     retyped and decoded to its default"
                        .to_owned(),
                ),
                Ok(_) => Ok(()),
            };
            match (is_deliberate_break(pins.dir, &path), replays) {
                (false, Ok(())) => replayed += 1,
                (false, Err(why)) => failures.push(format!("{}: {why}", rel(&path))),
                (true, Err(_)) => broken += 1,
                (true, Ok(())) => failures.push(format!(
                    "{}: is listed in DELIBERATE_BREAKS but replays through the current types; the \
                     entry has outlived its reason: remove it",
                    rel(&path)
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "journal entries of the previous deployment would not replay:\n  {}\n\n\
         A renamed, removed or retyped field or variant kills every in-flight invocation on upgrade. \
         Keep the old name (a new field defaults, a new variant is added; nothing is renamed or removed); \
         see the gateway module docs.",
        failures.join("\n  ")
    );
    assert!(replayed > 0, "no fixture was replayed");
    assert_eq!(
        broken,
        DELIBERATE_BREAKS.len(),
        "every listed break is an archived fixture on disk"
    );
}

/// The leak guard on the journaled types, without a server: every variant of
/// every journaled type, built around an account whose agent key is a
/// sentinel, serialises without the sentinel. The key has two ways in. The
/// `account` step's entry is built the way the static resolver builds it from
/// configuration carrying the key: the one path a key travels next to an
/// `Account`. The two write outcomes that keep a transport failure's text
/// (`DeleteOutcome::Transport`, `SetPaymentsOutcome::Transport`) are produced
/// by a gateway opened with the sentinel credentials against an endpoint that
/// refuses connections, so the text is a real client error's. Every other
/// variant is built from a szamlazz.hu answer or a fixed value and has no
/// key-adjacent input; the registry's samples are scanned so that the claim
/// stays "every variant" when one gains an input. Complements the compile-time
/// `assert_not_impl_any!` guard on `AgentKey` / `Credentials` (`account`
/// tests) and the e2e scan of every journal byte, which needs a server.
#[tokio::test]
async fn no_journaled_type_serialises_the_agent_key() {
    const SENTINEL: &str = "journal-sentinel-agent-key-9c4e1a";
    let contains_sentinel = |json: &str| json.contains(SENTINEL);
    assert!(
        contains_sentinel(&format!("{{\"k\":\"{SENTINEL}\"}}")),
        "the scan reads"
    );

    // The account, resolved from configuration that carries the key, with
    // every optional field set; the endpoint refuses connections.
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

    // The two outcomes that journal a transport failure's text, from a
    // gateway holding the sentinel credentials.
    let gateway = open_gateway(account, credentials);
    let delete = gateway.delete_proforma("D-1").await;
    assert!(matches!(delete, DeleteOutcome::Transport(_)), "{delete:?}");
    let entries = [PaymentEntry::new(
        jiff::civil::date(2026, 7, 4),
        ContractPaymentMethod::Transfer,
        dec!(12700),
    )];
    let set_payments = gateway.set_payments("SZ-1", &entries, false).await;
    assert!(
        matches!(set_payments, SetPaymentsOutcome::Transport(_)),
        "{set_payments:?}"
    );
    for (label, json) in [
        (
            "delete-outcome/transport",
            serde_json::to_string(&delete).expect("serialises"),
        ),
        (
            "set-payments-outcome/transport",
            serde_json::to_string(&set_payments).expect("serialises"),
        ),
    ] {
        assert!(!contains_sentinel(&json), "{label}: {json}");
    }

    // Every variant of every journaled type the registry pins.
    let mut scanned = 0;
    for pins in registry() {
        for variant in &pins.variants {
            assert!(
                !contains_sentinel(&variant.json),
                "{}/{}: {}",
                pins.dir,
                variant.stem,
                variant.json
            );
            scanned += 1;
        }
    }
    assert!(scanned > 0, "no variant was scanned");
}

/// The keys a `szamlazz_agent` response type would bring into a journal entry
/// and the worker's projections leave out: the seller block, the buyer block
/// (the buyer's name, addresses, email and tax numbers under it), the line
/// items, the financial items, the labels and the PDF of a queried document,
/// and the PDF of a create reply. Personal data of the buyer and the largest
/// parts of a document, none of which any handler reads. (The `Account`'s
/// seller block carries an `email` block of its own: the operator's
/// configuration, not a document's.)
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

/// The guard behind "no `szamlazz_agent` response type is journaled": every
/// variant of every journaled type serialises without any of the
/// [`NEVER_JOURNALED`] keys, at any depth. What the projections exist for
/// (#127): a journal entry is visible in the Restate UI for the retention
/// period, and holds what the handlers read, not the document. The archived
/// pre-#127 shape of a found document is the positive control: it carries
/// every one of the keys, so the scan reads.
#[test]
fn no_journaled_type_carries_the_buyer_the_seller_the_items_or_the_pdf() {
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

    let archived = fs::read_to_string(fixtures().join("lookup-outcome/live.1.json"))
        .expect("the archived pre-#127 shape is committed");
    assert_eq!(
        scan(&archived),
        NEVER_JOURNALED,
        "the archived document carries every key the projection drops: the scan reads"
    );

    let mut scanned = 0;
    for pins in registry() {
        for variant in &pins.variants {
            let carried = scan(&variant.json);
            assert!(
                carried.is_empty(),
                "{}/{}: journals {carried:?}: {}",
                pins.dir,
                variant.stem,
                variant.json
            );
            scanned += 1;
        }
    }
    assert!(scanned > 0, "no variant was scanned");
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
/// writes for the variant, and (under [`Mode::Update`]) writes or archives.
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

    /// A variant named in a `variants!` list but given no sample is refused
    /// by `pins`, naming the variant: the check that closes the gap between
    /// "the new variant compiles" (the exhaustive match) and "the new variant
    /// has a fixture" (a sample the generator can write).
    #[test]
    #[should_panic(
        expected = "query-outcome: no sample for variant(s) [\"CredentialsRejected\", \"Api\"]"
    )]
    fn a_variant_without_a_sample_is_refused_by_name() {
        pins(
            "query-outcome",
            &[
                QueryOutcome::Found(document("SZ-1", false)),
                QueryOutcome::NotFound,
            ],
            &variants!(QueryOutcome {
                Found(_) => "found",
                NotFound => "not-found",
                CredentialsRejected { .. } => "credentials-rejected",
                Api { .. } => "api",
            }),
        );
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
