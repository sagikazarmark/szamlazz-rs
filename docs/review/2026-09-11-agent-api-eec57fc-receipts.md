# Számla Agent receipt API review — `eec57fc`

## Result and evidence boundary

**No confirmed defect in ordinary documented valid receipt requests or responses was found.** Create, storno, query and email send cover the current published fields. Genuine XSD sequences match the writers. There are **zero P0/P1/P2 implementation findings**. One confirmed **P3 malformed-artifact recovery improvement** is separated below from valid-response compatibility.

The important outstanding issues are contradictory first-party sources and missing execution evidence: effective downloadable schemas, receipt order-number settings, automatic MNB execution, call-ID scope/recovery, partial email inheritance, and actual reporting/delivery. A passing parser or request-schema test cannot settle these.

- Review date: **2026-09-11**. HEAD verified as **`eec57fcf3036d93cd68c9cfc017338cd3020e7dd`** at start and during final synthesis.
- Full `crates/szamlazz-agent/src/ops/receipt.rs:1–1479` inspected, including all public request/response types, serializers, parsers and embedded tests. Relevant shared `item.rs`, `types.rs`, `number.rs`, `xml.rs`, `wire.rs`, `client.rs`, `error.rs`, `recovery.md`, README and external tests were inspected.
- Primary documentation was fetched live, starting with all four requested categories and following their request, XML/XSD, response and settings/rules pages, including schema-location URLs, linked knowledge-base material and relevant Hungarian counterparts. Public PHP 2.12.4 source was downloaded and inspected in memory, **not executed**.
- **No test suite, probe, vendor operation, account inspection or credential access was performed.** The parent runs the suite. Reproductions below are source-derived recipes, not claims of a newly executed Rust test. No test results from earlier reviews are reused.
- The earlier `61c334f` receipt review was read only after the independent code/live-document comparison, as a crosscheck. Its conclusions were re-evaluated; its execution claims are not evidence of this HEAD passing.
- This review creates only this report, through `apply_patch`. Concurrent work appeared in the Restate crate during the review; the reviewed Agent source remained unmodified in `git status`.

In source citations below, `receipt.rs` means `crates/szamlazz-agent/src/ops/receipt.rs`; other source filenames mean `crates/szamlazz-agent/src/`. `tests/` means `crates/szamlazz-agent/tests/`. P3 means low-priority improvement, not a blocker for the documented ordinary paths.

## Confirmed finding

### R-H1 — P3: a corrupt nonblank PDF prevents access to otherwise readable receipt data

**Classification:** malformed-artifact recovery ergonomics, not demonstrated rejection of a valid vendor response. **Confidence:** high in code behavior; occurrence on an actual receipt exchange is unestablished.

- **Location:** `receipt.rs:690–701`, especially `Some(encoded) => Some(Pdf::from_base64(&encoded)?)` at **694**; `types.rs:104–116`; convenience-client propagation at `client.rs:403–405`.
- **Primary source:** [creation response][C-response]: “The response always contains the generated receipt data in XML (the `<nyugta>` block). The PDF (`<nyugtaPdf>`, base64-encoded) is only included … if `<pdfLetoltes>true</pdfLetoltes>` was set.” Its sample contains the literal **`<nyugtaPdf>...</nyugtaPdf>`**. [Storno response][S-response] similarly separates the SN data and PDF.
- **Code behavior:** receipt payload deserialization succeeds first, but invalid base64 propagates an error before `Receipt` is returned. The error contains no partially decoded receipt. Through `Client::send`, callers lose the typed number, call ID, order, type and other readable fields. A custom transport retaining `RawResponse` can still inspect its body; the convenience client does not return that complete raw body on this parse error.
- **Impact:** recovery of a successful create/storno with a corrupted artifact is harder than necessary. The error remains conservatively `OutcomeClass::Unknown` (`error.rs:752–768`), so this is not false “nothing issued” advice. Queries lose that observation too. Missing or blank PDF already retains identity with `pdf: None`.
- **Inspected reproduction:** `tests/upstream.rs:852–945` deliberately expects the published dots to fail base64 decoding, substitutes `JVBERi0=`, then checks all receipt blocks. `tests/receipt_wire.rs:44–64` separately asserts missing/blank PDF preservation. These tests were read, not run here.
- **Possible improvement:** retain successfully interpreted receipt data alongside an explicit PDF-decoding diagnostic. Do not silently label corrupt data as a valid PDF. Keep malformed receipt identity distinct from malformed optional artifact data.

Source-derived public-parser reproduction, suitable for the parent to run without a vendor call (not added to the test tree):

```rust
use szamlazz_agent::ops::receipt::{QueryReceipt, ReceiptSelector};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{ParseError, ResponseError};

// When placed beside existing integration tests:
let sample = include_str!("synthetic/xmlnyugtavalasz.xml");
let query = QueryReceipt::new(ReceiptSelector::ReceiptNumber("R".into()));
let with_pdf = |encoded: &str| RawResponse::new::<&str, &str>([],
    sample.replace("<nyugta>",
        &format!("<nyugtaPdf>{encoded}</nyugtaPdf><nyugta>"))
        .into_bytes());
assert!(query.parse(&with_pdf("JVBERi0=")).is_ok());
assert!(query.parse(&with_pdf(" ")).unwrap().pdf.is_none());
assert!(matches!(query.parse(&with_pdf("...")),
    Err(ResponseError::Parse(ParseError::Base64(_)))));
```

The XSD calls `nyugtaPdf` a `string`; that does not override the prose's base64/PDF semantics. The illustrative dots are not a valid receipt PDF. This is therefore not counted as an ordinary valid-response bug.

## Live primary-source acquisition register

All linked entries in this register were fetched during this review. Current main documentation pages display **`v202608271632`**; this is a site build label, not proof of the age of each assertion. Legacy pages linked by the knowledge base remain reachable with older build labels.

| Area | Live sources inspected | Coverage |
|---|---|---|
| Create | [Category][C-category], [request][C-request], [XML + inline XSD][C-xml], [response + inline XSD][C-response] | Action, full header/items/tenders, call ID, PDF, 336–340 |
| Storno | [Category][S-category], [request][S-request], [XML + inline XSD][S-xml], [response][S-response] | Number/template/call ID sequence, SN result and three refusal cases |
| Query | [Category][Q-category], [request][Q-request], [XML + inline XSD][Q-xml], [response][Q-response] | Both selectors, optional call ID/template, shared result |
| Email | [Category][E-category], [request][E-request], [XML + inline XSD][E-xml], [response + inline XSD][E-response] | All email leaves, container omission/resend, success/error verdict |
| Receipt rules | [Index][R-index], [NAV][R-nav], [order number][R-order], [PDF template][R-template], [erasure code][R-erasure], [item amounts][R-amounts] | Every one of the five rule pages |
| Shared rules | [Currencies][B-currency], [errors][B-errors], [sending requests][B-send], [authentication][B-auth], [session cookies][B-session], [invoice order rules][I-order], [VAT rates][I-vat] | Relevant dependencies; invoice rules not silently transferred to receipts |
| Knowledge base | [Order number][K-order], [erasure usage][K-erasure], reporting [HU][K-nav-hu] / [EN][K-nav-en], [NAV connection guide][K-nav-setup] | Linked operational constraints and conflicting rollout/settings wording |
| Hungarian checks | [Create XML][HU-C-xml], [create response][HU-C-response], [query XML][HU-Q-xml], [storno response][HU-S-response], [send XML][HU-E-xml], [order rule][HU-R-order], [amount rule][HU-R-amounts] | Conflicts are not merely English translation inference |
| Legacy links | [Receipt XSD][OLD-C-xsd], [receipt “other” rules][OLD-C-other], linked [invoice erasure information][OLD-I-info] and [invoice XSD][OLD-I-xsd] | Followed the old links actually reached from knowledge-base/rule pages |
| PHP | [Create][P-create], [storno][P-storno], [query][P-query], [send][P-send], [PDF][P-pdf], [changelog][P-changelog], [2.12.4 archive][P-zip] | Wrapper behavior, source comments and implementation, not live acceptance |
| Downloaded XSDs | [Create][X-create], [storno][X-storno], [query][X-query], [send][X-send], [receipt response][X-response], [send response][X-send-response] | All six documents read in full; self-contained receipt definitions |

**Broken schema-location links followed:** all four `https://www.szamlazz.hu/docs/xsds/` paths printed in the examples returned **404**: `nyugtast/xmlnyugtast.xsd`, `nyugtaget/xmlnyugtaget.xsd`, `nyugtasend/xmlnyugtasend.xsd`, `nyugta/xmlnyugtasendvalasz.xsd`. The working downloads are under `/szamla/docs/xsds/`; the send response specifically lives at **`nyugtasend/xmlnyugtasendvalasz.xsd`**, not `nyugta/` or `nyugtasendvalasz/` (both attempted `/szamla/` alternatives also returned 404). Create's sample supplies a single schema-location URI instead of a namespace/location pair. None of those hints is emitted by Rust; none is an operation field it must supply.

**PHP acquisition:** fresh archive SHA-256 **`30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`**. References below are relative to `PHPApiAgent-2.12.4/szamlaagent/`. Archive entries were read in memory; PHP was never run. A guessed `/php/nyugta-letrehozas` URL returned 403; the actual linked `/php/nyugta-generalas` page was successfully fetched.

## Request coverage table

“Covered” denotes representation and serialization against the cited wire definition, not acceptance of arbitrary values under account/business rules. R = required element, O = optional element. Authentication is a runtime requirement even though the individual credential leaves are optional in XSD.

### Operation/envelope coverage

| Operation | Multipart field | Root / response | Code |
|---|---|---|---|
| Create | `action-szamla_agent_nyugta_create` | `xmlnyugtacreate` → `xmlnyugtavalasz` / `Receipt` | `receipt.rs:189–191,223–295` |
| Storno | `action-szamla_agent_nyugta_storno` | `xmlnyugtast` → `xmlnyugtavalasz` / `Receipt` | `receipt.rs:340–366` |
| Query | `action-szamla_agent_nyugta_get` | `xmlnyugtaget` → `xmlnyugtavalasz` / `Receipt` | `receipt.rs:420–451` |
| Send | `action-szamla_agent_nyugta_send` | `xmlnyugtasend` → `xmlnyugtasendvalasz` / `()` | `receipt.rs:502–532` |

All match the four request pages and [shared action table][B-send]. Each namespace is exactly `http://www.szamlazz.hu/<root>`; namespace identity is independent of the **HTTPS** POST endpoint `https://www.szamlazz.hu/szamla/` (`wire.rs:14`). `xml.rs:157–178` supplies UTF-8 declaration/default namespace; `wire.rs:66–100,405–411` builds one XML file part, `text/xml`, CRLF framing and multipart content type. Receipt operations add no other files. `Client::send` posts that body (`client.rs:374–405`).

All four place credentials in `beallitasok`: either `szamlaagentkulcs` or `felhasznalo` then `jelszo` (`xml.rs:628–637`). Create/storno/query always emit R boolean `pdfLetoltes`, default false. Send has no such field. **None of the receipt schemas declares `valaszVerzio`**; omitting an invoice-style response-version selector is correct. The general plain-text-error page explicitly lists invoice/storno/credit/PDF-query operations, not receipts.

### Create: every declared request field

| Wire path under `xmlnyugtacreate` | Occurrence / public representation | Writer / conclusion |
|---|---|---|
| `beallitasok/{felhasznalo,jelszo,szamlaagentkulcs}` | O individual leaves; shared `Credentials` | `228–230`; covered in both auth forms |
| `beallitasok/pdfLetoltes` | R; `download_pdf: bool` | `230`; always emitted |
| `fejlec/hivasAzonosito` | O; `call_id: Option<String>` | `233`; no generated/rotated identity |
| `fejlec/elotag` | R; `prefix: String` | `234`; prefix stock/format delegated to server |
| `fejlec/fizmod` | R; open `PaymentMethod` | `235`; free-text methods remain possible |
| `fejlec/penznem` | R; open `Currency` | `236`; includes Ft and non-ISO vendor spellings such as KSH |
| `fejlec/devizabank`, `devizaarf` | O in XSD; `ExchangeRate.bank`, `.rate` | `237–242`; correct receipt names; foreign-currency validation below |
| `fejlec/megjegyzes` | O; `comment` | `243`; covered |
| `fejlec/pdfSablon` | O; `template` | `244–246`; A/J/L/N and `Other` |
| `fejlec/fokonyvVevo` | O; `ledger_customer` | `247`; covered |
| `fejlec/rendelesSzam` | O; `order_number` | `248`; correct capitalization and example-tail position |
| `tetelek/tetel` | R, 1..unbounded; `items` | `193–199,250–277`; zero rows refused before sending |
| `tetel/megnevezes`, `azonosito` | R name / O item ID; `LineItem.name`, `.id` | `256–257`; covered |
| `tetel/mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | R; Decimal / string / Decimal | `258–260`; covered |
| `tetel/afakulcs` | R string; `VatRate` | `261`; numeric and special/open tokens |
| `tetel/netto`, `afa`, `brutto` | R; explicit Decimal amounts | `262–264`; receipt spelling, never `…Ertek` on output |
| `tetel/fokonyv/{arbevetel,afa}` | O container and leaves; shared ledger subset | `265–270`; full receipt ledger coverage |
| `tetel/megjegyzes` | O; `LineItem.comment` | `271`; covered |
| `tetel/torloKod` | O nonnegative int; `erasure_code_count: Option<u32>` | `200–207,272–274`; 0..400 locally; inline/download conflict C1 |
| `kifizetesek/kifizetes` | O container, 1..unbounded children; `payments` | `278–287`; empty vector omits container |
| `kifizetes/fizetoeszkoz`, `osszeg`, `leiras` | R tender / R amount / O description | `282–284`; `ReceiptPayment` retains all three |

`CreateReceipt::new` sets only actual constructor requirements and defaults options absent (`163–186`). PHP cash/Ft defaults are wrapper defaults, not wire-mandated Rust defaults. The `leiras` example annotation says “double,” but both inline/download schemas say **string** and the example contains `OTP SZÉP kártya`; Rust's `Option<String>` is correct.

All shared `LineItem` fields (`item.rs:90–127`) were traced. Unsupported receipt fields are refused explicitly at `receipt.rs:655–685`: `margin_vat_base`, `ledger.economic_event`, `ledger.vat_economic_event`, `ledger.settlement_from`, `ledger.settlement_to`. They cannot silently disappear from a checked request. The remaining fields all reach the wire. `write_xml` is explicitly unchecked; callers wanting validation use `to_wire`/`Client::send` (`wire.rs:367–373,405–411`).

### Storno, query and email

| Wire fields | Public representation and code | Assessment |
|---|---|---|
| Storno R `nyugtaszam`, O `pdfSablon`, O `hivasAzonosito` | `StornoReceipt`, `receipt.rs:314–359` | Complete; exact **number → template → call ID** sequence |
| Query O `nyugtaszam`, O `rendelesSzam` | `ReceiptSelector`, `369–395,433–439` | Exactly one alternative; stronger than XSD's two optional leaves and consistent with request prose/PHP |
| Query O `hivasAzonosito`, O `pdfSablon` | `call_id`, `template`, `399–416,440–443` | Covered; normal lookup omits call ID; no call-ID-only selector claimed |
| Send R `nyugtaszam` | `SendReceipt.receipt_number`, `483–499,512–514` | Existing receipt by number; no order selector documented |
| Send O `emailKuldes`, with O `email`, `emailReplyto`, `emailTargy`, `emailSzoveg` | `ReceiptEmail`, `454–470,515–522` | All leaves represented, exact sequence; sending abstraction always includes container |

**Schema sequence:** create and query use **`xs:all`**, including their settings/header/root and create item/ledger/tender fields. The examples' statement “the order of the fields is fixed, they cannot be interchanged” is stronger than those XSDs. The bank/rate declaration order is not a serializer defect: the example and Rust put bank before rate, and `all` allows either. Storno and send use real **`xs:sequence`**, and Rust matches every sequence, including credential order and all optional combinations. PHP's receipt item writes comment before ledger (`Item/ReceiptItem.php:65–74`), whereas Rust follows the example's ledger-before-comment; both fit `all`.

## Payment, amount, template and exchange-rate rules

1. **Tenders are optional, not independently registered credit entries.** [Create XML][C-xml]: “The `<kifizetesek>` section … is not mandatory, but if present, then the sum of the values should be equal with the total amount of the receipt.” Code **340** means paid amount differs from gross. Rust explicitly delegates that equality to the server (`receipt.rs:156–160`) and writes each supplied amount unchanged. No documented rule requires each tender name to equal header `fizmod`.
2. **No wire five-tender cap is established.** Both create/response XSDs declare `kifizetes` with `maxOccurs="unbounded"`. Fresh PHP source has `Document/Receipt/Receipt.php:26` **`CREDIT_NOTES_LIMIT = 5`**, and `addCreditNote` silently stops appending at five (`118–121`), but `setCreditNotes` directly replaces the array (`134–135`) and the writer iterates it all (`250–255`). This is inconsistent wrapper behavior, not proof that the Számla Agent refuses six tenders. Rust's vector follows the wire definitions; a six-tender live result remains unknown.
3. **HUF/Ft item constraints:** [amount rules][R-amounts] explicitly require whole gross, net and VAT at most two decimal places, and **exact** net + VAT = gross; unit-price × quantity and net × VAT-rate checks have a separately stated **2 HUF** tolerance. The page says these restrictions do not apply to foreign currencies. Rust does not duplicate these business checks. `LineItem::new` preserves the documented `787.40 / 212.60 / 1000`; `try_calculated` has explicit rounding, with net rounded before VAT and gross derived by exact addition (`item.rs:183–222`). `Rounding::Scale(2)` can still produce fractional gross; HUF minor-unit rounding is a stricter local choice, correctly explained at `receipt.rs:104–108`, `item.rs:9–18,74–87`.
4. **Numeric fidelity:** `number.rs:6–59` refuses unrepresentable intermediate arithmetic instead of silently rounding; `number.rs:63–140` parses finite decimal/scientific wire values exactly within Decimal's domain. `xml.rs:647–664` uses it for receipt amounts/rate. INF/NaN and finite values outside Decimal's range/precision are refused, even though XSD `double` has a broader value space. This is a deliberate domain boundary, not evidence an ordinary vendor receipt fails.
5. **VAT/open sets:** receipt request code families (including EU/EUK/MAA/ÁKK) remain available; numeric percentages and future tokens are not closed off (`types.rs:178–366`). Response `afatipus` takes precedence over numeric `afakulcs`, retaining both raw fields (`receipt.rs:646–652`, `types.rs:1087–1093`). Download-only TEHK survives as `Other`. No migration to HO is invented. The invoice VAT guide's tax meanings do not establish additional receipt request fields such as `eusAfa` or buyer tax data.
6. **Templates:** A = A4, J = ticket, L = ticket with logo, N = 80 mm, matching [template rules][R-template]. Omission/empty uses A; [create XML][C-xml] additionally states invalid token → A. `ReceiptTemplate::Other` preserves tokens; serde snake-case request enum names are independent of XML's hand-written mapping. Actual rendering for any of the four templates was not tested.
7. **Erasure count:** [receipt rule][R-erasure] says nonnegative int, enable in settings or **539**, maximum **400** or **537**. [General errors][B-errors] adds **538**, unavailable in demo/test accounts. The [linked HU knowledge base][K-erasure] says `torloKod` carries **“az igényelt kódok darabszámát”** (the number of requested codes), not an existing code identifier. Rust's field name/type/bound are correct. The linked article's `SzlaMost` requirement is specifically for invoices; Rust does not impose an invoice template on receipts.
8. **Foreign currency:** [currencies][B-currency] says “the exchange rate and the name of the bank … must also be provided,” using **`devizaarf` / `devizabank`** for receipts. Validation requires a rate structure and a nonblank unpadded bank for non-HUF, allowing numeric omission only with exact `MNB` (`receipt.rs:208–218`). Explicit zero/negative rates are left to the server; zero cannot simply be prohibited because first-party PHP comments describe it as an automatic-rate signal. HUF/Ft detection is case-insensitive while wire spelling is preserved (`types.rs:401–410`); server acceptance of all lower-case spellings was not established here.

### Automatic MNB: supporting evidence, not an executed receipt experiment

In [PHP 2.12.4][P-zip], `src/szamlaagent/Header/ReceiptHeader.php:62–79` says:

> “Ha 'MNB' és nincs megadva az árfolyam ($exchangeRate), akkor az 'MNB' aktuális árfolyamát használjuk a bizonylat elkészítésekor.”

Translation: with MNB and no supplied exchange rate, the current MNB rate is used when creating the document. Lines 74–75 additionally describe omitted/zero rate when the currency exists in MNB's database. `buildFieldsData:232–233` permits bank without numeric rate. The custom receipt example's line 43 repeats the omitted-rate assertion, but line **44 explicitly sends `300.0`**. It is not an example execution of omission.

`ExchangeRate::automatic_mnb` (`types.rs:966–980`) already accurately labels the receipt-specific PHP evidence and lack of account-probe proof. Retaining this supported option is justified despite the general XML prose's stricter wording. Neither a unit test nor XSD optionality proves the server looked up a rate.

## Email container semantics

The [EN send XSD comment][E-xml] says **“If omitted, no e-mail is sent.”** The [HU version][HU-E-xml] agrees: **“Ha nincs megadva, nem kerül e-mail kiküldésre.”** Inside the sample's present `emailKuldes`, the comment says **“e-mail details, if not defined, the previous e-mail will be sent.”** These distinguish container absence from absent details; they need not be read as a contradiction.

| Rust state | Actual XML | Documented meaning / limit |
|---|---|---|
| `email: None` | `<emailKuldes></emailKuldes>` | Present empty block requests previous-email resend |
| `Some(ReceiptEmail::default())` | Same present empty block | Same wire request |
| A child `None` | Child omitted | Optional leaf; independent inheritance not established |
| A child `Some("")` | Explicit empty child | Distinct from omission; clearing/default/fallback meaning not established |
| All four details populated | Full ordered block | Documented first-send path |
| Absent container | Not produced by `SendReceipt` | Vendor describes no mail; an intentionally sending/resending abstraction need not expose a no-op |

Code: `receipt.rs:454–459,486–498,515–522`; `xml.rs:574–604`. `tests/receipt_wire.rs:66–154` specifically observes container presence, avoiding an outline comparator that collapses absent and empty. `tests/schema_requests.rs:663–691` exports all 16 child-presence masks with populated and empty strings plus default resend, under both authentication forms.

**PHP is not proof of resend:** `Document/Receipt/Receipt.php:214` drops `emailKuldes` if its built details array is empty; `265–277` drops blank individual values. It therefore does not emit Rust's present-empty resend request. The full-detail [PHP send example][P-send] supports field mapping only. Its archive comment (`examples/document/receipt/send_receipt.php:17–18`) claims a PDF in a send response, but both current inline/download send response schemas have only verdict/code/message. Rust's `()` return follows those definitions, not that stale example comment.

First-send missing subject is concretely documented as **code 7**, `Hiányzó adat: emailtargy elem.` ([send response][E-response]); XSD-optional leaves do not imply a first send can omit all necessary details. No receipt-specific source establishes comma/semicolon multi-recipient syntax, per-field merging, BBCode support, inbox delivery, or an email-delivery query. Invoice email rules do not fill those gaps. A lost acknowledgement must not be resolved by checking receipt existence (`recovery.md:20`).

## Complete response coverage

Create/storno/query all call the same parser (`receipt.rs:293–295,364–366,449–451,690–701`). [Query response][Q-response] delegates to creation response. [Storno response][S-response] explicitly says **“the data of the storno receipt, not the original receipt”**, with `tipus=SN` and its own PDF.

| Wire field/group under `xmlnyugtavalasz` | Public projection / parser location | Optionality and behavior |
|---|---|---|
| `sikeres`, `hibakod`, `hibauzenet` | Shared `Verdict`, `xml.rs:461–554` | Required verdict; optional code/message; errors before payload |
| `nyugtaPdf` | `Receipt.pdf`, `receipt.rs:693–710` | Optional base64; absent/blank → None; corrupt → R-H1 |
| `nyugta` | `ReceiptBody.nyugta`, `691–692,704–720` | XSD optional across success/failure; required by successful-operation prose and parser |
| `alap/id` | `Receipt.id`, `727,755` | Required integer, i64 wider than XSD int |
| `alap/hivasAzonosito` | `.call_id`, `728,756–761` | Optional creation identity |
| `alap/nyugtaszam`, `tipus` | `.receipt_number`, `.document_type`, `729–730,762–763` | Required strings/open ReceiptType; NY/SN/Other |
| `alap/stornozott` | `.reversed`, `731,764–765` | Required boolean fact; true/false/1/0; absent/blank is not false |
| `alap/stornozottNyugtaszam` | `.reversed_receipt_number`, `732,766–771` | Optional original number named by SN, not its own number |
| `alap/kelt` | `.issue_date`, `733,772–773` | Required civil date; complete valid timezone discarded |
| `alap/fizmod`, `penznem` | `.payment_method`, `.currency`, `734–735,774–775` | Required open values |
| `alap/devizabank`, `devizaarf` | `.exchange_bank`, `.exchange_rate`, `736–737,776–779` | Optional string/exact Decimal |
| `alap/megjegyzes`, `fokonyvVevo` | `.comment`, `.ledger_customer`, `738–739,780–787` | Optional business text |
| `alap/teszt` | `.test`, `740,788–789` | XSD required, model Option; missing/empty unknown, never assumed live |
| `alap/rendelesSzam` | `.order_number`, `741,790–795` | Optional business text |
| `tetelek/tetel/megnevezes`, `azonosito` | `ReceiptItem.name`, `.id`, `806–808,842–843` | Required name / optional identifier |
| `tetel/mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | `.quantity`, `.unit`, `.unit_price`, `809–817,844–846` | Required Decimal/string/Decimal |
| `tetel/afatipus`, `afakulcs` | `.vat_type`, `.vat_rate_code`, `818–820,847–848` | Optional category; required raw rate text; category precedence |
| `tetel/netto`, `afa`, `brutto` | `.net_value`, `.vat_value`, `.gross_value`, `821–826,849–851` | Required Decimal; published `…Ertek` aliases also accepted |
| `tetel/fokonyv/{arbevetel,afa}` | `ReceiptItemLedger`, `827–837,852–855` | Optional container/leaves, complete projection |
| `kifizetesek/kifizetes/{fizetoeszkoz,osszeg,leiras}` | `Receipt.payments`, `743–746,860–881` | All tender fields; missing container → empty vector |
| `osszegek/afakulcsossz/{afatipus,afakulcs,netto,afa,brutto}` | `Totals.by_vat_rate`, `xml.rs:833–861,877–895` | Repeated subtotal; optional category and required rate/amounts |
| `osszegek/totalossz/{netto,afa,brutto}` | `Totals.total`, `xml.rs:840–841,863–875,898–905` | Required grand total/amounts |

No declared response field is missing. In particular, **neither response schema nor response example declares item `megjegyzes` or `torloKod`**, so the lack of those fields in `ReceiptItem` is not a confirmed omission. Request support does not prove an echo. No seller/buyer party block, invoice external ID, email-delivery record, outstanding-balance field or NAV-reporting status is declared in this receipt response. PHP's smaller object projection is not a completeness target for Rust.

### Envelope, PDF, verdict and errors

- **Precedence:** nonblank `szlahu_down` → nonblank error-code header → known non-2xx status → UTF-8/expected root and namespace/complete XML → body verdict → payload (`wire.rs:251–310`, `xml.rs:528–554`). Body-only HTTP-200 errors work; at non-2xx without decisive header, the result is `HttpStatus`, classified Unknown. This is explicit shared policy, not an observed receipt header matrix.
- **Complete XML and namespaces:** `xml.rs:37–154,201–300` checks bindings, structure through EOF, declaration and lexical syntax; DTD is refused. `protocol_text:310–377` filters foreign subtrees, preserves protocol aliases and keeps a placeholder so foreign children cannot concatenate text into a manufactured scalar. Serde then recognizes fields by parent path and rejects duplicate recognized singleton fields. Namespace/list controls include all three receipt data operations and email verdict tests.
- **Verdict:** true/1 succeeds; false/0 returns code/message without requiring receipt data. Empty verdict is currently read as false by shared `flexible_bool` (`xml.rs:769–778`), unlike the required receipt reversal fact; absent verdict fails deserialization. Unknown/absent codes remain conservative. A readable known code with empty verdict is a malformed-input policy, not evidence of a valid-source mismatch. A nonempty code alongside `sikeres=true` is ignored by `Verdict::api_error`, while an error header wins; contradictory-channel emission is unestablished.
- **Diagnostics:** `Verdict::parse:478–499` reads code/verdict separately from optional message, allowing an unreadable duplicate/nested message to become unavailable without losing a readable refusal. It does not choose an arbitrary one of duplicate messages. Other unknown well-formed extensions are ignored.
- **PDF:** whitespace-wrapped standard base64 is decoded (`types.rs:110–116`), but content is not checked for `%PDF-` or renderability. `Pdf` is a byte container, not a validator. A requested-but-missing PDF still yields the receipt and `None`; this preserves stronger document evidence rather than inventing an artifact. Raw PDF-only data is not a documented receipt response mode.
- **No receipt numbered-56 special case:** create/storno/query use `xml::valasz`, send uses `xml::verdict`. Receipt docs do not establish invoice notification-warning success semantics, so an error 56 remains an error/Unknown. `error.rs:368–370`'s generic “56 surfaces … only when … no document number” wording should be understood in its invoice-issuing context; it is not an invariant across these receipt parsers. This is a documentation precision opportunity, not grounds to import an unsupported receipt warning mode.
- **Known receipt errors:** 336 prefix already used for invoices, 337 invalid prefix, 338 duplicate call ID, 339 absent receipt, 340 tender mismatch, 363 whole gross, 364 net precision, 365 VAT precision; erasure 537/538/539. All mapped at `error.rs:155–190,244–256,296–308`; 339 is NotFound, others listed here Rejected (`375–421`). Open errors, absent code, parse/status/transport/unavailability remain Unknown.
- **Code 7 is contextual:** the missing-subject send example is not proof of an absent receipt. `ErrorCode::MissingData` and `OutcomeClass::NotFound` explicitly describe this broader scope (`error.rs:43–48,450–454`, `recovery.md:17`). The current contract is therefore not contradicted merely by its broad variant name.
- **Storno refusal numbers are not assigned by the page.** Its messages cover nonexistent original, already reversed original and a storno-receipt target. They begin “Hiányzó adat,” but that wording alone does not establish code 7 or 339. The synthetic storno-339 test verifies parser propagation, not the vendor's assignment.
- **Transport:** body-transfer failure retains status/headers in `IncompleteResponse`, remains Unknown and never constructs a fake empty completed response (`client.rs:66–100,117–125,385–403`). Native default timeout is 60 s, redirects disabled, jar reused only within a client/account. There is no application recovery/retry loop; injected HTTP retry policies remain active. [Vendor error rules][B-errors] say at most five sends of the same request, then stop for operator intervention; no precise combined write-plus-query budget is published.

## Call ID and recovery

| Operation | Established source rule | Current implementation/documentation | Remaining boundary |
|---|---|---|---|
| Receipt create | [C-response]: “If this field is in use, it needs to be unique, otherwise the API call will be unsuccessful”; 338 “Caller ID … already exists on the server” | Optional caller-owned ID; persist before send; keep throughout logical issuance; recover by known number or deliberately managed order (`receipt.rs:94–102,118–122`, `recovery.md:15`) | Not replay-success idempotency; no original number/PDF promised by 338; retention/scope/concurrency unspecified |
| Receipt storno | [S-response]: SN result; already-reversed and SN-target refusals | Stable logical call identity; original query can establish reversal but not recover SN number/PDF (`receipt.rs:298–310,322–325`) | No storno-specific 338 guarantee; no invoice-style replay promise |
| Query | [Q-request]/[Q-xml]: number or order; optional call identifier | `ReceiptSelector` has exactly those alternatives; normal `call_id=None` (`receipt.rs:369–404`) | Field declaration does not establish call-ID lookup/filter behavior |
| Order lookup | [P-query]: “same ‘last matching document’ behaviour as invoice queries” | Documented in selector/recovery rustdoc | Meaning of last, NY/SN choice after storno, collisions and normalization unresolved |
| Email | [E-response]: success/failure verdict | Lost acknowledgement leaves delivery unresolved (`receipt.rs:473–479`, `recovery.md:20`) | Document existence says nothing about email; repeat may duplicate delivery |

The parser returns the **reported** receipt, not proof of a match to the request. It does not enforce by-number echo equality, SN type/original-reference agreement, or financial consistency. These checks belong to adoption/recovery logic and the fields needed are exposed. The README example checks call ID, order, NY type, reversal and contradictory original reference (`README.md:196–209`). A generic parse result must not be treated as a receipt-specific storno verdict without checking those facts.

An immediate empty query, elapsed time or a refusal to a later exchange does not settle an earlier send. `OutcomeClass::Rejected` explicitly classifies this exchange only, including 338 (`error.rs:438–440`, `recovery.md:4–9`). Missing call-ID-only/internal-ID/external-ID selectors, order-based storno/send, receipt edit/delete or post-creation tender registration are not gaps in the reviewed operation definitions. The creation error table's parenthetical “query, send or delete” for 339 does not define a new public receipt-delete protocol.

## Confirmed source contradictions and justified deviations

These are independently verified disagreements in live documents/source. Their operational significance is not asserted as a live server result.

| ID | URLs and exact evidence | Affected code / disposition |
|---|---|---|
| C1 — schema coverage | [EN/HU create XML][C-xml] / [HU][HU-C-xml] declare `<element name="torloKod" … minOccurs="0">`; [download][X-create] has no such declaration. [EN/HU query XML][Q-xml] / [HU][HU-Q-xml] declare `<element name="rendelesSzam" type="string" … minOccurs="0">`; [download][X-query] has no such declaration. | `receipt.rs:272–274,438`. Keep documented capabilities. Download-only validation rejects them, but that is not proof Rust sends unsupported fields. PHP item writer and query header also emit them. |
| C2 — order prose vs schema | [C-xml]/[Q-xml]: “order of the fields is fixed”; both schemas use `<all>`. Create example bank → rate, declaration rate → bank. | `receipt.rs:237–242,424–445`; Rust follows examples and genuine sequences. `all` declaration position must not be mistaken for required order. |
| C3 — response VAT vocabulary | [Download response][X-response] includes `<enumeration value="TEHK">`; [inline response][C-response] does not. [PHP changelog][P-changelog] says 2.10.22 removed VAT_TEHK. | Open `VatRate::Other` preserves the token. No named-variant requirement or automatic substitution follows. |
| C4 — receipt repetition toggle | [R-order]/[HU-R-order]: “own toggle, separate from the invoice setting” / “külön kapcsoló … függetlenül”; [K-order]: “bizonylattípusonként nem állítható be külön.” [Legacy receipt rules][OLD-C-other] likewise say the same setting applies to other types. | `receipt.rs:142–147`, `recovery.md:31–34`, README follow the current Agent-specific rule. This is source-backed, but actual independence and post-storno reuse need confirmation. |
| C5 — MNB omission | [Currencies][B-currency]: bank and rate “must also be provided”; [PHP archive][P-zip] `ReceiptHeader.php:65` explicitly says MNB + no rate uses current MNB. | `receipt.rs:208–218`, `types.rs:966–980`. Justified documented exception; account execution remains unknown. |
| C6 — NAV rollout | [R-nav]: “there is nothing you need to do for now” / “working on automating”; [K-nav-en]: “From 1 September 2026 … automatically”; [K-nav-hu]: “2026. szeptember 10-től automatikusan továbbítja … visszamenőleg”, with connection/permission requirements. [Setup step 13][K-nav-setup] lists “Hozzáférés a nyugtaadat-szolgáltatási interfészhez.” | `README.md:167–174` already reflects the more specific HU rollout and setup, and says issuance is not reporting proof. No missing receipt XML setting/status is established. |
| C7 — illustrative response is internally inconsistent | [C-response] literally shows PDF `...`, empty int `hibakod`, second-item `nettoErtek/afaErtek/bruttoErtek`, NY + unreversed + original-number reference, two item gross values of 25,400, tenders 1,000 + 3,000, grand gross 254. | `receipt.rs:821–826` deliberately accepts amount aliases; parser retains reported amounts rather than “repairing” totals. Empty code is absent. Dots produce R-H1. Repaired examples are not live financial records. |
| C8 — arithmetic explanation | [R-amounts]/[HU-R-amounts] claim `787.40157480315 + 212.59842519685 = 999.999999...`. Their exact decimal sum is **1000.00000000000**. | The two operands still exceed allowed HUF precision. Rust should not encode the erroneous sum or infer a universal first-error ordering from this example. No receipt probe establishes the server's numerical comparison implementation. |
| C9 — PHP tender cap and resend shape | [PHP archive][P-zip] `Receipt.php:26,118–121` caps one add helper at five, while `134–135` bypasses it; `214` omits an empty email block. [C-xml] permits unbounded tenders; [E-xml] distinguishes absent-block no-send from no-detail resend. | Rust follows the explicit wire contract rather than those wrapper choices. Six-tender acceptance and resend delivery still require evidence. |
| C10 — PHP send-PDF comment | [PHP archive][P-zip] `examples/document/receipt/send_receipt.php:17–18` says the response contains PDF; [E-response]/[X-send-response] define verdict/code/message only. | `receipt.rs:504,527–532` returning `()` is justified. PHP documentation is supporting evidence, not automatically the strongest source. |

Additional deliberate parser choices: missing/empty test marker remains unknown; empty item/tender/subtotal collections are tolerated despite XSD child minima; optional XML-whitespace-only business text is absent while other decoded characters, including NBSP/padding, survive (`xml.rs:745–756`). Required number strings may be blank because string types carry no nonblank constraint; that does not make them usable recovery identities. Required reversal is stricter and refuses absent/empty data. Civil Date has a finite range, keeps legacy accepted spellings (including some outside XSD 1.0) and discards only a complete supported timezone suffix. These choices are not a claim that the parser performs full XSD validation.

## Recorded observations crosscheck

**No receipt-specific executed evidence was found in the requested repository records.** This is not a claim that receipts have never been used elsewhere.

- `docs/szamlazz-hu-behaviour.md:3–26,42–107,165–172` records invoice-family observations on particular TEST accounts/days. Invoice B4 repeated-storno success, B8 removal of original credit entries, order-number replay/reuse, external-ID newest-holder rules and P60 rounding/storage observations **do not establish receipt behavior**. The receipt `stornozott` spelling/type must not inherit the invoice `sztornozott` optionality.
- The dated `docs/research/2026-09-11-credit-clearing-live.md:12–32,69–75` records actual populated/empty **invoice credit-entry clearing** and explicitly says **“Receipt and combined-preview probes were not executed.”** This is stronger than inferring execution from existing probe code, and it settles no receipt tender rule.
- `docs/research/2026-09-11-agent-vendor-clarification.md:3,64–85` is an **unsent draft**, not a vendor answer. Its missing `torloKod`/query order declarations were independently confirmed by today's live fetches.
- `docs/research/2026-09-10-agent-vendor-questions.md:58–72` records the order-toggle conflict and no receipt lifecycle execution. The conflict still exists, including in the freshly followed legacy rule page.
- `docs/testing.md:244–264` describes receipt probes as documentation hypotheses and requires dated results before promoting them into verified behavior.

### Existing probes inspected, not executed evidence

| Probe | Source and actual assertions | What it does not establish |
|---|---|---|
| `receipt_lifecycle` | `tests/probes/receipts.rs:176–241`: HUF fractional-net create, number/order/ID/call ID/type/test checks, sums and PDF signature, deliberate repeat of completed create expecting 338, storno with original/SN queries | Unknown-answer recovery, concurrent call-ID races, retention, repeated storno refusal numbers, post-storno order selection |
| `receipt_automatic_mnb` | `:243–264`: EUR, MNB with omitted rate; query requires positive rate/bank, currency and gross | Actual omitted-rate acceptance until run; arbitrary currency precision, zero-rate equivalence, historical-rate selection |
| `receipt_email_resend` | `:266–312`: all four details, then present-empty resend, both acknowledged; manual inbox instruction | Inbox delivery or inherited contents from protocol success alone; absent-block no-op, empty-string/partial-field semantics |
| Cleanup helper | `:115–172`: verify original order/type; reverse known number; query SN original reference and original reversal; defer cleanup on uncertainty | A general durable recovery mechanism or permission to blindly rerun an interrupted probe |

## Tests and verification machinery inspected

**Execution status for every row: not run here; parent suite owns execution.** These rows describe assertions present at HEAD, not passing results.

| Test/source | Inspected coverage |
|---|---|
| `receipt.rs:929–980` | Civil-date forms for create/storno/query; timezone boundaries, invalid calendar/suffix/multibyte controls |
| `receipt.rs:986–1245` | Four canonical XML goldens; foreign rate/MNB emission, empty items, bank validation, receipt metadata and invoice-only refusal, template/call-ID sequence, both query selectors, empty resend |
| `receipt.rs:1248–1479` | All receipt data blocks, tender/totals, missing test marker, PDF decode, JSON roundtrip, body/open errors and header precedence |
| `tests/receipt_wire.rs:12–244` | Required reversal fact, blank PDF identity retention, email container/child presence, normal query call-ID omission, explicit fractional HUF values |
| `tests/upstream.rs:852–961` | Literal source placeholder expected to fail; explicit PDF repair before field assertions; both item amount spellings; send success/error source examples |
| `tests/business_text.rs:49–85` | Nonblank receipt call/order/reference/bank/comment/ledger/tender text preserved; XML-whitespace absence vs NBSP |
| `tests/numeric_fidelity.rs:15–185` | Scientific/padded numeric VAT, raw-token retention, category precedence, underflow/precision-loss refusal including receipt quantity |
| `tests/error_classification.rs:49–242` | Documented receipt code mappings, synthetic query/storno/send 339, contextual code 7 and open-code controls; not vendor-emitted-code evidence |
| `tests/response_namespaces.rs:36–115` and shared controls | Repeated receipt rows under aliases/interleaved extensions; create/storno/query same result; foreign email verdict cannot establish success; shared reserved bindings/attributes/scalars |
| `tests/response_completion.rs:12–87,172–193` | Complete XML document, malformed prefix/tail/truncation controls, receipt query included |
| `tests/response_headers.rs:9–40,148–224` | Shared diagnostic/precedence policies, primarily invoice-operation controls; not a receipt live-header matrix |
| `tests/literals.rs:105–141` | External consumer construction of `SendReceipt` and optional email data |
| `item.rs:232–440`, `xml.rs:916–1112`, `wire.rs:448–647` | Exact arithmetic/error boundaries, rounding, escaping, verdict handling, multipart framing/collision and header handling |
| `tests/schema_requests.rs:31–91,572–691` | Generated receipt request matrix through checked multipart boundary, both auth forms, all templates/PDF/call-ID combinations and selectors, optional fields, empty ledger, erasure 0/400, email masks |
| `scripts/check-agent-schemas.py:20–26,59–85,88–174` | Real offline `xmllint` XSD validation of EN-inline/download corpora, hashes, exact expected-conflict diagnostics, field-path coverage only from fully valid cases, negative controls |

The schema matrix explicitly expects only `torloKod` and `rendelesSzam` download conflicts for receipts (`tests/schema_requests.rs:48–56`). It does not patch schemas to become green. Its multiple-item receipt row can have nonmatching tender sums; XSD validity is not business acceptance. The script's negative controls are shared and mostly invoice-specific, not a receipt payment/rule oracle. Response schemas, HU sources, rendered artifacts and server acceptance are outside that executable check (`docs/testing.md:144–154`).

### Follow-up evidence with highest value

1. Vendor confirmation of the effective create/query schemas and reconciliation of the independent-toggle claim; exact post-storno order reuse and NY/SN order selection.
2. Dated receipt lifecycle execution preserving complete request/response evidence: create with/without PDF, duplicate stable ID, both query selectors, SN and original verification, already-reversed/SN-target refusal codes. The existing lifecycle covers only part of this matrix.
3. Call-ID retention/scope/concurrency and query call-ID behavior; no automatic fresh identity while unresolved. Neither PHP nor XSD answers these.
4. Omitted-rate MNB receipt execution, explicit zero behavior, foreign-currency stored precision and supported lower-case currency handling.
5. Email first-send/resend plus an operator-observed inbox record; separately contrast absent block, empty block, explicit empty leaves and partial overrides. Determine recipient-list support and test-account routing specifically for receipts.
6. Tender cardinality beyond five, negative/zero tenders, and tender data on original/SN after reversal. Do not borrow invoice credit-entry behavior or PHP helper limits.
7. Rendered A/J/L/N PDFs and erasure-code behavior under an appropriately configured account. Existing `%PDF-` checks prove only a signature; test-account 538 restricts erasure experiments.
8. NAV reporting confirmation through an appropriate reporting surface; receipt issuance success alone exposes no reporting status.
9. Optional offline hardening: R-H1's typed partial-artifact result, explicit receipt tests for contradictory known identity/type/original reference, and a documented policy for malformed empty verdict plus readable refusal code. These are separate from compatibility with ordinary published replies.

## References

[C-category]: https://docs.szamlazz.hu/agent/category/generating-a-receipt
[C-request]: https://docs.szamlazz.hu/agent/generating_receipt/request
[C-xml]: https://docs.szamlazz.hu/agent/generating_receipt/xml
[C-response]: https://docs.szamlazz.hu/agent/generating_receipt/response
[S-category]: https://docs.szamlazz.hu/agent/category/reversing-a-receipt
[S-request]: https://docs.szamlazz.hu/agent/reversing_receipt/request
[S-xml]: https://docs.szamlazz.hu/agent/reversing_receipt/xml
[S-response]: https://docs.szamlazz.hu/agent/reversing_receipt/response
[Q-category]: https://docs.szamlazz.hu/agent/category/querying-a-receipt
[Q-request]: https://docs.szamlazz.hu/agent/querying_receipt/request
[Q-xml]: https://docs.szamlazz.hu/agent/querying_receipt/xml
[Q-response]: https://docs.szamlazz.hu/agent/querying_receipt/response
[E-category]: https://docs.szamlazz.hu/agent/category/sending-a-receipt
[E-request]: https://docs.szamlazz.hu/agent/sending_receipt/request
[E-xml]: https://docs.szamlazz.hu/agent/sending_receipt/xml
[E-response]: https://docs.szamlazz.hu/agent/sending_receipt/response
[R-index]: https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules
[R-nav]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting
[R-order]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number
[R-template]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template
[R-erasure]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code
[R-amounts]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts
[B-currency]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies
[B-errors]: https://docs.szamlazz.hu/agent/basics/error-handling
[B-send]: https://docs.szamlazz.hu/agent/basics/sending-requests
[B-auth]: https://docs.szamlazz.hu/agent/basics/authentication
[B-session]: https://docs.szamlazz.hu/agent/basics/session-cookie
[I-order]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number
[I-vat]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates
[HU-C-xml]: https://docs.szamlazz.hu/hu/agent/generating_receipt/xml
[HU-C-response]: https://docs.szamlazz.hu/hu/agent/generating_receipt/response
[HU-Q-xml]: https://docs.szamlazz.hu/hu/agent/querying_receipt/xml
[HU-S-response]: https://docs.szamlazz.hu/hu/agent/reversing_receipt/response
[HU-E-xml]: https://docs.szamlazz.hu/hu/agent/sending_receipt/xml
[HU-R-order]: https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/order-number
[HU-R-amounts]: https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/item-amounts
[OLD-C-xsd]: https://docs.szamlazz.hu/hu/agent/generating_receipt/xsd
[OLD-C-other]: https://docs.szamlazz.hu/hu/agent/generating_receipt/other
[OLD-I-info]: https://docs.szamlazz.hu/hu/agent/generating_invoice/important-information#adattorlokod
[OLD-I-xsd]: https://docs.szamlazz.hu/hu/agent/generating_invoice/xsd
[K-order]: https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan
[K-erasure]: https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor
[K-nav-hu]: https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg
[K-nav-en]: https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting
[K-nav-setup]: https://www.szamlazz.hu/nav-online-szamlazas-regisztracios-segedlet/
[P-create]: https://docs.szamlazz.hu/php/nyugta-generalas
[P-storno]: https://docs.szamlazz.hu/php/sztorno-nyugta-generalas
[P-query]: https://docs.szamlazz.hu/php/nyugta-lekerdezes
[P-send]: https://docs.szamlazz.hu/php/nyugta-kuldes
[P-pdf]: https://docs.szamlazz.hu/php/nyugta-pdf
[P-changelog]: https://docs.szamlazz.hu/php/changelog
[P-zip]: https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip
[X-create]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd
[X-storno]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd
[X-query]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd
[X-send]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd
[X-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[X-send-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd
