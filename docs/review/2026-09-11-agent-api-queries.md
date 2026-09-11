# Számla Agent XML/PDF query compliance audit — 2026-09-11

**Reviewed HEAD:** `2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81`.

**Scope:** `ops/query_xml.rs`, `ops/query_pdf.rs`, and shared `xml.rs` / `number.rs` response machinery; the envelope, public types and raw-header code necessary to follow those paths. Current API surface, including unchanged code, rather than a changes-only review.

## Result

**No new operational implementation defect confirmed. Complete declared query-field coverage.** The freshly downloaded `szamla.xsd` has **125 child-element declarations in 19 complex structures**, counting reused structures once and including the anonymous root. Every declaration maps to the public XML result. The PDF result exposes all **six successful payload fields** in its response schema. All twelve operation × selector × credential combinations validate against the fresh downloadable request XSDs.

The earlier mixed-prefix list, normalized reserved-namespace and duplicate-expanded-attribute findings are **fixed at this HEAD**, independently rechecked. Eight complete conforming invoice specimens validate in libxml2 and parse in the crate, including each of the four mixed-prefix repeated-row cases. Targeted repository tests pass.

| Priority | Current conclusion | Classification |
|---|---|---|
| P0–P2 | None established within this scope | No demonstrated supported-response failure, identity/reversal injection, panic or lost declared field |
| P3 / vendor clarification | **QV-1:** HU PDF request inline XSD is malformed and contradicts EN/download selector order and requiredness | Confirmed vendor-source conflict; current Rust writer agrees with EN/download |
| Informational | **QV-2:** official success examples contain placeholder/abbreviated PDFs and sparse content inconsistent with their schemas | Documentation limitations, not parser bugs |
| Explicit boundaries | Finite exact Decimal; finite civil dates; sparse content; URL trimming; body/header precedence; PDF bytes and numbered-56 behavior | Detailed below; not silently promoted into defects |

This is not a universal XML/XSD conformance certification or a live-service acceptance claim. Severity ranks a demonstrated implementation impact, not merely a difference from the broadest possible XSD value space.

## Baseline and evidence discipline

- HEAD was checked repeatedly. `git diff HEAD -- crates/szamlazz-agent Cargo.toml Cargo.lock` was empty. Unrelated dirty files and older untracked audit reports existed; the reviewed implementation remained at the pinned revision.
- Source/models and fresh vendor material were examined before the older query reports were read for closure assessment. Older conclusions are not evidence that a defect remains.
- Read-only source/test audit: no source, repository test, fixture or dependency edits; no live account calls or credentials. This report is the repository artifact from this review. No further delegation.
- Scratch acquisition, generators and public-API reproductions were authored with `apply_patch` under `/tmp/opencode/query-2ba5fb86-audit/`. Python performs unauthenticated documentation GETs; Rust calls `AgentRequest::write_xml` / `parse` only. The scratch dependency versions match the workspace's direct parser dependencies: quick-xml 0.42.0, xmlparser 0.13.6, jiff 0.2.35, rust_decimal 1.43.0 and base64 0.23.1.
- Code locations below are relative to `crates/szamlazz-agent/src/` unless prefixed otherwise. They refer to this HEAD. Schema line locations refer to the newly downloaded `szamla.xsd`, independently SHA-256-identical to `fixtures/upstream/agent/xsd/szamla.xsd`.
- Historical account observations come from `docs/szamlazz-hu-behaviour.md`; raw probe logs are not in the repository. Their limited account/date evidence is kept distinct from documentation and synthetic execution.

## Fresh primary-source register

All six query request/XML/response pages were freshly fetched **in both EN and HU**, including tabbed code blocks; all four relevant downloadable XSDs were fetched independently. The pages displayed build `v202608271632`, not a date for every statement.

| Primary sources | Controlling statements / use |
|---|---|
| XML request [EN][xr], [HU][xhr] | Internal outgoing documents issued in Számlázz.hu only: “csak belső (Számlázz.hu-ban kiállított) kimenő számlák”. POST multipart field `action-szamla_agent_xml`; number, order or external id. |
| XML XML/XSD [EN][xx], [HU][xhx], [download][xd] | Fixed order; root-level credentials; optional `szamlaszam`, `rendelesSzam`, `pdf`, `szamlaKulsoAzon`. |
| XML response [EN][xs], [HU][xhs] | Success is a full `szamla`; error is `xmlszamlavalasz` with false verdict/code/message; unknown number/order/external id is **7**. |
| PDF request [EN][pr], [HU][phr] | POST multipart field `action-szamla_agent_pdf`; three alternative selectors; order lookup returns the last document; external id must have been supplied at creation. |
| PDF XML/XSD [EN][px], [HU][phx], [download][pd] | Root-level credentials and response version; QV-1 conflict. |
| PDF response [EN][ps], [HU][phs], [envelope XSD][sd] | Version 2 is structured `xmlszamlavalasz` with base64 PDF; 1/omitted is raw PDF or text error; additional HTTP headers may arrive. Nine schema elements: three verdict/error and six payload. |
| [Full `szamla.xsd`][sx] | Complete field inventory, namespace `http://www.szamlazz.hu/szamla`, qualified elements, sequences, numeric/date types. No includes/imports. |
| Shared outgoing annotations [EN][ae], [HU][ah] | Same schema, field meaning and appearance mapping. Used for vocabulary, not to extend the Agent's internal-outgoing retrieval boundary. |
| [Invoice-create XML][ix] | Followed the XML-response page's schema navigation. This page describes the create request `xmlszamla`, not the returned `szamla`; create-only fields are not presumed query fields. |
| [Invoice-create response][ir] | Supplementary shared HTTP-header vocabulary; does not prove every create header is emitted by PDF queries. |
| [XML 1.0][xmlspec], [Namespaces 1.0][nsspec], [XSD datatypes][datatypes] | Character/token grammar, expanded-name identity and namespace normalization; boolean, double, date and base64 lexical/value boundaries. |

Fresh download fingerprints:

| Download | SHA-256 |
|---|---|
| `szamla.xsd` | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` |
| `xmlszamlaxml.xsd` | `06cd34ce07ca8f3c0919cf7c4e6505bbda66ddf6b72d60736c849e695f7e19f3` |
| `xmlszamlapdf.xsd` | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` |
| `xmlszamlavalasz.xsd` | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |

`fetch.py` saves original response bytes and HTML-decoded `<pre>` text separately, with hashes in `manifest.txt`. `compare.py` independently confirms matching ordered declarations, types, min/max occurrences, numeric facets and enumerations between the download and both shared outgoing inline schemas; between XML request download and EN/HU inline; between PDF request download and EN inline; and between envelope download and EN/HU PDF response inline.

Fresh example-code-block hashes (not substituted inputs): XML success EN `a4150b417f44dab6c96e7e308153755c2aa512ed6314a3b2f6f301a6588465e7`, HU `2597194638b48b81797d2a22b5cdfb6d08efb5dd662dc5e2dabe60e5eddca73f`; PDF success EN/HU `7b09e17855708a5d91a824dbb686bf719232dca9c902c07ae06bc2bf107f548a`. HTML hashes can change between acquisitions without changing extracted XML. The initial category-like `/agent/querying_xml` URL returned 403 and a guessed `/xsds/szamla.xsd` returned 404; the actual request/response pages and nested schema URL above were successfully obtained.

## Confirmed vendor findings and ambiguities

### QV-1 — HU PDF request XSD disagrees with EN/download

**P3 vendor clarification, high confidence; no confirmed Rust defect.**

- [HU inline][phx] is not even well-formed XML: `...xmlszamlapdf"xmlns:tns=...` lacks an attribute separator. Parsing the original extracted block reports **line 1, column 140**.
- Its readable declarations make `szamlaszam` required and order the tail as `szamlaszam → valaszVerzio → rendelesSzam → szamlaKulsoAzon`.
- [EN inline][px] and [download][pd] make `szamlaszam` optional and order `szamlaszam → rendelesSzam → valaszVerzio → szamlaKulsoAzon`.
- HU request prose itself says number, order **or** external id (“vagy”). The crate matches that alternative-selector contract and EN/download order (`ops/query_pdf.rs:62–80`).

Concrete generated order request:

```xml
<xmlszamlapdf xmlns="http://www.szamlazz.hu/xmlszamlapdf">
  <szamlaagentkulcs>dummy</szamlaagentkulcs>
  <rendelesSzam>O</rendelesSzam><valaszVerzio>2</valaszVerzio>
</xmlszamlapdf>
```

It validates against the download. It cannot satisfy the HU inline declarations even after repairing the missing attribute separator. External-id-only queries similarly conflict with HU's required invoice number. Current writer order should not be called wrong on that evidence. Seek vendor alignment of HU XSD/prose; no deployed alternative-order experiment was authorized. Recorded external-id PDF success (`docs/szamlazz-hu-behaviour.md:63–68`) supports that capability, not every order-sequence alternative.

### QV-2 — Published examples are illustrative, not executable conforming replies

**Informational, high confidence.** Both freshly acquired XML-query success examples contain `<pdf>The receipt .pdf can be found here in BASE64 encoding</pdf>`; the PDF-query success contains `....` inside base64. The public parsers correctly return `ParseError::Base64` for these originals. Replacing only the PDF element text with synthetic `JVBERi0=` makes the EN XML example parse as `D-LOLO-66`, gross `557`, and PDF example as `XXX-2012-3`, gross `38100`. This is an explicitly modified control, not recovery of a real PDF.

The XML example also omits XSD-required values including `gazdEsemAzon`, `keszpenz`, `katafokonyv`, buyer `lokacio` / `privatePersonIndicator`, and item `sztetordering`; its `fizmodunified=other` is outside the schema's Hungarian enumeration. Its line totals (`380/76/456`) differ from its grand totals (`464/93/557`). The model appropriately preserves readable supplied content without enforcing arithmetic or all XSD-required presence. The HU XML request example has an empty boolean `<pdf/>` while its prose says use true/false; the crate emits a valid explicit boolean.

Further ambiguities are bounded rather than invented into missing fields:

- “Last” under an order is documented without its exact ordering criterion; the account probes saw latest issued document of any kind (`docs/szamlazz-hu-behaviour.md:45,208–209`). External id is not unique and returns its newest holder in those probes (`:63–70`). A query parser does not establish ownership or adopt a document for a caller.
- `forras` describes imported records in a shared schema, whereas the Agent request explicitly restricts retrieval to internally issued outgoing documents. `QueryInvoiceXml` rustdoc already states that boundary (`:51–53,239–241`).
- `afalevon` is an integer with no published unit/range; `FinancialItem.deductible_vat` deliberately retains it without treating it as a percentage (`:491–494`).
- `cimke` has `maxOccurs=1` in this download despite a labels container; the crate's vector tolerates more. No dropped label results.
- Shared annotations label `eszamla` as a string and `sztetordering` as a double in comments, but the XSD types both as int. Code uses i64-backed values; the appearance meanings are corroborated by HU annotations and the recorded P73 probes.

## Request and response metadata coverage

### Requests

| Wire surface | Public API / implementation |
|---|---|
| POST `https://www.szamlazz.hu/szamla/`, multipart file action | `AgentRequest::to_wire`, `wire.rs:395–408`; `QueryInvoiceXml::ACTION` `:535–537`, `QueryInvoicePdf::ACTION` `:58–60` |
| Root and namespace | `xmlszamlaxml` / `http://www.szamlazz.hu/xmlszamlaxml`, XML `:539–558`; `xmlszamlapdf` / corresponding URI, PDF `:62–81` |
| `felhasznalo`, `jelszo`, `szamlaagentkulcs` | `Credentials` key or username/password, directly under root in schema order; `xml.rs:603–613` |
| `szamlaszam` | `InvoiceSelector::InvoiceNumber(InvoiceNumber)`; exact caller text, XML escaped |
| `rendelesSzam` | `InvoiceSelector::OrderNumber(String)`; exact caller text, no trim/case-fold |
| `szamlaKulsoAzon` | `InvoiceSelector::ExternalId(String)`; emitted last |
| XML query `pdf` | `include_pdf: bool`, default false, always emits true/false before external id; XML `:57–73,552–555` |
| PDF query `valaszVerzio` | Shared `ops::RESPONSE_VERSION` = 2, PDF `:75`; no response-version-1 mode |

`InvoiceSelector` (`types.rs:1034–1053`) permits exactly one selector **field**, although its strings may still be empty. Schemas do not specify a nonblank facet; query-specific validation leaves a usable value to the service. `to_wire` checks XML 1.0 characters, unlike raw `write_xml` (`wire.rs:402–419`). Omitting `xsi:schemaLocation` and unused credential/selector placeholders loses no business input. All twelve generated combinations used XML-special characters and validated with libxml2 against the new downloads.

### Query replies

| Wire field / branch | Public mapping / behavior | Source location |
|---|---|---|
| XML success `szamla` | `InvoiceDocument`; inventory below | `ops/query_xml.rs:563–609` |
| XML failure `xmlszamlavalasz` | Verdict/code/message → `ResponseError::Api`; body-only code 7 works | XML `:580–586`; `xml.rs:448–498` |
| XML generic successful envelope | Refused as unexpected operation body, even when numbered | XML `:583–586` |
| PDF `sikeres` | Unique scalar required; true/false/1/0 supported; empty is false, never invented success | `xml.rs:453–475,743–775` |
| PDF `hibakod`, `hibauzenet` | Optional code/diagnostic; typed known error or retained unknown; absent code stays absent | `xml.rs:458–490`; `ops/envelope.rs:179–219` |
| PDF `szamlaszam` | `InvoicePdf.invoice_number`; body then decoded header; usable number required | `ops/query_pdf.rs:40–41,87`; envelope `:120–133,278–282` |
| PDF `szamlanetto`, `szamlabrutto` | `net_total`, `gross_total: Option<Decimal>` | PDF `:42–45,88–89`; envelope `:229–240` |
| PDF `kintlevoseg` | `outstanding: Option<Decimal>`; missing is not zero | PDF `:46–49,90`; envelope `:241–246` |
| PDF `vevoifiokurl` | `customer_account_url: Option<String>`; body before decoded header | PDF `:50–53,91`; envelope `:135–143` |
| PDF `pdf` | Required decoded `Pdf`; absent/blank → `Missing("pdf")` | PDF `:54–55,92`; envelope `:104–118,145–148` |
| XML `pdf` | Optional `Pdf`; blank/absent → None, even if requested; any nonblank value decoded, even if unsolicited | XML `:604–607` |

Header policy on both paths: nonblank `szlahu_down`, error code header, known non-2xx status, then body (`wire.rs:273–311`). The PDF path uses `parse_issued` and inherits its numbered-56 exception. Body-only code 7 at HTTP 200 is an API answer; HTTP 500 without a code header is `HttpStatus` before body interpretation. These are documented policies (`README.md:310–312`), not assumptions that every status comes from szamlazz.hu.

PDF net/gross/outstanding use XML before `szlahu_nettovegosszeg`, `szlahu_bruttovegosszeg`, `szlahu_kintlevoseg` (`envelope.rs:158–168,229–246`). Empty body values allow header fallback; malformed nonblank XML does not. Headers accept ungrouped dot/comma decimals and exponents with HTTP SP/HTAB padding (`:354–373`); XML does not accept decimal commas. Textual header decoding is once-only (`wire.rs:343–350`): `+` → space, `%2B` → plus. XML URL content is entity-decoded, not percent-decoded.

`CreatedInvoice` also reads `szlahu_id`, payment method and notification status, but `InvoicePdf` does not project those auxiliary fields (`envelope.rs:226–250`; PDF `:86–93`). They are absent from the PDF-specific body schema; its generic “additional headers” sentence does not promise these typed PDF fields. Raw callers retain the headers in `RawResponse`. XML `<szamla>` has no external-id, outstanding or customer-account URL element, and the XML result does not merge such success headers into its body model.

## Complete `szamla` field and nested-type inventory

**R/O** below describe XSD required/optional presence, not a demand that the permissive parser enforce it. `?` means `Option<T>`. Grouped mappings are in corresponding order; every member is enumerated. All amounts are exact `Decimal`, all integer code/id fields i64, all dates printed civil `Date`. Optional business strings use `business_text`: absent/empty/XML-whitespace-only → None; otherwise decoded characters preserved, including padding and NBSP (`xml.rs:720–732`).

### Root, reusable structures and supplier — 28 declarations

Schema `:78–116,309–321`; public `ops/query_xml.rs:76–147,324–339`; wire/conversions `:618–703,802–826`.

| Path / type | XSD | Public model |
|---|---|---|
| Root `szallito`, `alap`, `vevo` | R complex | `InvoiceDocument.supplier: Supplier`, `info: InvoiceInfo`, `buyer: BuyerInfo` |
| Root `tetelek`, `osszegek` | R complex | `items: Vec<DocumentItem>` (required wrapper), `totals: Totals` |
| Root `qutetek`, `cimkek`, `kifizetesek` | O complex | `financial_items: Vec<FinancialItem>`, `labels: Vec<String>`, `credit_entries: Vec<RecordedCreditEntry>`; absent → empty |
| Root `pdf` | O string | `pdf: Pdf?` via base64 operation contract |
| `cimTipus`: `orszag`; `irsz`, `telepules`, `cim` | O string; R strings | `Address.country: String?`; `zip`, `city`, `address: String` |
| `cimpostaTipus`: `nev`, `orszag`, `irsz`, `telepules`, `cim` | O strings | `BuyerPostalAddress.name`, `country`, `zip`, `city`, `address: String?` |
| `bankTipus`: `nev`, `bankszamla` | O strings | `Bank.name`, `account: String?` |
| `szallito/id`, `nev` | R int/string | `Supplier.id: i64?`, `name: String` |
| `szallito/cim`, `postacim` | R/O `cimTipus` | `address: Address`, `postal_address: Address?` |
| `szallito/adoszam`, `csoportazonosito`, `adoszameu` | R/O/O strings | `tax_number`, `group_id`, `eu_tax_number: String?` |
| `szallito/bank` | O `bankTipus` | `bank: Bank?` |

Supplier postal address intentionally uses `Address`, not buyer postal structure: the schema does not declare a recipient name there. Required string leaves may be empty; missing required structural/name leaves fail. Supplier id/tax number are intentionally more permissive than the XSD.

### `alapTipus` — 28 declarations

Schema `:120–152`; public `ops/query_xml.rs:225–322`, appearance `:149–223`; wire/conversion `:705–800`; reference helper `:1089–1096`.

| `alap/…` | XSD | `InvoiceInfo` mapping |
|---|---|---|
| `id`, `szamlaszam` | R int/string | `id: i64`, `invoice_number: InvoiceNumber` |
| `gazdEsemAzon`, `forras` | R/O int | `economic_event_id`, `source: i64?` |
| `iktatoszam` | O string | `registration_number: String?` |
| `tipus` | R string | `document_type: DocumentType` (open token) |
| `eszamla` | R int | `appearance: InvoiceAppearance` (0 not-invoice, 1 paper, 2/3 electronic, otherwise retained unknown) |
| `hivszamlaszam`, `hivdijbekszam` | O strings | `referenced_invoice_number`, `referenced_proforma_number: InvoiceNumber?`; nonblank characters retained |
| `kelt`, `telj`, `fizh` | R dates | `issue_date`, `fulfillment_date`, `due_date: Date?` |
| `fizmod` | R string | `payment_method: PaymentMethod?` (open token) |
| `fizmodunified` | R restricted string | `unified_payment_method: String?` (open) |
| `keszpenz` | R boolean | `cash_payment: bool`; missing/empty → false |
| `rendelesszam` | O string | `order_number: String?`; lowercase response spelling differs from request `rendelesSzam` |
| `nyelv`, `devizanem` | R restricted string/string | `language: String?`, `currency: Currency?` |
| `devizabank`, `devizaarf` | O string/double | `exchange_bank: String?`, `exchange_rate: Decimal?` |
| `megjegyzes`, `afatipus` | O strings | `comment`, `vat_type: String?` |
| `penzforg`, `kata`, `katafokonyv` | R booleans | `cash_accounting`, `kata`, `kata_ledger: bool`; missing/empty → false |
| `email` | O string | `email: String?`, document-associated address |
| `teszt`, `sztornozott` | R/O boolean | `test`, `reversed: bool?`; no false/live value invented for absent test marker |

### Buyer and its ledger — 18 declarations

Schema `:155–180`; public `ops/query_xml.rs:341–398`; wire/conversion `:828–913`.

| Path | XSD | Public mapping |
|---|---|---|
| `vevo/id`, `nev`, `azonosito` | O int, R string, O string | `BuyerInfo.id: i64?`, `name: String`, `identifier: String?` |
| `vevo/cim`, `postacim` | R `cimTipus`, O `cimpostaTipus` | `address: Address?`, `postal_address: BuyerPostalAddress?` |
| `vevo/email`, `adoszam`, `csoportazonosito`, `adoszameu` | O/R/O/O strings | `email`, `tax_number`, `group_id`, `eu_tax_number: String?` |
| `vevo/lokacio`, `privatePersonIndicator` | R int/boolean | `location: i64?`, `private_person: bool` (missing/empty false) |
| `vevo/fokonyv` | O complex | `ledger: BuyerLedgerInfo?` |
| `fokonyv/vevo`, `vevoazon` | O strings | `BuyerLedgerInfo.account`, `buyer_id: String?` |
| `fokonyv/datum` | O date | `date: Date?` |
| `fokonyv/folyamatostelj` | O boolean | `continuous_fulfillment: bool?` |
| `fokonyv/elszDatTol`, `elszDatIg` | O dates | `settlement_from`, `settlement_to: Date?` |

### Printed items and ledger — 21 declarations

Schema `:183–221`; public `ops/query_xml.rs:400–463`; wire/conversion `:915–998`.

| Path | XSD | Public mapping |
|---|---|---|
| `tetelek/tetel` | R unbounded complex | `items: Vec<DocumentItem>`; empty wrapper tolerated |
| `tetel/nev`, `azonosito` | R/O strings | `DocumentItem.name: String`, `id: String?` |
| `mennyiseg`, `mennyisegiegyseg`, `nettoegysegar` | R double/string/double | `quantity: Decimal`, `unit: String`, `unit_price: Decimal` |
| `afatipus`, `afakulcs` | O restricted string, R nonnegative double | `vat_type: String?`, `vat_rate_code: String` (raw token) |
| `netto`, `arresafaalap`, `afa`, `brutto` | R/O/R/R double | `net_value: Decimal`, `margin_vat_base: Decimal?`, `vat_value`, `gross_value: Decimal` |
| `megjegyzes`, `sztetordering` | O string, R int | `comment: String?`, `ordering: i64?` |
| `fokonyv` | O complex | `ledger: DocumentItemLedger?` |
| `fokonyv/arbevetel`, `afa`, `gazdasagiesemeny`, `gazdasagiesemenyafa` | O strings | `DocumentItemLedger.revenue_account`, `vat_account`, `economic_event`, `vat_economic_event: String?` |
| `fokonyv/elszdattol`, `elszdatig` | O dates | `settlement_from`, `settlement_to: Date?`; lowercase wire names are correct here |

### Totals — 10 declarations

Schema `:224–253`; public `types.rs:1055–1107`; shared wire/conversion `xml.rs:799–882`.

| Path | XSD | Public mapping |
|---|---|---|
| `osszegek/afakulcsossz` | R unbounded complex | `Totals.by_vat_rate: Vec<VatTotal>`; absent → empty |
| `osszegek/totalossz` | R complex | `Totals.total: GrandTotal`, required |
| `afakulcsossz/afatipus`, `afakulcs` | O restricted string, R nonnegative double | `VatTotal.vat_type: String?`, `vat_rate_code: String` |
| `afakulcsossz/netto`, `afa`, `brutto` | R doubles | `VatTotal.net`, `vat`, `gross: Decimal` |
| `totalossz/netto`, `afa`, `brutto` | R doubles | `GrandTotal.net`, `vat`, `gross: Decimal` |

### Credit entries — 8 declarations

Schema `:256–271`; public `ops/query_xml.rs:507–533`; wire/conversion `:1052–1087`.

| Path | XSD | Public mapping |
|---|---|---|
| `kifizetesek/kifizetes` | R unbounded complex | `credit_entries: Vec<RecordedCreditEntry>`; empty wrapper tolerated |
| `datum`, `jogcim`, `osszeg` | R date/string/double | `RecordedCreditEntry.date: Date`, `title: PaymentMethod`, `amount: Decimal` |
| `megjegyzes`, `bankszamlaszam` | O strings | `comment`, `bank_account: String?` |
| `banktranzid`, `devizaarf` | O int/double | `bank_transaction_id: i64?`, `exchange_rate: Decimal?` |

The bank account meaning matches the HU annotation: actual sender's account, otherwise the account printed on the invoice; the field does not distinguish which. Required credit-entry date is stricter than optional invoice/ledger dates and rejects absent/empty/invalid text.

### Financial items and labels — 12 declarations

Schema `:274–306`; public `ops/query_xml.rs:465–505`; wire/conversion `:1000–1050`.

| Path | XSD | Public mapping |
|---|---|---|
| `qutetek/qutet` | O unbounded complex | `financial_items: Vec<FinancialItem>` |
| `qutet/nev`, `afatipus`, `afakulcs` | R string, O restricted string, R nonnegative double | `FinancialItem.name: String`, `vat_type: String?`, `vat_rate_code: String` |
| `qutet/netto`, `afa`, `brutto` | R doubles | `net`, `vat`, `gross: Decimal` |
| `qutet/elszdattol`, `elszdatig` | O dates | `settlement_from`, `settlement_to: Date?` |
| `qutet/afalevon` | R int | `deductible_vat: i64` |
| `qutet/cimkek` | O complex | `labels: Vec<String>` |
| `cimkek/cimke` (invoice and financial item) | O string, max 1 | `Vec<String>` retaining each decoded label |

The public VAT helpers choose nonempty `afatipus` over `afakulcs`; otherwise interpret an exactly representable numeric token as `VatRate::Percent` (`ops/query_xml.rs:456–463,499–505`, `types.rs:1087–1093`). Raw tokens remain available. Language/unified-payment/VAT/source sets are not closed validation gates. `JS`, obsolete/unknown VAT tokens and newer codes remain readable through open strings/enums. Waybill/carrier settings, request templates, seller email configuration and external id are not in this returned schema; their absence from `InvoiceDocument` is not missing query coverage.

## Shared parser audit and deliberate boundaries

### XML structure and namespaces

`response_root` validates UTF-8, one completed expected root, matching closes and legal prolog/epilog through EOF (`xml.rs:176–250`). It rejects DTDs rather than resolving external entities. `validate_lexical` applies the tokenizer to the original text and unescapes text/attribute values even in ignored subtrees, rejecting forbidden XML characters and undefined references (`:252–276`). Declaration and PI grammar checks are at `:371–434`.

`NamespaceReader` normalizes namespace declarations **before** installing/checking bindings and verifies unique expanded attribute names (`:19–130`). Scope pop occurs after an empty/end event is resolved. `protocol_text` filters whole foreign subtrees, canonicalizes protocol element QNames and keeps an unknown placeholder rather than joining scalar fragments around a removed foreign child (`:278–353`). Serde owns recognized parent paths and duplicate-singleton/scalar checks; overlapped lists permit ignored children between rows. This combination closes the older prefix/list issues without interpreting extension descendants as identity or reversal facts.

Confirmed positive/negative controls include:

- Complete XSD-valid invoice; mixed-prefix `tetel`, `qutet`, `kifizetes`, `afakulcsossz`; escaped reserved `xml` declaration; timezone dates; exponent money: validate and parse, with correct row counts.
- Escaped ordinary namespace URI, XML-special text via references/CDATA, legal attribute quoting, non-ASCII/supplementary names: accepted.
- Foreign or unknown-wrapper reversal does not supply `info.reversed`; foreign identity replacing the required identity is refused. `tr<foreign/>ue` and analogous nested identity/numeric values are refused, not concatenated.
- Duplicate identities/reversal/verdicts, reserved namespace rebinding/defaults, prefixed undeclaration, undeclared prefixes and duplicate expanded attributes: refused in tested cases.
- NUL/illegal references, `]]>` ordinary text, undefined entities in ignored content, invalid name/attribute syntax, truncation, malformed tails and second roots: refused.

Limits: this audit does not exhaust every XML 1.0 or Namespaces grammar production. For example, `valid_pi_target` permits colon-bearing PI targets and tests explicitly accept them; Namespaces 1.0 §7 says PI targets have no colons. That is a minor accepted-input conformance boundary without demonstrated business-field impact, not a substantive injection finding. XML version 1.1, DTD documents and non-UTF-8 decoding are outside the current boundary. No namespace URI dereferencing occurs.

### Numbers

`number.rs:63–141` parses sign, digits, optional decimal point and signed exponent into exact Decimal coefficient/scale without f64 or silent rounding. Trailing powers of ten are canceled before the representability check; exponent expansion is bounded to at most 29 significant integer digits. Zero remains zero even for an enormous syntactically valid exponent. `xml.rs:622–640` applies this to required/optional XML amounts; `envelope.rs:347–373` applies it to metadata and the distinct HTTP grammar.

Fresh controls: Decimal maximum `79228162514264337593543950335` and equivalent `792281625142643375935439503350e-1` both succeed; `+.1E+1` succeeds; `1e-29`, `1e100`, `INF`, `NaN`, underscores and XML decimal commas fail. Existing fidelity tests additionally cover zero/excess trailing zeroes, underflow, high precision, stored scale and VAT interpretation across channels.

This is an **explicit finite exact-decimal boundary** (`README.md:394–401`), not complete `xs:double` value-space support. `xs:double` permits infinities/NaN, and finite binary64 values outside Decimal's magnitude/scale. Choosing to refuse those cannot be counted as an accidental monetary corruption. Integers are deliberately i64 rather than the schema's narrower int; malformed/out-of-range i64 text fails. VAT rate raw strings are intentionally more permissive than the XSD numeric facet. No amount arithmetic consistency checks are performed on queried totals. The private arithmetic helpers at `number.rs:6–59` are not executed by either query parser.

### Dates, booleans and text

`xml.rs:642–702` first retains the existing Jiff finite civil-date parsing domain, then accepts complete `Z` / `±hh:mm` suffixes with maximum `±14:00`, preserving the printed date without UTC conversion. Character-boundary-checked splitting prevents the prior multibyte slicing panic class. Invalid dates, offset `+14:01`, suffix junk and `é12345` are errors; optional absent/empty dates remain None. Unit tests exercise **all eleven date positions** (`ops/query_xml.rs:1333–1469`).

Finite civil dates do not promise the entire XSD year space or strict lexical identity: accepted legacy spellings include basic `20240229`, year zero and signed expanded negative years, as recorded in the tests and README `:221–225`. Out-of-domain dates are not operational bugs merely because a broad XSD permits them. The separate Adatkapcsolat policy of retaining invalid-date content as None is not the Számla Agent policy: at this HEAD an invalid nonblank optional Agent date fails, per its README.

Booleans accept true/false/1/0. Optional test/reversal/continuous-fulfillment values preserve absence; ordinary independent content flags default missing/empty to false (`xml.rs:734–776`). Business text uses XML whitespace only for emptiness; numbers/optional dates/booleans use some broader Unicode trimming. These are permissive lexical boundaries, not evidence that values silently switch meaning across conforming inputs.

`vevoifiokurl` still Unicode-trims XML boundary whitespace through `empty_as_none` (`ops/envelope.rs:114–115`; `xml.rs:706–718`). Fresh `<vevoifiokurl>&#160;opaque:x&#160;</vevoifiokurl>` returns `Some("opaque:x")`. The README explicitly excludes URLs from business-text preservation (`:244–251`); no broken real link is established. Record this fidelity limitation instead of recycling the older URL finding as a current operational defect. Body/header disagreement deliberately selects body, including a mismatched invoice-number header. Query success does not attest that the returned selector identity equals the caller's intended one.

### PDF and verdict exceptions

`Pdf::from_base64` removes whitespace and uses standard base64 decoding (`types.rs:104–117`); it is a byte wrapper, not a PDF structure/magic/signature validator. The synthetic `%PDF-` controls are five bytes, not complete PDFs. A malformed nonblank XML-query PDF fails the whole result; a requested but missing/blank PDF returns None on XML query and `Missing("pdf")` on dedicated PDF query. These follow current operation result types rather than unrecorded vendor guarantees.

PDF query inherits numbered-56 leniency (`ops/envelope.rs:179–251`): a failure-56 body with a usable number and valid PDF returns `InvoicePdf`, without projecting the notification flag. Fresh synthetic execution confirms this. The PDF-specific docs do not establish that fetching a PDF can produce a notification event; no live evidence was found. A real non-56 body refusal takes precedence over optional payload decoding, and malformed/duplicate verdict or code cannot establish success. Unusable optional diagnostics may be discarded while the refusal remains (`xml.rs:453–474`), an intentional evidence-preservation rule. Treat 56-on-query as an unverified shared policy rather than claim it is observed PDF behavior.

### Resource and operational limits

Both paths buffer the body; shared checking makes multiple passes and `protocol_text` builds a projected XML allocation, then decoded strings/vectors/PDF allocate again. Exponent parsing does not expand hostile exponents without bound. No new measured resource exhaustion, panic, malformed-input complexity regression or response-size threshold was established. Deep nesting/large collections, streaming, transport deadlines and account authentication are not certified by these offline parser checks. The lead audit owns full-crate/client verification.

## Recorded exceptions respected

From `docs/szamlazz-hu-behaviour.md`, retained as bounded historical evidence:

- Absent `sztornozott` before reversal, true afterward on the original, never on the storno; reversal removes original credit entries (`:77–80`). The model preserves optional reversal rather than requiring false.
- `eszamla=1` is paper; creates requesting electronic were read back as 3; 2 is documented but unobserved (`:97–98,264–267`). Current open appearance mapping is correct.
- `<vevo>` can change after a later create while invoice `<alap><email>` is separate (`:111`); rustdoc warns that the buyer is not an immutable at-issue snapshot (`ops/query_xml.rs:360–365`).
- Credit entries may arrive in a different order; query and credit errors may be body-only (`:133–142`). The parser retains list order without assigning submission order semantics and checks body errors.
- External ids are not echoed; shared-id queries return the newest holder; consumed/deleted proformas can both be absent (`:63–71`). No fabricated external-id field or uniqueness guarantee.
- Comma-valued monetary headers and numeric VAT rates such as 27.0 occur (`:160–162`). Header grammar and VAT numeric interpretation cover them.

## Verification record, closure and reproductions

Commands run successfully against the pinned crate:

```text
cargo test -p szamlazz-agent --test numeric_fidelity --test response_namespaces --test response_completion --test business_text --test response_headers
cargo test -p szamlazz-agent --lib ops::query
cargo test -p szamlazz-agent --lib xml::tests
cargo test -p szamlazz-agent --test upstream
```

Results: 35 integration tests; 29 query unit tests; 27 matches in the `xml::tests` filter (22 overlap the query tests and 5 shared XML tests); 11 upstream/outline tests. **80 distinct tests passed**, no failures. The upstream corpus was available and exercised; its PDF repairs are explicitly labeled in `tests/upstream.rs`, not fresh live examples. Full crate tests remain the lead's responsibility.

Scratch commands and artifacts:

```text
python3 /tmp/opencode/query-2ba5fb86-audit/fetch.py
python3 /tmp/opencode/query-2ba5fb86-audit/xsd_check.py
cargo run --manifest-path /tmp/opencode/query-2ba5fb86-audit/Cargo.toml --offline
python3 /tmp/opencode/query-2ba5fb86-audit/xsd_check.py
python3 /tmp/opencode/query-2ba5fb86-audit/compare.py
```

The first XSD command generates eight conforming specimens (the complete invoice and seven variants); the Rust program generates twelve requests, parses the fresh examples with original and substituted PDFs, and tests 28 independent XML/date/numeric/namespace mutations plus envelope and numeric-boundary cases; the second XSD command validates those emitted requests. Python's standard library has no XSD engine, so `xsd_check.py` calls installed **libxml2** with explicit ctypes signatures. No package installation was needed. An initial `python` invocation failed because only `python3` is installed; all reported checks use the latter.

| Earlier finding | Independent current disposition |
|---|---|
| f83e5fd FQ-1 mixed-QName repeated rows / interleaved extensions | **Closed.** Fresh schema-generated mixed-prefix cases validate and parse; `tests/response_namespaces.rs:36–103,287–299` passes interleaved extension/list and singleton controls. Canonicalization at `xml.rs:310–334`. |
| f83e5fd FQ-2 escaped reserved `xml` URI | **Closed.** `reserved-xml.xml` validates/parses; current normalized resolver at `xml.rs:73–102`. |
| f83e5fd FQ-3 duplicate expanded attributes / forbidden bindings | **Closed for the reported cases.** Direct scratch refusals plus `tests/response_namespaces.rs:236–284`; checks at `xml.rs:90–125`. |
| Earlier CQ-2 illegal characters/token grammar | **Closed for the reported cases.** `tests/response_completion.rs:102–145` passes; lexical checker at `xml.rs:252–276`. |
| Earlier missing PDF outstanding/customer URL and retrieval-scope docs | **Closed.** Public fields/mapping and explicit boundary exist at cited locations. |
| Earlier URL-trimming observation | Still reproducible, but explicitly outside business-text fidelity policy; no newly demonstrated operational defect. |

The new full invoice is mechanically generated from every current schema child declaration, not an old review fixture. For an easily reconstructed negative specimen, replace `</alap>` in a valid invoice with `<sztornozott>true</sztornozott><sztornozott>false</sztornozott></alap>`: current public parse refuses the duplicate. For the old valid-prefix case, duplicate one complete `tetel` and change only the second row's QName to `p:tetel` with `xmlns:p="http://www.szamlazz.hu/szamla"`: current parse returns both rows. Neither test requires vendor access.

## Remaining evidence limits

No account calls were made: vendor emission frequency, exact “last” selection criterion, conflicting HU PDF sequence acceptance, full PDF validity, code-56-on-query emission and real uncommon numeric/date values remain unverified. The XSD-generated invoice establishes syntax/model coverage, not accounting consistency or vendor issuance acceptance. Tests do not prove all combinations of arbitrary unknown extensions, extreme nesting or all XML grammar forms. Published examples are neither complete PDFs nor fully XSD-conforming business records. Within those limits, the currently declared query contract is covered and the earlier substantive query parser findings are resolved.

[xr]: https://docs.szamlazz.hu/agent/querying_xml/request
[xhr]: https://docs.szamlazz.hu/hu/agent/querying_xml/request
[xx]: https://docs.szamlazz.hu/agent/querying_xml/xml
[xhx]: https://docs.szamlazz.hu/hu/agent/querying_xml/xml
[xs]: https://docs.szamlazz.hu/agent/querying_xml/response
[xhs]: https://docs.szamlazz.hu/hu/agent/querying_xml/response
[xd]: https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd
[pr]: https://docs.szamlazz.hu/agent/querying_pdf/request
[phr]: https://docs.szamlazz.hu/hu/agent/querying_pdf/request
[px]: https://docs.szamlazz.hu/agent/querying_pdf/xml
[phx]: https://docs.szamlazz.hu/hu/agent/querying_pdf/xml
[ps]: https://docs.szamlazz.hu/agent/querying_pdf/response
[phs]: https://docs.szamlazz.hu/hu/agent/querying_pdf/response
[pd]: https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd
[sx]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[sd]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd
[ae]: https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak
[ah]: https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak
[ix]: https://docs.szamlazz.hu/agent/generating_invoice/xml
[ir]: https://docs.szamlazz.hu/agent/generating_invoice/response
[xmlspec]: https://www.w3.org/TR/xml/
[nsspec]: https://www.w3.org/TR/xml-names/
[datatypes]: https://www.w3.org/TR/xmlschema-2/
