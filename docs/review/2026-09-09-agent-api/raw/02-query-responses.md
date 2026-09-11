# Számla Agent invoice XML/PDF queries: conformance audit

**Date:** 2026-09-09. **Reviewed revision:** `a804c740eb8446211c1cdca3eea4fb93d298d25d` (clean working tree at the start).

## Summary

The current `InvoiceDocument` covers **every element declared by the freshly fetched queried-`szamla` XSD**, including financial items, labels, both parties' address variants, ledger metadata, VAT subtotals, appearance and all seven credit-entry fields. The request writers use the correct action names, namespaces, selectors, settings and element order. Body-only query errors are handled.

Four actionable findings remain, with their evidential limits stated below:

| ID | Priority | Finding | Confidence / reach |
|---|---|---|---|
| QR-01 | P2 | Valid timezone-qualified `xs:date` values reject the entire XML query | Reproduced; schema-valid trigger, not observed in the recorded live queries |
| QR-02 | P2 | PDF query discards documented outstanding amount and customer account URL | Reproduced; both are explicitly in the PDF response schema, optional on the wire |
| QR-03 | P3 | Credit-entry bank-account rustdoc describes the receiving account instead of the sender/fallback account | Direct contradiction of both official language versions; parsing itself preserves the value |
| QR-04 | P3 | PDF header-total fallback rejects the comma-decimal form recorded from szamlazz.hu | Reproduced; conditional on an absent body total; comma form observed on create, not established on PDF query |

P2 means a functional interoperability or data-access defect worth fixing in normal development; P3 means a narrower documentation or conditional interoperability defect. No P0/P1 issue was established. Acceptance of schema-incomplete documents and other deliberate leniency are **not** counted as findings.

## Scope, method and sources

Reviewed the complete request/parser/model implementations in:

- `crates/szamlazz-agent/src/ops/query_xml.rs` (including all private wire structs, conversions and tests).
- `crates/szamlazz-agent/src/ops/query_pdf.rs` and its shared `ops/envelope.rs` path.
- Relevant parts of `types.rs`, `xml.rs`, `wire.rs`, `client.rs`, `ops.rs`, `error.rs`'s exercised error behaviour, crate README, `tests/upstream.rs`, synthetic/golden fixture references, and `fixtures/SOURCES.md`.
- **All** of `docs/szamlazz-hu-behaviour.md`, especially lines 40–45, 59–80, 96–98, 104–111, 129–145 and 155–162. Its test-account observations take precedence over a conflicting illustrative example or general prose for this audit.

Code citations use the filenames under `crates/szamlazz-agent/src/` listed above (`query_xml.rs`, `query_pdf.rs` and `envelope.rs` are under `src/ops/`). `behaviour.md` abbreviates `docs/szamlazz-hu-behaviour.md`. All line ranges refer to the reviewed revision.

Official documentation and the four schemas below were fetched afresh over HTTPS during this audit. The docs pages displayed build **`v202608271632`**. The cached corpus was a navigation/comparison aid, not the freshness evidence. `fixtures/SOURCES.md:48–49,68–71,96–102,121–135` maps the query examples and schemas and warns that inline and downloaded schemas can diverge. The fresh request schemas and the inline request schemas agree on declarations/order; the fresh `szamla.xsd` and inline outgoing-invoice XSD agree on the field tree audited below. No new query field was found relative to the cached `fixtures/upstream/agent/xsd/szamla.xsd`.

### Official source key (all fetched 2026-09-09)

| Key | Official URL | Relevant statement / excerpt |
|---|---|---|
| XREQ | https://docs.szamlazz.hu/agent/querying_xml/request | “only the data of internal outgoing invoices (issued in Számlázz.hu) can be retrieved via this interface”; identifies all three selectors and says the last document with an order number is returned |
| XXML | https://docs.szamlazz.hu/agent/querying_xml/xml | “the order of the fields is fixed, **they cannot be interchanged**”; inline request XSD |
| XRESP | https://docs.szamlazz.hu/agent/querying_xml/response | Success: “Full `szamla` XML document”; error: “`xmlszamlavalasz` XML with `<sikeres>false</sikeres>`, `<hibakod>` and `<hibauzenet>`”; missing selector gives **7** |
| PREQ | https://docs.szamlazz.hu/agent/querying_pdf/request | Multipart `action-szamla_agent_pdf`; invoice number, order number or external identifier |
| PXML | https://docs.szamlazz.hu/agent/querying_pdf/xml | Fixed field order; `valaszVerzio` is required `int`; “2 = XML response with PDF in base64” |
| PRESP | https://docs.szamlazz.hu/agent/querying_pdf/response | Version 2: “Structured `xmlszamlavalasz` with base64-encoded PDF inside `<pdf>`”; “additional parameters may also arrive in the HTTP response header”; full inline response XSD |
| XS | https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd | Fresh standalone XML-query request schema |
| PS | https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd | Fresh standalone PDF-query request schema |
| S | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd | Full queried-document schema; `targetNamespace="http://www.szamlazz.hu/szamla"`, `elementFormDefault="qualified"` |
| E | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd | Shared reply schema; `targetNamespace="http://www.szamlazz.hu/xmlszamlavalasz"` |
| OUT | https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak | Annotated example and inline XSD of the **same `szamla` document**; used for field meanings, not for imposing the Adatkapcsolat delivery protocol on queries |
| OUTHU | https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak | Hungarian original of the field annotations, including appearance, location and credit-entry bank account |
| AUTH | https://docs.szamlazz.hu/agent/basics/authentication | “either an Agent key (recommended) or a username and password” |
| SEND | https://docs.szamlazz.hu/agent/basics/sending-requests | Common endpoint, POST, multipart action table, case-sensitive names |
| ERR | https://docs.szamlazz.hu/agent/basics/error-handling | Typed error meanings; version-1 `[ERR]` text belongs to version-1 operations |
| CRESP | https://docs.szamlazz.hu/agent/generating_invoice/response | Shared header vocabulary: `szlahu_nettovegosszeg` “Net total (not URL encoded)”, gross equivalent, number URL-encoded |
| DATE | https://www.w3.org/TR/xmlschema-2/#date | §3.2.9.1: `'-'? yyyy '-' mm '-' dd zzzzzz?`; “the date and optional timezone are represented exactly the same way as they are for dateTime” |

The XML query response page references `szamla.xsd` indirectly through its sample and generating-invoice link. OUT supplies the explicit working download URL and full inline copy. The create-request schema `xmlszamla.xsd` is **not** substituted for the queried-document schema.

No live Számla Agent call, account operation, production edit or subagent was used. Local parser experiments are synthetic and I/O-free apart from building the temporary program. No claim below infers live frequency from a synthetic trigger.

## Ranked findings

### QR-01 — P2: timezone-qualified schema dates reject the document

**Code:** `query_xml.rs:703–708` (`kelt`, `telj`, `fizh`), `806–825` (buyer ledger), `947–960` (item ledger), `994–997` (financial-item settlement dates), `1034–1039` (credit-entry date); `xml.rs:269–283` passes optional text directly to `FromStr`. The required credit-entry date delegates directly to Jiff's serde implementation. There is no intervening XSD-date adapter anywhere on this path.

**Official requirement:** S declares each of these as `type="date"`, without a no-timezone restriction (cached navigation: `szamla.xsd:132–134,159–162,189–190,258,301–302`). DATE explicitly permits the optional timezone. A civil date model may deliberately discard the zone, but refusing a valid lexical spelling is a separate restriction.

**Concrete trigger:** In an otherwise valid response replace `<kelt>2026-09-09</kelt>` with `<kelt>2026-09-09Z</kelt>` or `<kelt>2026-09-09+02:00</kelt>`. Both fail with `invalid response XML: parsed value '2026-09-09', but unparsed input ... remains`. The same `Z` change to `kifizetes/datum` also fails.

**Impact:** One valid date spelling prevents retrieval of every other field and any included PDF. Optional dates are only lenient about absence/empty text: a non-empty value failing `Date::from_str` is fatal. This is not the separate Adatkapcsolat parser's lenient date policy, and that policy must not be attributed to `szamlazz-agent`.

**Fix direction:** Introduce explicit required/optional XSD-date readers on the query wire boundary. Validate and consume the complete date and optional `Z`/`±hh:mm` suffix, preserve the calendar date in `Date`, and document loss of zone information. Do not blindly truncate at byte 10 or silently accept arbitrary suffixes. Existing absence/empty semantics can remain.

**Regression check:** Table-test all eleven date positions: three core dates, three buyer-ledger dates, two item-ledger dates, two financial-item dates and the required credit-entry date. Plain date, `Z`, positive/negative offsets and boundary offset `+14:00` should preserve the same civil date. Invalid date, junk suffix, invalid offset and multibyte text must return a controlled error; optional empty/absent dates remain `None`. Verify required credit date independently because it does not use `empty_as_none`.

**Limit:** All dated official examples and recorded live results use plain dates. This is a confirmed schema compatibility gap, not evidence that today's production queries routinely fail. The deliberate `Option<Date>` for absent `telj` is correct (`behaviour.md:96`) and is not this finding.

### QR-02 — P2: `InvoicePdf` loses two fields explicitly documented for PDF queries

**Code:** `query_pdf.rs:36–48,75–83` exposes/copies only invoice number, net, gross and PDF. The full code check finds that `envelope.rs:93–105,123–131,205–228` already reads `kintlevoseg` and `vevoifiokurl`, with header fallback, and `CreatedInvoice` exposes them at `envelope.rs:43–48`. They are lost at the final PDF projection, not absent from the shared parser.

**Official requirement:** PRESP's **own** response schema (also E) contains:

```xml
<element name="kintlevoseg" type="double" maxOccurs="1" minOccurs="0"></element>
<element name="vevoifiokurl" type="string" maxOccurs="1" minOccurs="0"></element>
```

**Concrete trigger:** A successful numbered PDF response containing `<kintlevoseg>117</kintlevoseg>` and `<vevoifiokurl>https://example.invalid/invoice</vevoifiokurl>`. The local parse succeeds; serialized `InvoicePdf` contains only `invoice_number`, `net_total`, `gross_total`, `pdf`. Both supplied values are inaccessible from the result.

**Impact:** The normal PDF-query interface cannot return the documented current balance or buyer-facing URL. `Client::send` returns only `R::Response` and drops the raw reply (`client.rs:262–288`), so these cannot be recovered from that call by inspecting an accompanying raw response. A custom transport can retain `RawResponse`, which is a workaround rather than model coverage.

**Fix direction:** Add optional `outstanding` and `customer_account_url` fields to `InvoicePdf` and move the already-parsed values into them. The separate `document_id` header is auxiliary and is not an omitted element of PRESP's XSD; see the omissions section.

**Regression check:** Parse a PDF response with both body values, one with both only in headers, and one with neither. Assert values survive the PDF projection and JSON round-trip, with body-over-header precedence. Keep the missing-PDF refusal.

**Limit:** These fields are optional. The official PDF success example omits them, so this is not a claim that every PDF reply currently carries them. There is no documented rationale in the reviewed query interface for discarding them.

### QR-03 — P3: credit-entry bank-account meaning is reversed in rustdoc

**Code:** `query_xml.rs:511–512`: “Bank account the amount arrived **on** (`bankszamlaszam`).” Wire field and projection are correct at `1042–1043,1057`.

**Official evidence:** OUT's annotation says: “The payment was actually made **from this bank account**, or the bank account number on the invoice (if the sender's bank account number is unknown)”. OUTHU confirms: “A kifizetés ténylegesen **erről a bankszámláról érkezett**, vagy a számlán szereplő bankszámlaszám (ha a küldő bankszámlaszám nem ismert)”.

**Trigger/impact:** A bank-linked credit entry with a known payer account is presented to the consumer as though the value were the recipient account. Integrations using rustdoc to label/export the field can reverse payer/payee semantics. There is also an explicit fallback, so the value is not unconditionally a payer account either.

**Fix direction:** Describe it as the sender's account when known, otherwise the bank account printed on the invoice, quoting the vendor ambiguity. Keep the neutral field name `bank_account` and the exact wire value.

**Regression check:** Documentation review against both official annotations; existing parsing assertions already verify that the string is retained. A new implementation-mirroring unit test would not verify its real-world meaning.

**Limit:** This audit did not create or inspect a live bank-linked entry. The semantic correction rests on explicit first-party annotations of the shared schema.

### QR-04 — P3: header-only comma-decimal totals break the PDF fallback

**Code:** `query_pdf.rs:75–82` uses the shared parser; `envelope.rs:146–155,208–225,285–303` falls back to header text and applies ordinary `Decimal::from_str`. The entire helper was checked: there is no comma-decimal normalization. `RawResponse::header` returns the unchanged header string (`wire.rs:220–229`).

**Evidence:** PRESP allows additional header parameters and makes body totals optional. CRESP defines the net/gross headers as numeric and **not URL encoded**. The recorded P60-E1/E3 observation in `docs/szamlazz-hu-behaviour.md:160` explicitly contains `szlahu_nettovegosszeg 100,01`. That is accepted server behaviour, not a server error being reported by this audit.

**Concrete trigger:** Successful numbered base64 PDF, no `szamlanetto`, header `szlahu_nettovegosszeg: 100,01`. Local result: `ParseError::Invalid { field: "szlahu_nettovegosszeg", message: "Invalid decimal: unknown character" }`. Changing only the header to `100.01` succeeds. A valid body total masks this because body has precedence.

**Impact:** The existing fallback can reject a successfully downloaded PDF solely on auxiliary header formatting. The shared helper also serves other operations, but this finding's tested entry point is `QueryInvoicePdf`.

**Fix direction:** Give numeric headers their own reader accepting the observed single decimal comma as well as a dot, without weakening XML `xs:double` reading or indiscriminately deleting punctuation. Keep malformed/mixed separator values explicit errors.

**Regression check:** Header-only net and gross cases with `100,01`, `100.01`, signed values and zero; body values overriding disagreeing headers; malformed mixed separators rejected. If applying to outstanding as well, establish and document the supported spelling consistently.

**Limit:** The compound condition (comma total **and** absent body equivalent on a PDF query) was not observed live. The comma evidence is from creation; PRESP does not specify separator syntax. Rank is therefore lower than QR-01/02. Do not describe this as an observed live PDF failure.

## Request coverage: every field and setting

Source: XXML/XS and PXML/PS. Both request roots have a flat `sequence`, not `beallitasok`. Both support exactly one selected identifier in the public model.

| In schema order | XML query | PDF query | Code / assessment |
|---|---|---|---|
| `felhasznalo`, `jelszo` | optional strings | optional strings | `xml.rs:251–260`; username followed by password, both directly under root |
| `szamlaagentkulcs` | optional string | optional string | Same helper; exclusive credential form agrees with AUTH |
| `szamlaszam` | optional string | optional string | `query_xml.rs:529–535`, `query_pdf.rs:60–66`; `InvoiceNumber` selector |
| `rendelesSzam` | optional string | optional string | Same matches; **capital S** is correct (queried response's `rendelesszam` is different) |
| `pdf` | optional boolean, always written by crate | not a PDF-query field | `query_xml.rs:536`; `include_pdf` defaults false (`62–70`), explicit true/false valid |
| `valaszVerzio` | not an XML-query field | required int, always `2` | `query_pdf.rs:67`; `ops.rs:27–31`; no missing setting on XML query |
| `szamlaKulsoAzon` | optional string after `pdf` | optional string after `valaszVerzio` | `query_xml.rs:537–539`, `query_pdf.rs:68–70`; correct capitalization and position |

- XML root/action: `xmlszamlaxml`, `http://www.szamlazz.hu/xmlszamlaxml`, `action-szamla_agent_xml` (`query_xml.rs:519–541`), matches XREQ/XS.
- PDF root/action: `xmlszamlapdf`, `http://www.szamlazz.hu/xmlszamlapdf`, `action-szamla_agent_pdf` (`query_pdf.rs:50–73`), matches PREQ/PS.
- `InvoiceSelector` has all three alternatives (`types.rs:1015–1034`). It prevents multiple selector **elements**, not an empty string inside the selected alternative. The schemas have no minimum string length; no new local validation obligation is inferred.
- Default endpoint and file-part framing match SEND: `wire.rs:7–14,66–99`; client POST and content type at `client.rs:262–271`.
- XML declaration/UTF-8, escaping and default namespace are correctly supplied (`xml.rs:19–40,209–220`); `to_wire` rejects XML-1.0-forbidden characters (`wire.rs:387–414`). Schema-location attributes are hints, not declared application fields; omitting the sample's `xmlns:xsi`/`xsi:schemaLocation` is not a gap.
- No query date range, batch selector, `download_copies`, `aggregator`, PDF template or invoice-kind flag exists in either query XSD. The model correctly does not transplant creation/storno settings.
- Last-document-by-order and external-id lookup are server choices. Recorded exact case/whitespace matching and newest external-id holder (`behaviour.md:40–45,63–71`) are not client defects. The writer does not rewrite identifiers.

## Queried `szamla`: complete field inventory

All rows refer to **S**, corroborated by OUT's inline schema. `R` = schema occurrence 1; `O` = 0..1; `*` = repeatable. `?` in the model column means `Option`. Presence relaxation is recorded, not ranked as a defect. Unless another file is named, code ranges below are in `crates/szamlazz-agent/src/ops/query_xml.rs`. Each range includes the wire declaration and its public conversion; public definitions are at `73–144,228–517`.

### Root and parties

| Wire path / schema | Public mapping | Exact implementation |
|---|---|---|
| `szamla/szallito` R; `alap` R; `vevo` R | `supplier`, `info`, `buyer` | `594–609,566–569` |
| `szamla/tetelek` R / `tetel` 1..* | `items: Vec<DocumentItem>`; empty list tolerated | `594–609,891–895,570` |
| `szamla/qutetek` O / `qutet` 0..* | `financial_items`, absent wrapper → empty | `594–603,976–980,571` |
| `szamla/cimkek` O / `cimke` 0..1 | `labels: Vec<String>`; accepts multiple labels too | `600–603,1021–1026,572` |
| `szamla/osszegek` R | `totals` | `604,573`; `xml.rs:341–350,385–391` |
| `szamla/kifizetesek` O / `kifizetes` 1..* | `credit_entries`, absent/empty → empty | `605–606,1028–1032,574–579` |
| `szamla/pdf` O string (prose says base64) | `pdf: Option<Pdf>`; blank → absent, invalid base64 → error | `607–608,580–583` |
| `szallito/id` R int | `supplier.id: ?i64` | `648–679` |
| `szallito/nev` R string | `supplier.name` | `648–679` |
| `szallito/cim` R; `postacim` O, both `cimTipus` | `address: Address`, `postal_address: ?Address` | `648–679` |
| `szallito/adoszam` R; `csoportazonosito`, `adoszameu` O strings | `tax_number?`, `group_id?`, `eu_tax_number?` | `656–676` |
| `szallito/bank` O / `nev`, `bankszamla` O strings | `bank?: Bank { name?, account? }` | `631–646,662–676` |
| Every `cimTipus/orszag` O; `irsz`, `telepules`, `cim` R strings | `Address { country?, zip, city, address }` | `611–629` |
| `vevo/id` O int; `nev` R string; `azonosito` O string | `buyer.id?: i64`, `name`, `identifier?` | `841–888` |
| `vevo/cim` R `cimTipus` | `buyer.address?: Address` (absence tolerated) | `848–849,878` |
| `vevo/postacim` O `cimpostaTipus` | `buyer.postal_address?` | `850–851,879` |
| `vevo/postacim/{nev,orszag,irsz,telepules,cim}` all O strings | `BuyerPostalAddress { name?, country?, zip?, city?, address? }` | `778–802` |
| `vevo/email` O; `adoszam` R; `csoportazonosito`, `adoszameu` O strings | `email?`, `tax_number?`, `group_id?`, `eu_tax_number?` | `852–859,880–883` |
| `vevo/lokacio` R int | `location?: i64`; 1 domestic, 2 EU, 3 outside EU, -1 unknown | `860–861,884`; meaning at `379–381` agrees with OUT/OUTHU |
| `vevo/privatePersonIndicator` R boolean | `private_person: bool`, default false | `862–867,885` |
| `vevo/fokonyv` O | `ledger?: BuyerLedgerInfo` | `868–869,886` |
| `vevo/fokonyv/{vevo,vevoazon}` O strings | `ledger.account?`, `buyer_id?` | `804–839` |
| `vevo/fokonyv/datum` O date; `folyamatostelj` O boolean | `date?`, `continuous_fulfillment?` | `810–813,833–834` |
| `vevo/fokonyv/{elszDatTol,elszDatIg}` O dates | `settlement_from?`, `settlement_to?` | `814–825,835–836`; exact mixed-case names preserved |

Supplier postal address correctly uses `cimTipus`, **not** the buyer's all-optional `cimpostaTipus`. The supplier postal address has no `nev` in S. The two parties' different postal models are justified, not duplication hiding an omitted field.

### Core data (`alap`)

| Wire field / schema | `InvoiceInfo` field | Exact wire / conversion ranges |
|---|---|---|
| `id` R int; `szamlaszam` R string | `id: i64`, `invoice_number: InvoiceNumber` | `684–686,746–747` |
| `gazdEsemAzon` R int | `economic_event_id?: i64` | `687–692,748` |
| `forras` O int; `iktatoszam` O string | `source?: i64`, `registration_number?` | `693–696,749–750` |
| `tipus` R string | `document_type: DocumentType` | `697,751`; open mapping `types.rs:747–847` |
| `eszamla` R int | `appearance: InvoiceAppearance` | `698,752`; integer mapping `146–220` |
| `hivszamlaszam`, `hivdijbekszam` O strings | `referenced_invoice_number?`, `referenced_proforma_number?` | `699–702,753–754,1064–1071` |
| `kelt`, `telj`, `fizh` R dates | `issue_date?`, `fulfillment_date?`, `due_date?` | `703–708,755–757`; QR-01 |
| `fizmod` R string | `payment_method?: PaymentMethod` | `709–710,758` |
| `fizmodunified` R enumerated string | `unified_payment_method?: String` | `711–712,759` |
| `keszpenz` R boolean | `cash_payment`, default false | `713–714,760` |
| `rendelesszam` O string | `order_number?` | `715–716,761` |
| `nyelv` R enumerated string | `language?: String` | `717–718,762` |
| `devizanem` R string | `currency?: Currency` | `719–720,763` |
| `devizabank` O string; `devizaarf` O double | `exchange_bank?`, `exchange_rate?: Decimal` | `721–724,764–765` |
| `megjegyzes`, `afatipus` O strings | `comment?`, `vat_type?` | `725–728,766–767` |
| `penzforg`, `kata`, `katafokonyv` R booleans | `cash_accounting`, `kata`, `kata_ledger`, default false | `729–734,768–770` |
| `email` O string | `email?` (document-associated address) | `735–736,771` |
| `teszt` R boolean; `sztornozott` O boolean | `test?`, `reversed?` | `737–740,772–773` |

Semantics cross-check:

- OUT/OUTHU: `eszamla`: “0: not an invoice, 1: paper invoice, 2: e-invoice, 3: e-invoice”. All four are represented; unknown integer codes retained. P73 (`behaviour.md:97–98`) confirms 1/3. No appearance finding.
- OUT lists `JS` as well as `SZ`, `SS`, `HS`, `ES`, `VS`, `D`, `SL`. `JS` is retained as `DocumentType::Other("JS")`; the open-set contract means a missing named variant does not lose the field or reject it.
- `forras` retains 26/28/34 and other integers. Its presence in the shared schema does **not** contradict XREQ's restriction to internally issued outgoing documents: the parser can cover a superset of what this operation promises to retrieve.
- The 15 language tokens in S (`hu en de it ro sk hr fr es cz pl bg nl ru si`) all survive `String`; no request-side closed `Language` is applied to a response.
- Every `fizmodunifiedTipus` token survives unchanged apart from the common edge trim. `fizmod` is independent free text; `PaymentMethod::Other` retains translated/sample tokens such as `credit_card` and `transfer` (`types.rs:577–677`). `keszpenz` is not derived from either string.
- Optional invoice references retain their nonblank text; the observed external id is never echoed (`behaviour.md:68`), so absence of a response `external_id` is correct.

### Printed items, financial items, totals and credit entries

| Wire path / schema | Public mapping | Exact implementation |
|---|---|---|
| `tetelek/tetel/nev` R string; `azonosito` O string | `DocumentItem.name`, `id?` | `897–901,926–930` |
| `mennyiseg` R double; `mennyisegiegyseg` R string; `nettoegysegar` R double | `quantity: Decimal`, `unit`, `unit_price: Decimal` | `902–906,931–933` |
| `afatipus` O enum string; `afakulcs` R double ≥ 0 | `vat_type?`, `vat_rate_code: String`; category takes precedence in helper | `907–909,934–935,444–451` |
| `netto` R double; `arresafaalap` O double; `afa`, `brutto` R doubles | `net_value`, `margin_vat_base?`, `vat_value`, `gross_value`, all Decimal | `910–917,936–939` |
| `megjegyzes` O string; `sztetordering` R int | `comment?`, `ordering?: i64` | `918–921,940–941` |
| `fokonyv` O / `{arbevetel,afa,gazdasagiesemeny,gazdasagiesemenyafa}` O strings | `ledger?: DocumentItemLedger { revenue_account?, vat_account?, economic_event?, vat_economic_event?, … }` | `922–923,942,947–969` |
| `fokonyv/{elszdattol,elszdatig}` O dates | `ledger.settlement_from?`, `settlement_to?` | `957–960,970–971` |
| `qutetek/qutet/nev` R string | `FinancialItem.name` | `982–984,1004–1007` |
| `qutet/afatipus` O enum string; `afakulcs` R double ≥ 0 | `vat_type?`, `vat_rate_code`; helper reads category first | `985–987,1008–1009,485–491` |
| `qutet/{netto,afa,brutto}` R doubles | `net`, `vat`, `gross`: Decimal | `988–993,1010–1012` |
| `qutet/{elszdattol,elszdatig}` O dates | `settlement_from?`, `settlement_to?` | `994–997,1013–1014` |
| `qutet/afalevon` R int | `deductible_vat: i64` | `998–999,1015`; percentage interpretation is not annotated by the fetched XSD, see uncertainty |
| `qutet/cimkek` O / `cimke` 0..1 string | `labels: Vec<String>` | `1000–1001,1016,1021–1026` |
| `osszegek/afakulcsossz` 1..* / `afatipus` O enum; `afakulcs` R double ≥ 0 | `Totals.by_vat_rate: Vec<VatTotal>`; `vat_type?`, `vat_rate_code` | `xml.rs:341–359,385–403`; `types.rs:1036–1074` |
| `afakulcsossz/{netto,afa,brutto}` R doubles | `VatTotal.{net,vat,gross}: Decimal` | `xml.rs:360–368,399–401` |
| `osszegek/totalossz` R / `{netto,afa,brutto}` R doubles | `Totals.total: GrandTotal { net, vat, gross }` | `xml.rs:348–349,371–383,406–413`; `types.rs:1077–1088` |
| `kifizetesek/kifizetes/datum` R date | `RecordedCreditEntry.date: Date` | `1034–1036,1053`; QR-01 |
| `kifizetes/jogcim` R string | `title: PaymentMethod` | `1037,1054`; unknown/free text preserved |
| `kifizetes/osszeg` R double | `amount: Decimal` | `1038–1039,1055` |
| `kifizetes/megjegyzes`, `bankszamlaszam` O strings | `comment?`, `bank_account?` | `1040–1043,1056–1057`; QR-03 concerns meaning only |
| `kifizetes/banktranzid` O int; `devizaarf` O double | `bank_transaction_id?: i64`, `exchange_rate?: Decimal` | `1044–1047,1058–1059` |

S's VAT category set is `TAM AAM EU EUK MAA F.AFA K.AFA ÁKK TAHK TEHK EUT EUKT HO EUE EUFADE EUFAD37 ATK NAM EAM KBAUK KBAET`. Every category is retained in `vat_type`; `VatRate` additionally names all except `TEHK`, which becomes `Other("TEHK")` (`types.rs:286–338`). All three rate helpers correctly prefer a nonempty category over the numeric zero used alongside it. Numeric rates are kept as wire strings and converted on demand; finite scientific notation `2.7E1` was checked and returns `Percent(27)`. No “scientific notation unsupported” finding is justified with the current dependencies.

The schema has no quantity, unit price, margin base, item identifier or ledger block **inside `qutet`**. Their absence from `FinancialItem` is correct. Financial items are neither substituted for printed items nor summed into totals by the client. Totals are returned as reported; the documented/live independent rounding behaviour does not justify recomputing them (`behaviour.md:159–162`).

## PDF response coverage and parser/error cases

### Every `xmlszamlavalasz` element

Source PRESP/E, in schema order:

| Element | Handling |
|---|---|
| `sikeres` R boolean | Shared `Verdict`, all four XML boolean forms (`xml.rs:115–146,285–312`) |
| `hibakod` O string | Typed `ErrorCode`, unknown preserved, absent/blank code → `Absent` (`xml.rs:119–140`) |
| `hibauzenet` O string | Error message preserved, including CDATA; absent → empty message (`xml.rs:121–139`) |
| `szamlaszam` O string | Result requires a nonblank number from body or `szlahu_szamlaszam` (`envelope.rs:109–121,193–199,244–255`) |
| `szamlanetto`, `szamlabrutto` O doubles | Optional `InvoicePdf` totals, body then corresponding header (`envelope.rs:208–219`; QR-04 for fallback spelling) |
| `kintlevoseg` O double | Shared parser reads it; PDF projection drops it (QR-02) |
| `vevoifiokurl` O string | Shared parser reads it; PDF projection drops it (QR-02) |
| `pdf` O base64Binary | Shared decode, PDF query requires it (`envelope.rs:133–136,227`, `query_pdf.rs:82`) |

### Control flow and edge behaviour

| Case | Current result / assessment |
|---|---|
| Full `szamla` in correct namespace | XML query parses all mapped blocks (`query_xml.rs:547–584`) |
| `xmlszamlavalasz` with false verdict, including code 7 and no headers | XML query returns typed error before asking for invoice fields (`query_xml.rs:549–563,1621–1635`); matches XRESP and live observation `behaviour.md:141–142` |
| Successful `xmlszamlavalasz` sent to XML query | Explicit `UnexpectedBody`, not fabricated `InvoiceDocument` (`query_xml.rs:556–562`) |
| PDF version-2 successful envelope | Shared verdict/payload parsing and required number/PDF (`query_pdf.rs:75–83`, `envelope.rs:167–230,251–264`) |
| Missing/incorrect root or root namespace, HTML, empty body, invalid UTF-8 | Controlled parse error (`xml.rs:63–108`; XML-query tests `1441–1454,1637–1660`). Namespace comparison is exact: the documented URI is `http`, even though transport is HTTPS |
| Namespace prefix on an otherwise equivalent document | Root uses expanded local name/namespace, not prefix spelling (`xml.rs:67–90`); not restricted to the example's default prefix style |
| Boolean `true/false/1/0`, optional blank/absent flags | Supported (`xml.rs:285–312`); invalid nonempty tokens fail; selected required-but-defaulted flags also accept absence/empty as false |
| Missing required structural fields or malformed numeric content | Deserialization fails unless a field/list explicitly has a default; no global full-XSD validation promised |
| Header error / `szlahu_down` | Header error and nonblank maintenance header handled first; URL-decoded message (`wire.rs:245–305,329–336`) |
| Non-2xx with no error header | `HttpStatus` **before** body parsing (`wire.rs:286–303`). Contrary to broad wording at `wire.rs:129–136`, a body-only code on HTTP 500 would not be extracted. No such official/live query response is established, so noted as a boundary rather than ranked as a conformance error |
| Plain `[ERR]` text or raw PDF despite version 2 | Unexpected body (unless an error header already decides); deliberate version-2-only contract (`ops.rs:27–31`, `tests/upstream.rs:516–557`) |
| Whitespace/newlines inside base64 | Compacted and decoded with standard base64 (`types.rs:104–117`) |
| Blank/missing PDF | XML query returns `None`; PDF query returns missing-PDF error. XML query does not make a missing PDF fatal merely because `include_pdf=true` (`query_xml.rs:580–583`) |
| Invalid nonempty PDF | Parse error on ordinary success; the official abbreviated/prose placeholders are not real base64 and correctly fail (`tests/upstream.rs:417–460,559–595`) |
| Error 56 with number | PDF shares the explicit issuance-oriented exception: optional failures are softened, then the PDF is required (`envelope.rs:158–230`). No evidence a read generates notification error 56; do not claim a live PDF-query bug from this hypothetical |
| Foreign namespace on a recognized child | Root validated, descendant namespace not validated by serde. Synthetic `<szamlaszam xmlns="urn:wrong">` is accepted. This is a parser validation boundary, not missing coverage of a valid official response |
| Trailing bytes after a complete document | Synthetic `</szamla><broken` is accepted: root identification stops at the first tag, deserialization reads one value. No claim of full-body well-formedness validation should be made (`xml.rs:89–90`, `query_xml.rs:564`). Not ranked: accepting extra content is outside this audit's valid-response compatibility findings |

## Intentional omissions, leniency and uncertainties

1. **Schema requiredness is not always reality.** S requires fields missing from XRESP's own success example (`gazdEsemAzon`, `keszpenz`, `katafokonyv`, `lokacio`, `privatePersonIndicator`, `sztetordering`). The optional/default mappings are appropriate. Optional `test` does not invent a live-account flag; `reversed` preserves the absence/true distinction seen on original/storno documents (`query_xml.rs:293–316,1590–1619`; `behaviour.md:77–80`).
2. **Lenient list counts and open values are deliberate.** Empty item/credit/subtotal lists, repeated labels despite S's `cimke maxOccurs="1"`, future document/VAT/payment tokens, unknown appearance/location/source codes and wider `i64` integers are not conformance findings. Every S integer is currently `int`; the wider policy is explicit (`query_xml.rs:10–25`, README `238–245`).
3. **Date leniency is narrower in this crate than in Adatkapcsolat.** Optional empty dates become `None`; malformed nonempty dates fail. Multibyte bad date text was locally checked and returned an error, not a panic. Do not transplant the Adatkapcsolat “invalid date → None plus raw XML” contract here. QR-01 is specifically about **valid** XSD dates.
4. **Numeric model is financial Decimal, not all IEEE `double` values.** Finite exponent notation was reproduced successfully for amounts and rates. `NaN`, infinities and values outside Decimal's range are not established invoice amounts; no change to floating-point money is proposed merely because S uses `double`. Exact coverage of the whole double range was not claimed.
5. **String normalization is visible.** Optional strings using `empty_as_none` lose edge whitespace and collapse empty to `None` (`xml.rs:269–283`); required names/addresses and labels use strings directly. This is existing response-helper behaviour, not a newly inferred missing field. Consumers needing original bytes must keep `RawResponse`; `InvoiceDocument` has no raw-XML archive.
6. **No external id or XML outstanding amount is omitted from `InvoiceDocument`.** S has neither element. Live queries never echo `szamlaKulsoAzon` and carry no `kintlevoseg` (`behaviour.md:68,80`). Outstanding is a separate envelope/header concern; the PDF response does declare it, hence QR-02.
7. **No queried waybill, attachment list or erasure-code list is declared by S.** Request-side `fuvarlevel`, creation templates/settings, `torloKod`, buyer telephone/contact comments and tax-status input fields are not automatically response fields. They should not be added to this model based only on the create XSD. `vevo/lokacio` and `privatePersonIndicator` are the declared response fields and are present.
8. **Metadata projection:** `InvoicePdf` also drops the shared parser's `document_id` header and notification-warning flag. Neither is an element of PRESP's schema, and no query notification is documented. They are recorded as auxiliary omissions, not extra field-coverage findings. Header payment method likewise remains transport metadata, not a missing PDF XSD element.
9. **Buyer data is current, not an immutable issuance snapshot.** Live D6 saw later creation update queried `<vevo>`; `alap/email` remains per-document (`behaviour.md:111`). “As recorded on the invoice” in `BuyerInfo` rustdoc (`query_xml.rs:355`) must not be read as a temporal immutability guarantee. No request-vs-query payload equality check belongs in this parser.
10. **Financial-item meaning has limits.** S only comments `qutet` as “pénzügyi tételek” (financial items); `afalevon` is an unannotated `int`. The model correctly carries it, but the claim “Deductible VAT percentage” (`query_xml.rs:479–480`) is not independently established by these fetched pages. Its exact unit/range and the frequency of `qutetek` on internally issued query results remain unverified. No unsupported 0–100 validation should be inferred.
11. **Example drift is not schema drift.** Fresh XRESP's seller text differs from the cached fixture asserted in `tests/upstream.rs:597–602`; fresh PRESP has an abbreviated PDF. The cached fixture checks are useful regression tests, not proof that the current web example is byte-identical or a valid PDF. This audit checked current field declarations independently.
12. **Live boundaries remain explicit:** timezone-suffixed dates, PDF-only optional metadata emission, header-only comma totals on PDF queries, source 26/28/34 query availability, legacy `JS`, and e-invoice appearance `2` were not live-tested. S/OUT document the shapes or tokens; that is not evidence of their observed frequency on this operation.

## Verification record

Executed without networked Agent access:

```text
cargo test --locked --offline -p szamlazz-agent --lib
178 passed; 0 failed

cargo test --locked --offline -p szamlazz-agent --test upstream
9 passed; 0 failed

cargo run --offline --manifest-path /tmp/opencode/query-response-audit-20260909/Cargo.toml
```

The temporary program depends on the local crate with default features (no HTTP client). Its relevant resolved versions match the workspace: Jiff `0.2.35`, rust_decimal `1.43.0`, quick-xml `0.42.0`, serde `1.0.229`. It starts from a synthetic complete `szamla` with plain dates and mutates one feature at a time. Baseline succeeded; timezone `Z` and `+02:00` on issue date and `Z` on credit date failed; finite exponent amounts/rates succeeded; foreign child namespace and trailing broken markup were accepted; invalid multibyte date returned a parse error; PDF balance/URL were lost in JSON projection; dot header amount succeeded and comma header amount failed.

Important existing checks inspected: query selectors/goldens (`query_xml.rs:1087–1115`, `query_pdf.rs:99–125`), all-section model fixture (`query_xml.rs:1275–1391`), JSON round-trip (`1393–1415`), appearance/reversal/test markers (`1456–1619`), body-only code 7 (`1621–1635`), shared verdict table (`xml.rs:501–568`), shared envelope outcome/precedence table (`envelope.rs:343–619`), and official response examples (`tests/upstream.rs:559–703`). The all-section test is broad field coverage, not a test of every XSD lexical spelling or every field's semantics.

The report is the only workspace deliverable. No production change or live follow-up is part of this audit.
