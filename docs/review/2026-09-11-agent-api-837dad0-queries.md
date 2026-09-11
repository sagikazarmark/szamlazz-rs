# Számla Agent full-tree query review — 837dad0

**Reviewed HEAD:** `837dad024300e2a202c2b6351fcba73df82a7744`

**Review / official-source fetch date:** 2026-09-11

**Owned scope:** `szamlazz-agent` XML/PDF queries, the complete queried-document model, and shared parsing/response machinery insofar as it affects those queries.

## Result

**No implementation defect was confirmed in the reviewed query surface.** Every child-element declaration in the freshly fetched `szamla.xsd` has a mapping: **125 declarations across 19 complex structures**, counting reusable structures once, including the anonymous root. The PDF result represents all **six successful payload fields** in its published envelope schema. No missing seller, buyer, printed-item, financial-item, ledger, credit-entry or totals field was found.

The published queried-document schema contains **no waybill or carrier block**. These are invoice-creation inputs, not established query outputs. Treating their absence as a query-model defect would confuse `xmlszamla` with `szamla`.

| Classification | Current conclusion |
|---|---|
| Confirmed defects, P0–P3 | None established in this scope |
| Supported behavior | Complete declared query-field mapping; three selectors; correct response roots, body-only errors, PDF metadata and decoded artifacts |
| Accepted policy deviations | Sparse content/defaults, exact finite Decimal, finite civil dates, open tokens, URL/number trimming in the envelope, PDF artifact requirements, body/header precedence |
| Vendor ambiguity | **VQ-1, P3 clarification:** HU PDF request XSD is malformed and contradicts EN/download selector requiredness/order; other source/sample limitations below |
| Optional unsupported functionality | Raw response-version-1 mode, auxiliary header projection, raw XML retention, additional named variants; no missing declared body capability |

**Verification:** 77 selected existing tests passed, zero failures. No source, test, fixture or existing-report changes; no live Számla Agent requests, credentials or delegation. This report is the sole repository artifact authored by this review.

## Method and evidence boundaries

- HEAD matched the requested full SHA at initial and final pre-report checks; the worktree was clean. This is an audit of the complete scoped implementation, including unchanged code, rather than a diff review.
- Final verification after writing found other concurrently authored, untracked `837dad0` review reports. They were left untouched; the scoped source, manifests, fixtures and behavior-note diff remained empty, and HEAD remained pinned.
- Independently read implementation, model conversions, fresh official pages/downloads, `fixtures/SOURCES.md` and `docs/szamlazz-hu-behaviour.md` before opening historical `docs/review/*agent-api*` reports. Historical reports were then used for closure checks, not as current source evidence.
- Started at the requested [XML category][xc] and [PDF category][pc], followed request/response/XML+XSD pages and schema navigation. Fetched all six operation pages in EN and HU, four directly relevant XSD downloads, the shared outgoing-document annotations/schema, and relevant creation/header descriptions. All network activity was unauthenticated documentation GETs.
- Official pages display build `v202608271632`. This identifies the site build, not the date or operational truth of every statement. Fresh tool-returned pages and XSD contents were inspected; no new byte-hash manifest or archived download corpus was created.
- All code locations below are exact HEAD line ranges, relative to `crates/szamlazz-agent/src/` unless stated otherwise. Mapping was traced from wire structs through conversions to public fields, not inferred from English names alone.
- The default-feature offline tests call parsers/writers and workspace corpus checks. No new scratch harness, XSD validation engine, PDF renderer, fuzzing campaign or live acceptance test was run. Request conformance here is a direct declaration/order comparison plus existing serialization tests, **not a claim of freshly executed XSD validation**.
- Account observations are the recorded one-TEST-account probes dated September 3, 6 and 7; their raw logs are outside the repository (`docs/szamlazz-hu-behaviour.md:3–28`). Published documentation examples and synthetic fixtures are not account captures.

## 1. Supported requests and response branches

The [XML request][xr] explicitly says: “only the data of internal outgoing invoices (issued in Számlázz.hu) can be retrieved via this interface.” `QueryInvoiceXml` documents that limit at `ops/query_xml.rs:46–53`; its `source` field explicitly does not expand it (`:239–242`). The shared schema's imported-source codes therefore do not establish incoming or imported-invoice retrieval support.

Both request pages specify identification by invoice number, order number **or** external identifier, and “if multiple documents share the same order number, the last one is returned.” The following mappings cover every request declaration in the downloads [XD][xd] and [PD][pd]:

| Wire surface | Implementation / result |
|---|---|
| HTTPS POST, `multipart/form-data`, XML file | `wire.rs:7–14,66–99,395–408`; file part, not an ordinary text form field |
| XML action/root/namespace | `action-szamla_agent_xml`, `xmlszamlaxml`, `http://www.szamlazz.hu/xmlszamlaxml`; `ops/query_xml.rs:535–557` |
| PDF action/root/namespace | `action-szamla_agent_pdf`, `xmlszamlapdf`, `http://www.szamlazz.hu/xmlszamlapdf`; `ops/query_pdf.rs:58–80` |
| `felhasznalo`, `jelszo`, `szamlaagentkulcs` | Root-level credential fields in that order; username/password or key; `xml.rs:610–620` |
| `szamlaszam` | `InvoiceSelector::InvoiceNumber(InvoiceNumber)` |
| `rendelesSzam` | `InvoiceSelector::OrderNumber(String)`; caller text preserved |
| `szamlaKulsoAzon` | `InvoiceSelector::ExternalId(String)`, emitted last |
| XML `pdf` | `include_pdf: bool`, constructor false, explicit true/false before external id; `ops/query_xml.rs:57–73,545–555` |
| PDF `valaszVerzio` | Explicit shared `ops::RESPONSE_VERSION`, value 2, between number/order selector and external id; `ops/query_pdf.rs:68–78` |

The selector enum (`types.rs:1034–1053`) chooses exactly one **field**; its unrestricted strings can still be blank. Neither download specifies a nonblank string facet. There is no query-side normalization, multi-selector precedence or invented fallback. This preserves the recorded exact/case-sensitive order lookup (`docs/szamlazz-hu-behaviour.md:40–45`). `write_xml` escapes text; `to_wire` additionally checks XML 1.0 characters (`wire.rs:402–419`). Unused empty placeholders and `xsi:schemaLocation` need not be emitted to represent the request.

The [XML response][xs] says success is a “Full `szamla` XML document” and failure an `xmlszamlavalasz` with `<sikeres>false</sikeres>`, `<hibakod>` and `<hibauzenet>`. Unknown number/order/external id yields **7**. Current parsing at `ops/query_xml.rs:563–609`:

1. Checks header/down/status policy.
2. Requires a complete, correctly namespaced `szamla` or `xmlszamlavalasz`.
3. Parses a failure envelope into the typed reported error, including body-only code 7.
4. Refuses a *successful* generic envelope as the wrong XML-query response shape.
5. Projects the full `szamla` tree without recalculating totals or comparing it to the requested selector.

## 2. Complete queried-document field inventory

Structural source for every row: freshly downloaded [`szamla.xsd`][sx], also inspected inline on [the HU outgoing-document page][ah]. It is self-contained, with no schema include/import to expand. That page supplies field annotations for the same XML shape, not a broader Számla Agent retrieval entitlement.

**Notation:** R/O = XSD required/optional presence, not the permissive parser's gate. `?` = `Option<T>`. Grouped fields map in corresponding order. Strings are decoded characters, not retained XML bytes. Optional business text uses `xml.rs:727–739`: absent/empty/XML-space-only becomes None; otherwise padding, NBSP and other decoded characters survive.

### Root and reusable structures — 20 declarations

Public: `ops/query_xml.rs:76–147,324–339`. Wire/conversions: `:618–703,802–826`.

| Declaration / path | XSD | Public mapping |
|---|---|---|
| `szamla/szallito`, `alap`, `vevo` | R complex | `InvoiceDocument.supplier: Supplier`, `info: InvoiceInfo`, `buyer: BuyerInfo` |
| `szamla/tetelek` | R complex | `items: Vec<DocumentItem>`; wrapper required |
| `szamla/qutetek`, `cimkek` | O complex | `financial_items: Vec<FinancialItem>`, `labels: Vec<String>`; absent becomes empty |
| `szamla/osszegek` | R complex | `totals: Totals` |
| `szamla/kifizetesek` | O complex | `credit_entries: Vec<RecordedCreditEntry>`; absent becomes empty |
| `szamla/pdf` | O string | `pdf: Pdf?`; base64 meaning comes from query documentation |
| `cimTipus/orszag` | O string | `Address.country: String?` |
| `cimTipus/irsz`, `telepules`, `cim` | R strings | `Address.zip`, `city`, `address: String` |
| `cimpostaTipus/nev`, `orszag`, `irsz`, `telepules`, `cim` | O strings | `BuyerPostalAddress.name`, `country`, `zip`, `city`, `address: String?` |
| `bankTipus/nev`, `bankszamla` | O strings | `Bank.name`, `account: String?` |

The reusable billing-address structure also serves **seller postal address**. Only buyer postal address has the separate optional recipient-name field. Missing required address leaves fail; present empty strings are allowed. An entirely absent buyer billing block is tolerated, but a present incomplete `cimTipus` is not automatically completed.

### Seller — 8 declarations

Public: `ops/query_xml.rs:125–147`. Wire/conversion: `:672–703`.

| `szallito/…` | XSD | `Supplier` |
|---|---|---|
| `id`, `nev` | R int/string | `id: i64?`, `name: String` |
| `cim`, `postacim` | R/O `cimTipus` | `address: Address`, `postal_address: Address?` |
| `adoszam` | R string | `tax_number: String?` |
| `csoportazonosito`, `adoszameu` | O strings | `group_id`, `eu_tax_number: String?` |
| `bank` | O complex | `bank: Bank?` |

Seller id and tax number deliberately tolerate absence. No numeric conversion is applied to tax numbers, bank accounts or ZIP codes, preserving leading zeros and punctuation. The seller id is merely the reported internal id; no account-identity/stability check is inferred from it. Recorded seller-id observations do not establish stability across edits (`docs/szamlazz-hu-behaviour.md:144,192–197`).

### Core invoice data — 28 declarations

Public: `ops/query_xml.rs:225–322`; appearance `:149–223`. Wire/conversion: `:705–800`; reference-number helper `:1089–1096`.

| `alap/…` | XSD | `InvoiceInfo` |
|---|---|---|
| `id`, `szamlaszam` | R int/string | `id: i64`, `invoice_number: InvoiceNumber` |
| `gazdEsemAzon`, `forras` | R/O int | `economic_event_id`, `source: i64?` |
| `iktatoszam` | O string | `registration_number: String?` |
| `tipus` | R string | `document_type: DocumentType`, open token |
| `eszamla` | R int | `appearance: InvoiceAppearance`, open integer code |
| `hivszamlaszam`, `hivdijbekszam` | O strings | `referenced_invoice_number`, `referenced_proforma_number: InvoiceNumber?` |
| `kelt`, `telj`, `fizh` | R dates | `issue_date`, `fulfillment_date`, `due_date: Date?` |
| `fizmod` | R string | `payment_method: PaymentMethod?` |
| `fizmodunified` | R restricted string | `unified_payment_method: String?` |
| `keszpenz` | R boolean | `cash_payment: bool`, absent/empty false |
| `rendelesszam` | O string | `order_number: String?`; response spelling is deliberately lowercase |
| `nyelv`, `devizanem` | R restricted string/string | `language: String?`, `currency: Currency?` |
| `devizabank`, `devizaarf` | O string/double | `exchange_bank: String?`, `exchange_rate: Decimal?` |
| `megjegyzes`, `afatipus` | O strings | `comment`, `vat_type: String?` |
| `penzforg`, `kata`, `katafokonyv` | R booleans | `cash_accounting`, `kata`, `kata_ledger: bool`, absent/empty false |
| `email` | O string | `email: String?`, document-associated email |
| `teszt`, `sztornozott` | R/O boolean | `test`, `reversed: bool?`; absence preserved |

Semantic adjudication:

- [HU annotation][ah]: “0: nem számla, 1: papír számla, 2: e-számla, 3: e-számla”. The code implements exactly that mapping and retains unknown integers. P73 observed 1 for paper and 3 for electronic; 2 remains documented but unobserved (`docs/szamlazz-hu-behaviour.md:97–98,264–267`). This is not the create request's boolean `eszamla`.
- `iktatoszam` is the receiver's registration number, not szamlazz.hu's id. The annotation says “könyvelési rendszertől kapott adat” (data received from the accounting system).
- `hivszamlaszam` and `hivdijbekszam` retain the referenced numbers; no external id is reconstructed from them. The recorded storno/corrective economic-event linkage is consistent with the model (`docs/szamlazz-hu-behaviour.md:78,104–120`).
- `alap/afatipus` remains a separate invoice-level string. Its XSD annotation concerns non-Hungarian VAT due to another EU member; it is not substituted for the printed row's VAT category or used to calculate tax.
- The two payment fields remain distinct. [HU description][ah]: `fizmod` is the stored arbitrary string; `fizmodunified` is its normalized value, with unrecognized methods mapped by the vendor to “egyéb”. The sample's English `other` is preserved rather than rejected.
- `alap/email` is distinct from buyer email. Reversal status remains optional; the model does not invent a storno-number field on an original.

### Buyer and buyer ledger — 18 declarations

Public: `ops/query_xml.rs:341–398`. Wire/conversion: `:828–913`.

| Path | XSD | Public mapping |
|---|---|---|
| `vevo/id`, `nev`, `azonosito` | O int, R string, O string | `BuyerInfo.id: i64?`, `name: String`, `identifier: String?` |
| `vevo/cim`, `postacim` | R/O complex | `address: Address?`, `postal_address: BuyerPostalAddress?` |
| `vevo/email`, `adoszam`, `csoportazonosito`, `adoszameu` | O/R/O/O strings | `email`, `tax_number`, `group_id`, `eu_tax_number: String?` |
| `vevo/lokacio` | R int | `location: i64?` |
| `vevo/privatePersonIndicator` | R boolean | `private_person: bool`, absent/empty false |
| `vevo/fokonyv` | O complex | `ledger: BuyerLedgerInfo?` |
| `fokonyv/vevo`, `vevoazon` | O strings | `BuyerLedgerInfo.account`, `buyer_id: String?` |
| `fokonyv/datum` | O date | `date: Date?` |
| `fokonyv/folyamatostelj` | O boolean | `continuous_fulfillment: bool?` |
| `fokonyv/elszDatTol`, `elszDatIg` | O dates | `settlement_from`, `settlement_to: Date?` |

Numeric `id`, partner `azonosito` and ledger `vevoazon` are three distinct values. `lokacio` means 1 domestic, 2 EU, 3 outside EU, -1 unknown; it is **not** the create request's `adoalany` code set. Unknown integers survive. Mixed-case buyer settlement dates are explicitly renamed, correctly differing from item/financial lowercase dates. The buyer mutability caveat at `:360–365` is appropriately bounded to the recorded later-create observation (`docs/szamlazz-hu-behaviour.md:111`), not an immutable-at-issuance promise.

### Printed items and item ledger — 21 declarations, including list wrapper

Public: `ops/query_xml.rs:400–463`. Wire/conversion: `:915–998`.

| Path | XSD | Public mapping |
|---|---|---|
| `tetelek/tetel` | 1..unbounded complex | `items: Vec<DocumentItem>`; empty wrapper tolerated |
| `tetel/nev`, `azonosito` | R/O strings | `DocumentItem.name: String`, `id: String?` |
| `mennyiseg`, `mennyisegiegyseg`, `nettoegysegar` | R double/string/double | `quantity: Decimal`, `unit: String`, `unit_price: Decimal` |
| `afatipus`, `afakulcs` | O restricted string, R nonnegative double | `vat_type: String?`, `vat_rate_code: String` |
| `netto`, `arresafaalap`, `afa`, `brutto` | R/O/R/R double | `net_value: Decimal`, `margin_vat_base: Decimal?`, `vat_value`, `gross_value: Decimal` |
| `megjegyzes`, `sztetordering` | O string, R int | `comment: String?`, `ordering: i64?` |
| `fokonyv` | O complex | `ledger: DocumentItemLedger?` |
| `fokonyv/arbevetel`, `afa` | O strings | `DocumentItemLedger.revenue_account`, `vat_account: String?` |
| `fokonyv/gazdasagiesemeny`, `gazdasagiesemenyafa` | O strings | `economic_event`, `vat_economic_event: String?` |
| `fokonyv/elszdattol`, `elszdatig` | O dates | `settlement_from`, `settlement_to: Date?` |

No sorting, sign filtering or recalculation occurs. `sztetordering` is metadata, not an imposed vector sort. `fokonyv/afa` is a ledger account string, distinct from row tax money. Negative storno quantity and totals are preserved, matching recorded B1 (`docs/szamlazz-hu-behaviour.md:79`). `arresafaalap` is retained independently, including explicit zero. `vat_rate()` prioritizes nonblank `afatipus`, otherwise interprets the numeric token while leaving `vat_rate_code` available.

### Financial items and labels — 12 declarations, including reusable label/list structures

Public: `ops/query_xml.rs:465–505`. Wire/conversion: `:1000–1050`.

| Path | XSD | Public mapping |
|---|---|---|
| `qutetek/qutet` | 0..unbounded complex | `financial_items: Vec<FinancialItem>` |
| `qutet/nev`, `afatipus`, `afakulcs` | R string, O restricted string, R nonnegative double | `FinancialItem.name: String`, `vat_type: String?`, `vat_rate_code: String` |
| `netto`, `afa`, `brutto` | R doubles | `net`, `vat`, `gross: Decimal` |
| `elszdattol`, `elszdatig` | O dates | `settlement_from`, `settlement_to: Date?` |
| `afalevon` | R int | `deductible_vat: i64` |
| `cimkek` | O complex | `labels: Vec<String>` |
| `cimkek/cimke`, reused at root and financial row | 0..1 string | Each decoded label in `Vec<String>`; more than one tolerated |

`qutet` has no price/quantity fields in the schema. Its distinct financial-row model is correct. The XSD gives `afalevon` only `type="int"`, no unit or range; rustdoc `:491–494` explicitly avoids percentage arithmetic. The suggested QUiCK expansion at `:465–470` is qualified speculation, not asserted vendor terminology. No missing financial ledger sub-block was found: financial settlement dates are direct children, unlike printed-item ledger dates.

### Totals — 10 declarations

Public: `types.rs:1055–1107`. Wire/conversion: `xml.rs:806–889`.

| Path | XSD | Public mapping |
|---|---|---|
| `osszegek/afakulcsossz` | 1..unbounded complex | `Totals.by_vat_rate: Vec<VatTotal>`; absence tolerated |
| `osszegek/totalossz` | R complex | `Totals.total: GrandTotal`, required |
| `afakulcsossz/afatipus`, `afakulcs` | O restricted string, R nonnegative double | `VatTotal.vat_type: String?`, `vat_rate_code: String` |
| `afakulcsossz/netto`, `afa`, `brutto` | R doubles | `VatTotal.net`, `vat`, `gross: Decimal` |
| `totalossz/netto`, `afa`, `brutto` | R doubles | `GrandTotal.net`, `vat`, `gross: Decimal` |

All three subtotal components and all three grand-total components are direct mappings, not reconstructed from lines. This preserves the vendor sample's inconsistent totals and the recorded independently rounded monetary values (P60, `docs/szamlazz-hu-behaviour.md:159–162`). Special VAT category precedence agrees across line, financial row and subtotal helpers.

### Credit entries — 8 declarations, including list wrapper

Public: `ops/query_xml.rs:507–533`. Wire/conversion: `:1052–1087`.

| Path | XSD | Public mapping |
|---|---|---|
| `kifizetesek/kifizetes` | 1..unbounded complex | `credit_entries: Vec<RecordedCreditEntry>`; absent/empty tolerated |
| `datum`, `jogcim`, `osszeg` | R date/string/double | `RecordedCreditEntry.date: Date`, `title: PaymentMethod`, `amount: Decimal` |
| `megjegyzes`, `bankszamlaszam` | O strings | `comment`, `bank_account: String?` |
| `banktranzid`, `devizaarf` | O int/double | `bank_transaction_id: i64?`, `exchange_rate: Decimal?` |

The query has **no five-entry read cap**. It preserves vendor ordering without calling it submission order; recorded D7 returned five entries out of submission order (`docs/szamlazz-hu-behaviour.md:134`). Required `datum` differs from optional invoice/ledger dates: missing/empty is an error.

[HU bank-account annotation][ah]: “A kifizetés ténylegesen erről a bankszámláról érkezett, vagy a számlán szereplő bankszámlaszám (ha a küldő bankszámlaszám nem ismert)” — the actual sender's account, otherwise the printed invoice account if sender unknown. Current `:525–528` matches, without claiming the field distinguishes those sources. `jogcim` is preserved as an open payment-method title, not a fixed translated enumeration.

**Inventory total:** 20 + 8 + 28 + 18 + 21 + 12 + 10 + 8 = **125**. The nineteen structures are the anonymous root plus `cimTipus`, `cimpostaTipus`, `bankTipus`, `szallitoTipus`, `alapTipus`, `fokonyvvevoTipus`, `vevoTipus`, `fokonyvtetelTipus`, `tetelTipus`, `tetelekTipus`, `afakulcsosszTipus`, `totalosszTipus`, `osszegekTipus`, `kifizetesTipus`, `kifizetesekTipus`, `cimkekTipus`, `qutetekTipus`, `qutetTipus`.

## 3. Waybill and request-only data: exhaustively checked boundary

The XML response page's schema navigation links [invoice creation][ix]. That page describes root `xmlszamla`; the fetched query schema describes root `szamla`. Neither the queried response example nor its downloaded/shared inline XSD declares `fuvarlevel`.

Every carrier declaration was checked against the creation documentation [EN][ix], [HU][ihx] and [creation download][id]. `ops/waybill.rs:1–5` correctly describes these as **invoice-creation** data. Its full field set is:

| Creation-only block | Declared children / types | Rust location |
|---|---|---|
| `fuvarlevel` | Optional strings `uticel`, `futarSzolgalat`, `vonalkod`, `megjegyzes`; optional `tof`, `ppp`, `sprinter`, `mpl` blocks | `ops/waybill.rs:87–113,131–178` |
| `tof` | Optional strings `azonosito`, `shipmentID`, `countryCode`, `zip`, `service`; optional int `csomagszam` | `:9–24,137–148` |
| `ppp` | Optional strings `vonalkodPrefix`, `vonalkodPostfix` | `:26–33,149–154` |
| `sprinter` | Optional strings `azonosito`, `feladokod`, `iranykod`, `vonalkodPostfix`, `szallitasiIdo`; optional int `csomagszam` | `:35–50,155–166` |
| `mpl` | Required strings `vevokod`, `vonalkod`, `tomeg`; optional string `kulonszolgaltatasok`; optional double `erteknyilvanitas` | `:52–85,167–176` |

The creation annotation calls `uticel` unused and recommends `sprinter/iranykod`; the generic barcode is fallback when carrier data is insufficient. MPL weight is a **string**, despite numeric-looking content; ids/barcodes stay strings. These meanings are already represented. Request count types/range validation are not query-response defects. This inspection establishes the output boundary, not full creation validation or successful carrier rendering.

Likewise, no queried `arfolyam` object, buyer telephone/comment/signer configuration, seller email configuration, template/preview/simple-image flags, erasure-code list or external-id element is declared. Query exchange information is instead `alap/devizabank`, `alap/devizaarf`, and credit-entry `devizaarf`. The recorded external id is never echoed (`docs/szamlazz-hu-behaviour.md:68`). Adding presumed response paths would invent protocol content.

## 4. PDF response and metadata coverage

[PDF response documentation][ps]: version 2 returns “Structured `xmlszamlavalasz` with base64-encoded PDF inside `<pdf>`”; “In both cases, additional parameters may also arrive in the HTTP response header.” The inline and [downloaded envelope schema][sd] declare three verdict/error fields and six payload fields.

| Wire fact | Current mapping / behavior | Exact location |
|---|---|---|
| `sikeres` | Required unique scalar; true/false/1/0; empty reads false, never success | `xml.rs:460–481,750–783` |
| `hibakod`, `hibauzenet` | Typed known error or retained unknown code; absent code not invented; unusable diagnostic can be absent | `xml.rs:460–504` |
| `szamlaszam` | `InvoicePdf.invoice_number`, body then decoded header, usable number required | `ops/query_pdf.rs:40–41,83–93`; `ops/envelope.rs:120–133,275–280,326–329` |
| `szamlanetto`, `szamlabrutto` | `net_total`, `gross_total: Option<Decimal>` | PDF `:42–45,88–89`; envelope `:223–237` |
| `kintlevoseg` | `outstanding: Option<Decimal>`; absent is not zero | PDF `:46–49,90`; envelope `:238–243` |
| `vevoifiokurl` | `customer_account_url: Option<String>`, body before decoded header | PDF `:50–53,91`; envelope `:135–143,244` |
| `pdf` | Required decoded `Pdf`; missing/blank is `Missing("pdf")` | PDF `:54–55,92`; envelope `:116–117,145–148` |

Money fallback headers are respectively `szlahu_nettovegosszeg`, `szlahu_bruttovegosszeg`, `szlahu_kintlevoseg`. Body absence/blank permits fallback; malformed nonblank body money fails rather than trusting a competing header (`ops/envelope.rs:158–168,344–370`). Body values win even when a header disagrees. No amounts are derived from another field.

Raw header lookup is case-insensitive and first-match (`wire.rs:182–199,227–249`). Encoded textual values receive one decoding pass: `+` becomes space, `%2B` becomes literal plus (`:343–351`). Numeric/code headers remain raw. XML URL values receive entity decoding only, preserving literal `+`, `%2B` and escaped ampersands; they are not interpreted as header encodings. [Creation-response header descriptions][ir] distinguish encoded numbers/error text from unencoded net/gross; they are supplementary shared vocabulary, not proof every creation header appears on PDF reads.

`parse_issued` also reads auxiliary document id/payment method/notification status into `CreatedInvoice`, but `InvoicePdf` does not project them (`ops/envelope.rs:223–248`; `ops/query_pdf.rs:86–93`). The PDF-specific body schema declares none of these. No PDF-specific source or recorded live capture establishes that their absence is a lost promised typed query field. Raw-response users can inspect received headers; the bundled `Client::send` returns only the typed projection (`client.rs:374–405`). Optional wider metadata exposure remains a design opportunity, not a confirmed defect.

The XML document model does not merge success-header outstanding/customer URL into `InvoiceDocument`; neither is a `szamla` body field. Recorded B8 explicitly notes no XML `kintlevoseg` (`docs/szamlazz-hu-behaviour.md:80`).

## 5. Accepted policy deviations and fidelity limits

These are observable choices, separated from missing/wrong mappings. None is presented as vendor emission proved by a synthetic test.

### P-A — Sparse content and open sets

The XSD requires several values omitted by the official example: `gazdEsemAzon`, `keszpenz`, `katafokonyv`, buyer `lokacio`/`privatePersonIndicator`, item `sztetordering`. Existing options/defaults accommodate that example. They also tolerate some omissions without live evidence. In particular, missing/empty independent content booleans become false; optional `test`, `reversed` and buyer continuous-fulfillment retain absence. Required root blocks, identity/code fields, numeric item values and totals still have their own gates. This is not full XSD validation.

`DocumentType` (`types.rs:768–858`) supports SZ/D/ES/VS/HS/SS/SL and retains `JS` from the shared annotation as `Other`. All 21 schema VAT tokens are representable, including `TEHK` as `Other`; all 15 language tokens and future strings survive. `PaymentMethod` preserves arbitrary `fizmod` and `jogcim`, including the official English sample tokens. Currency does not rewrite `Ft` into `HUF`. Unknown source/location/appearance integers survive. Missing named variants are not missing data.

The three raw `afakulcs` fields deliberately do not enforce the schema's nonnegative-double facet. Helpers return an exactly representable percentage or `Other`, retaining the original raw token; `afatipus` takes precedence (`ops/query_xml.rs:456–463,499–505`; `types.rs:296–324,1087–1094`). No number is silently substituted for an unrepresentable rate.

### P-B — Exact finite numbers and integer widening

`xml.rs:629–647`, `number.rs:63–141` and `ops/envelope.rs:344–370` parse finite decimal/exponent text without f64. Coefficient trailing zeros are removed before representability checks; magnitude/scale failure is an error, not rounding. XML accepts signs, decimal points and exponents; commas/grouping/underscores are refused. HTTP money separately allows one decimal comma and outer SP/HTAB, so `100,01` becomes 100.01 and `1,234` means 1.234, not 1234. P60 recorded comma monetary headers (`docs/szamlazz-hu-behaviour.md:160`).

**Concrete boundary:** `1e-29`, excess significant precision and `1e100` fail in modeled Decimal amount fields; `100e-30` and equivalent exact spellings within range succeed. These cases passed the existing `tests/numeric_fidelity.rs:112–186`. XSD `double` is a larger domain, but README `:414–421` explicitly selects exact finite Decimal. No exceptional amount from a real query was established. Request arithmetic helpers at `number.rs:6–59` are not used to recompute queries.

Every queried XSD integer is deliberately widened to i64 (`ops/query_xml.rs:10–25`), including appearance. This is broader than the fetched schema's int widths, not truncation. Malformed/out-of-i64 values fail. Textual account identifiers remain strings.

### P-C — Civil dates, not full XSD-date lexical identity

All **eleven date positions** are covered: three invoice dates; buyer ledger date/from/to; item ledger from/to; financial-row from/to; required credit-entry date. `xml.rs:649–709` preserves the printed civil day, discards a complete `Z` or valid `±hh:mm` suffix through ±14:00, and does not convert to UTC. Checked slicing prevents multibyte-boundary panics.

`ops/query_xml.rs:1333–1469` exercises every date position, optional blank absence, invalid calendar/offset/suffix and multibyte content. Those tests passed. Nonblank invalid optional dates **fail the Agent query**; this must not be replaced in the review by Adatkapcsolat's separate invalid-date-as-content policy. The initial Jiff parse retains legacy finite-domain forms (tested compact dates, year zero and signed expanded years). README `:238–242` explicitly disclaims strict XSD lexical validation. No general rejection of every datetime-like spelling, arbitrary year support or byte-for-byte date retention is claimed.

### P-D — Character fidelity, URL normalization and raw content

Queried business strings/reference numbers preserve decoded nonblank characters (`xml.rs:727–739`; `ops/query_xml.rs:1089–1096`). Existing business-text tests confirm entity/CRLF/NBSP behavior. Optional empty and XML-whitespace-only strings intentionally collapse to None. Unknown XML fields and arbitrary future JSON fields are not retained; same-version JSON round trips preserve modeled values, Decimal strings, code tokens/integers, dates and base64 PDF.

PDF-envelope URLs use `empty_as_none`, which Unicode-trims before constructing the string (`ops/envelope.rs:114–115,135–143`; `xml.rs:713–725`). Thus `<vevoifiokurl>&#160;opaque:x&#160;</vevoifiokurl>` yields `Some("opaque:x")` by direct code tracing. This is a real **text-fidelity limit**, but README `:262–269` explicitly excludes URLs from general business-text preservation. No changed useful vendor link was established. Retain it as accepted normalization policy, not a recycled fixed/operational finding. Envelope invoice numbers also have explicit trimming (`ops/envelope.rs:120–133,326–329`), while `szamla` invoice-number strings are preserved.

### P-E — Artifacts and shared verdict policy

- `Pdf::from_base64` (`types.rs:104–117`) compacts whitespace and decodes standard base64. It is a byte wrapper, not a PDF magic/signature/structure validator. Synthetic `%PDF-` is not a complete renderable PDF.
- XML query returns None for absent/blank PDF even when requested, and decodes nonblank PDF even when not requested (`ops/query_xml.rs:604–607`). Nonblank malformed base64 fails the whole result. Dedicated PDF query instead requires the artifact (`ops/query_pdf.rs:92`). The official placeholder is not evidence that malformed real PDF content should be accepted.
- Both queries check nonblank down header, error header, known non-2xx status, then body (`wire.rs:278–311`). A body-only code 7 at 200 is an API answer; at 500 without a code header it is a status error under the current precedence policy. Status alone does not identify the responder.
- PDF query inherits numbered-code-56 leniency from `parse_issued` (`ops/envelope.rs:179–248,275–280`). A fabricated false-verdict 56 with usable number and valid PDF can become `InvoicePdf`, without its notification flag. No fetched PDF documentation establishes notification failure on a read; recorded probes could not trigger 56 (`docs/szamlazz-hu-behaviour.md:153,171`). This remains unverified shared policy, not an observed query incident. Malformed/duplicate body identity now propagates rather than becoming header fallback (`ops/envelope.rs:205–209,295–313`).

## 6. Shared XML fidelity checks

`xml.rs:183–257` checks UTF-8 and one completed expected root through EOF; `:262–283` checks lexical syntax/references even in ignored text/attributes. DTDs are refused, not resolved. The declaration/PI policy is at `:378–441`; it is not alternate-encoding transcoding. Official examples use UTF-8. No general XML conformance certification follows from these checks.

`NamespaceReader` (`:19–137`) resolves element namespaces, normalizes declarations before reserved-binding checks, and rejects duplicate expanded attribute names. The `xmlns` element prefix is refused at `:77–83`; PI targets must be colon-free at `:424–431`. `protocol_text` (`:292–360`) filters whole foreign subtrees and canonicalizes protocol QNames. A placeholder prevents `tr<foreign/>ue` from becoming `true`. Serde then owns parent paths, unique scalar/container fields and list projection; overlapped lists are enabled in `crates/szamlazz-agent/Cargo.toml:23`.

Existing controls executed successfully include mixed-prefix/interleaved `tetel`, `qutet`, `kifizetes`, `afakulcsossz`; foreign identity/reversal; namespace normalization/reserved bindings; expanded duplicate attributes; nested scalars; malformed prolog/epilog/truncation; illegal characters/references in business and ignored content; and legal quoting/non-ASCII names. See `tests/response_namespaces.rs:36–103,118–152,194–357` and `tests/response_completion.rs:12–87,105–193`.

No query-path panic or data substitution was reproduced. This is not an extreme-nesting/resource benchmark: body buffering, projected XML and PDF allocations remain. Resource limits and full transport behavior are not certified by this scoped review.

## 7. Vendor ambiguity and source defects

### VQ-1 — HU PDF request schema contradicts working selector contract

**Severity:** P3 vendor clarification; **not a confirmed Rust defect**. High confidence in the freshly fetched conflict, no new live evidence of alternative-order acceptance.

**Exact code:** `ops/query_pdf.rs:68–78`, selector model `types.rs:1034–1053`.

**Official quotes / URLs:**

- [HU PDF XML+XSD][phx] contains `targetNamespace="http://www.szamlazz.hu/xmlszamlapdf"xmlns:tns=...`, without an attribute separator. Its readable declarations say `<element name="szamlaszam" type="string" maxOccurs="1" minOccurs="1">` and order `szamlaszam → valaszVerzio → rendelesSzam → szamlaKulsoAzon`.
- [EN PDF XML+XSD][px] and [download][pd] instead say `szamlaszam ... minOccurs="0"` and order `szamlaszam → rendelesSzam → valaszVerzio → szamlaKulsoAzon`.
- [HU request][phr] says identification by invoice number, order number “vagy külső azonosító” (or external identifier), agreeing with the independent-selector model.

**Concrete consequence/reproduction:** the current writer produces the following order-only structure, consistent with EN/download:

```xml
<xmlszamlapdf xmlns="http://www.szamlazz.hu/xmlszamlapdf">
  <szamlaagentkulcs>placeholder</szamlaagentkulcs>
  <rendelesSzam>ORDER-1</rendelesSzam>
  <valaszVerzio>2</valaszVerzio>
</xmlszamlapdf>
```

A consumer attempting to load the original HU XSD first encounters malformed XML. Even after fixing that separator, the declarations reject this structure for missing number/wrong order; an external-id-only request also lacks its required number. This is a source-level demonstration, not a freshly executed validator run.

**Live-evidence adjudication:** recorded external-id PDF retrieval and newest-holder behavior (`docs/szamlazz-hu-behaviour.md:63–68`) support keeping independent external lookup. They do not prove every PDF order sequence. The Rust writer agrees with EN/download and both languages' request prose; changing it, adding a dummy number or retrying another shape is unjustified. Ask the vendor to align the HU XSD or explicitly document accepted variants.

### Other source limitations — informational, no implementation-defect severity

| Source observation | Concrete impact / adjudication |
|---|---|
| [XML success sample][xs] literally contains `<pdf>The receipt .pdf can be found here in BASE64 encoding</pdf>` | Unmodified body fails base64. The corresponding upstream test explicitly substitutes synthetic PDF text to exercise the remaining fields (`tests/upstream.rs:710–832`). This is an illustrative-source defect, not live valid-PDF rejection. |
| [PDF success sample][ps] contains `....` in base64 | It is abbreviated, not a complete download. Existing corpus tests test original refusal and a transformed input (`tests/upstream.rs:689–700`). Successful decoding of edited bytes does not recover the omitted PDF. |
| XML sample omits required fields and uses `fizmodunified=other` outside the Hungarian enumeration | Supports permissive reading, not proof every tolerated omission occurs live. Numeric/type/path mapping remains correct. |
| XML sample line values 380/76/456 disagree with totals 464/93/557 | The reader correctly preserves both; no arithmetic repair should be inferred from a documentation example. |
| [HU XML request example][xhx] has empty `<pdf></pdf>` but says true if needed, otherwise false | Writer emits a valid explicit boolean, consistent with the actual type and instruction. |
| [PDF response][ps] describes version `1 or not set`, while [request XSD][pd] requires `valaszVerzio` | Explicit 2 avoids the conflict; no missing PDF-download capability. |
| [Shared annotation][ah] calls appearance string and ordering double in comments, while [XSD][sx] types both int | Integer representation is justified; appearance meaning is independently supported by recorded P73. Fractional ordering emission is unestablished. |
| [XSD][sx] gives `cimke` maximum one and `afalevon` no unit/range | A vector preserves all allowed labels and tolerates extras. No percentage semantics or multiple-label emission is established. |
| [Request][xr] says “last” without an ordering key | Recorded latest-issued behavior is bounded; id versus issue-date ordering remains unverified (`docs/szamlazz-hu-behaviour.md:208–209`). Query success is not an ownership assertion. |

## 8. Optional unsupported functionality

These are outside the current typed query contract, not newly confirmed defects:

1. **Raw PDF/text version 1.** The vendor documents it, but `QueryInvoicePdf` deliberately requests 2. A selectable raw mode/streaming artifact interface would be additional functionality.
2. **Extra success-header projection.** Exposing `szlahu_id`, payment method or all raw headers on `InvoicePdf`/`InvoiceDocument` could help consumers. Generic optional-header prose does not establish every field as promised PDF metadata. Core `RawResponse` callers already retain received headers.
3. **Raw XML/unknown-field retention or partial results after malformed optional content.** The public model is a projection. Adding raw body retention or a document-plus-artifact-error result is a design change, not a missing declared mapping.
4. **Named variants for every legacy/open token.** `JS`, `TEHK` and additional normalized methods are preserved without dedicated variants. No information is lost by that choice.
5. **Waybill readback.** No fetched query source establishes such an output. This needs vendor response evidence before an extension can be specified; creation support alone is insufficient.

## 9. Historical-review closure, consulted after independent audit

Read the query reports dated September 10 (`…queries`, `…current-queries`, `…f83e5fd-queries`) and September 11 (`…queries`), plus the current and September 11 adjudications. None supplies an unfixed finding merely by having been reported before.

| Historical lead | Current HEAD disposition / evidence |
|---|---|
| Missing internal-outgoing restriction | Closed: `ops/query_xml.rs:51–53,239–242`, confirmed against fresh EN/HU request prose |
| Missing PDF outstanding/customer URL | Closed: `ops/query_pdf.rs:46–53,90–91`; body/header/default tests pass |
| Mixed-QName/interleaved list failures | Closed for reported cases: canonical projection `xml.rs:292–360`, overlapped lists, current namespace tests pass |
| Escaped reserved namespace and duplicate expanded attributes | Closed for reported cases: `xml.rs:73–135`, current namespace tests pass |
| Illegal XML characters/ignored malformed syntax | Closed for reported cases: `xml.rs:262–283`, current completion tests pass |
| Reserved `xmlns` element and colon PI gap | Closed: `xml.rs:77–83,424–431`; current tests reject both while retaining legal controls |
| Numbered-56 malformed-identity fallback | Closed at shared boundary: `ops/envelope.rs:205–209,295–313`; current identity-fallback tests pass. No independent PDF notification event inferred. |
| URL boundary trimming | Still present as explicit envelope policy, P-D; not re-counted as an operational defect |
| Timezone/numeric fidelity, bank-account meaning, buyer mutability, deductibility percentage, appearance flag | Current mapping/helper/docs match fresh sources or explicitly bounded policy; relevant tests pass |
| HU PDF XSD conflict | Freshly re-fetched after historical comparison; remains vendor ambiguity VQ-1 |

The old report's internal discussion of permitting colon-bearing PIs is not applicable to this HEAD. Current code and executed controls refuse them. Prior scratch results/hashes/XSD-validation claims are not represented as executions performed by this review.

## 10. Verification record and remaining limits

Executed from the repository, default crate features, no ignored tests:

```text
cargo test -p szamlazz-agent --locked --offline --lib ops::query_
  29 passed
cargo test -p szamlazz-agent --locked --offline --test business_text --test numeric_fidelity --test response_headers --test response_namespaces --test response_completion --test upstream
  48 passed: 2 + 6 + 14 + 11 + 4 + 11
```

**77 distinct selected tests passed, no failures.** Shared suites also contain checks of other operations; those passes do not extend this audit's ownership. The workspace upstream corpus is present; its tests can skip outside the workspace, and its explicit sample transformations are not live evidence. `fixtures/SOURCES.md:3–39,58–79,90–140,142–169` distinguishes official reference material, generated goldens, packaged synthetic data and modified/illustrative examples. The project-modified create schema is not the query response schema.

Unverified: deployed HU alternative order, frequency of sparse/extension forms, uncommon numeric/date emission, 56-on-PDF-query, full artifact validity, extreme nesting/resource costs, or auxiliary header emission on every query. No confirmed implementation defect is concealed by those limits, and no absence/time-based reconciliation claim is derived from a query returning 7. Recorded deleted/consumed proformas and shared external ids specifically prevent treating query absence as “never existed” or selector success as uniqueness.

## Fetched URL register

All successful entries below were fetched during this review on 2026-09-11. HTML pages include rendered examples and tabbed schemas; `.xsd` URLs returned XML text. URLs are official documentation/download endpoints, not live Agent calls.

| Group | URLs |
|---|---|
| Required starting pages | [XML category][xc]; [PDF category][pc] |
| XML EN | [request][xr]; [response][xs]; [XML+XSD][xx] |
| XML HU | [request][xhr]; [response][xhs]; [XML+XSD][xhx] |
| PDF EN | [request][pr]; [response][ps]; [XML+XSD][px] |
| PDF HU | [request][phr]; [response][phs]; [XML+XSD][phx] |
| Direct query/schema downloads | [xmlszamlaxml.xsd][xd]; [xmlszamlapdf.xsd][pd]; [szamla.xsd][sx]; [xmlszamlavalasz.xsd][sd] |
| Shared returned-document meanings/schema | [HU outgoing invoices][ah] |
| Followed creation/schema/header navigation | [EN creation XML][ix]; [HU creation XML][ihx]; [creation XSD download][id]; [creation response][ir] |
| Shared request/error navigation | [Basics category][bc]; [sending requests][sr]; [error handling][eh] |

Two exploratory paths returned **403** and supply no evidence: `https://docs.szamlazz.hu/agent/basics/response` and `https://docs.szamlazz.hu/agent/basics`. The actual category and operation response URLs above succeeded. No W3C/first-party PHP download or account-response acquisition is claimed in this review.

[xc]: https://docs.szamlazz.hu/agent/category/query-document-xml
[pc]: https://docs.szamlazz.hu/agent/category/query-document-pdf
[xr]: https://docs.szamlazz.hu/agent/querying_xml/request
[xs]: https://docs.szamlazz.hu/agent/querying_xml/response
[xx]: https://docs.szamlazz.hu/agent/querying_xml/xml
[xhr]: https://docs.szamlazz.hu/hu/agent/querying_xml/request
[xhs]: https://docs.szamlazz.hu/hu/agent/querying_xml/response
[xhx]: https://docs.szamlazz.hu/hu/agent/querying_xml/xml
[pr]: https://docs.szamlazz.hu/agent/querying_pdf/request
[ps]: https://docs.szamlazz.hu/agent/querying_pdf/response
[px]: https://docs.szamlazz.hu/agent/querying_pdf/xml
[phr]: https://docs.szamlazz.hu/hu/agent/querying_pdf/request
[phs]: https://docs.szamlazz.hu/hu/agent/querying_pdf/response
[phx]: https://docs.szamlazz.hu/hu/agent/querying_pdf/xml
[xd]: https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd
[pd]: https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd
[sx]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[sd]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd
[ah]: https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak
[ix]: https://docs.szamlazz.hu/agent/generating_invoice/xml
[ihx]: https://docs.szamlazz.hu/hu/agent/generating_invoice/xml
[id]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd
[ir]: https://docs.szamlazz.hu/agent/generating_invoice/response
[bc]: https://docs.szamlazz.hu/agent/category/basics
[sr]: https://docs.szamlazz.hu/agent/basics/sending-requests
[eh]: https://docs.szamlazz.hu/agent/basics/error-handling
