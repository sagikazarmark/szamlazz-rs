# Independent Számla Agent XML/PDF query review

**Date:** 2026-09-11. **Baseline:** `77d53c553c9ecdc86d5fa72ca932c636256ae807`, whole current implementation, not a diff.

## Verdict

**No confirmed ordinary-document interoperability defect or missing current query-schema field.** The XML-query model covers all **125 child-element declarations** in the freshly downloaded `szamla.xsd`: **18 named complex types plus the anonymous root structure** (126 element declarations including `szamla`). The PDF result exposes all six success-payload fields of its operation-specific response schema.

| ID | Classification / priority | Result |
|---|---|---|
| Q-A1 | Source ambiguity · P3 clarification | A schema-valid successful PDF envelope without a reported number is refused. The schema permits this, but success-specific vendor emission/guarantees are unresolved. |
| Q-S1 | Confirmed source conflict · P3 | Hungarian PDF-request XSD is malformed and disagrees with English/download on order and invoice-number optionality. Current writer follows English/download. |
| Q-H1 | Optional identity hardening · P3 | XML query accepts empty/whitespace-only invoice numbers. Schema-valid string, business-invalid identity; no observed vendor occurrence. |
| Q-S2 | Source/example limitations | Inline PDF examples contain invalid placeholder base64; XML example is sparse and arithmetically inconsistent; archived schema contains a removed external-id field. |
| Q-B1 | Intentional representation/projection boundaries | Exact finite Decimal, finite civil-date domain, decoded bytes rather than PDF validation, optional metadata, and typed rather than raw archival. |

Priority describes follow-up urgency, not a demonstrated incident. **No P0/P1/P2 implementation finding is established.** In particular, neither XSD-wide date/double support nor treating every XSD-valid success envelope as a useful fetched document is an existing crate guarantee.

## Scope and independence

- Read the complete production `ops/query_xml.rs` model, private wire structs and conversions, `ops/query_pdf.rs`, and their relevant `ops/envelope.rs`, `xml.rs`, `number.rs`, `types.rs` and `wire.rs` dependencies. References below are baseline line numbers relative to `crates/szamlazz-agent/src/`, unless prefixed otherwise.
- Starting HEAD matched the requested hash. `git diff -- crates/szamlazz-agent` was empty during verification. Existing changes in the Restate worker and its documentation were unrelated user/concurrent work.
- Concurrent work advanced HEAD to `370ff2e5398e9ff4a3aff3b6008ab82e38b9d058` before report completion. `git diff 77d53c553c9ecdc86d5fa72ca932c636256ae807 HEAD -- crates/szamlazz-agent` remained empty: the reviewed/tested Agent source still equals the requested baseline. This review made no commit.
- Fresh unauthenticated GETs retrieved the official documentation, inline tab contents, download schemas and linked historical archive. No `.env`, credentials, live tests, probes or Számla Agent operation POSTs were used. No subdelegation. Only this report was added to the repository by this review; scratch sources/output are under `/tmp/opencode`.
- Read `docs/szamlazz-hu-behaviour.md`, the dated credit-clearing evidence, and current vendor-clarification/backlog research before adjudication. Prior review `2026-09-11-agent-api-61c334f-queries.md` was consulted only after the fresh field and parser matrix; its observations were independently checked rather than adopted as evidence.
- Shared HTTP transport, creation/mutations, receipt and taxpayer implementations have separate review owners. Their helpers were inspected/tested here only where they determine these two query responses.

## Official sources and navigation

All sources below were fetched on the review date. Documentation footer: `v202608271632`; this identifies the site build, not a live-behaviour verification date.

| Ref | Official URL | Material / decisive quotation |
|---|---|---|
| XR | https://docs.szamlazz.hu/agent/querying_xml/request | “only the data of internal outgoing invoices (issued in Számlázz.hu) can be retrieved”; endpoint, POST, multipart `action-szamla_agent_xml`, three alternative selectors |
| XX | https://docs.szamlazz.hu/agent/querying_xml/xml | Request example and complete inline XSD; “the order of the fields is fixed, they cannot be interchanged” |
| XA | https://docs.szamlazz.hu/agent/querying_xml/response | Success: “Full `szamla` XML document”; error: `xmlszamlavalasz` with false/code/message; missing selector → code 7 |
| PR | https://docs.szamlazz.hu/agent/querying_pdf/request | Multipart `action-szamla_agent_pdf`; “invoice number … order number … or external identifier”; last document for repeated order |
| PX | https://docs.szamlazz.hu/agent/querying_pdf/xml | Request example and complete inline XSD; version 2; order before version |
| PA | https://docs.szamlazz.hu/agent/querying_pdf/response | Version 2: “Structured `xmlszamlavalasz` with base64-encoded PDF inside `<pdf>`”; “additional parameters may also arrive” in headers; complete success/error examples and XSD |
| HXR | https://docs.szamlazz.hu/hu/agent/querying_xml/request | Original-language internal-outgoing boundary and three selectors |
| HXX | https://docs.szamlazz.hu/hu/agent/querying_xml/xml | Hungarian example and inline XSD; empty PDF request placeholder versus boolean declaration |
| HXA | https://docs.szamlazz.hu/hu/agent/querying_xml/response | Hungarian successful XML example, code 7, schema navigation |
| HPR | https://docs.szamlazz.hu/hu/agent/querying_pdf/request | “számlaszám … rendelésszám … vagy külső azonosító” |
| HPX | https://docs.szamlazz.hu/hu/agent/querying_pdf/xml | Conflicting/malformed inline XSD, Q-S1 |
| HPA | https://docs.szamlazz.hu/hu/agent/querying_pdf/response | “A `minOccurs="0"` jelölésűek nem mindig jelennek meg”; same optional-number response schema |
| XD | https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd | Download URL from XX's sample `xsi:schemaLocation`; all request fields |
| PD | https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd | Download URL from PX's sample; all request fields |
| OUT | https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak | Shared `szamla` example, annotations and entire inline response schema; supplies SD URL in sample |
| SD | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd | Actual current `szamla` response schema |
| CX | https://docs.szamlazz.hu/agent/generating_invoice/xml | Destination of XA's schema link; actually describes `xmlszamla` creation requests, including waybill/carrier fields |
| CA | https://docs.szamlazz.hu/agent/generating_invoice/response | Shared envelope and header encoding conventions; not evidence every creation header appears on PDF queries |
| ERR | https://docs.szamlazz.hu/agent/basics/error-handling | Linked error guidance and version-1 `[ERR]` format |
| HOME | https://docs.szamlazz.hu/agent/ | Navigation to historical ZIP, explicitly marked last updated 2019 |
| ZIP | https://docs.szamlazz.hu/assets/files/SzamlaAgent-eab03f119308ff908bbf46c7cde5d1bb.zip | Fresh download: `xml/agentxml/{xmlszamlaxml.xml,xmlszamlaxml.xsd}`, `pdf/{xmlszamlapdf.xml,xmlszamlapdf.xsd}`, `xml/szamla/szamla.xsd`, related HTML forms |

**Schema discovery matters:** XA names relative `szamla.xsd` but its clickable link goes to CX's creation schema. Followed the site's Adatkapcsolat navigation (`/penzugyi-adatkapcsolat/` → OUT), whose sample gives SD's absolute URL. An Adatkapcsolat **Ack** schema is not the Agent response schema. The shared document vocabulary does not expand XR's retrieval boundary.

Fresh HTML `<pre>` extraction captured both visible examples and tabbed schemas. Structural comparison, ignoring formatting/comments, found **XX = XD, PX = PD, OUT inline XSD = SD**. This comparison was performed against fresh downloads, not repository fixtures.

| Fresh artifact | SHA-256 of downloaded bytes |
|---|---|
| XD | `06cd34ce07ca8f3c0919cf7c4e6505bbda66ddf6b72d60736c849e695f7e19f3` |
| PD | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` |
| SD | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` |
| ZIP | `b70bc43e7dfd7960b6234e8df6916d7a3aa5e10b841d20ae7d26b84e992e420a` |

Additional HTML hashes and fetched bytes: `/tmp/opencode/queries-77d53c5/manifest.json`. Scratch paths are local reproduction material, not permanent report attachments.

## Findings and adjudication

### Q-A1 — P3 clarification: successful PDF number optionality

**Code:** `ops/query_pdf.rs:83–93` calls `parse_issued`; `ops/envelope.rs:211–217,275–279` refuses success without a nonblank invoice number. `Body::invoice_number` tries body then decoded header (`:120–132`), trimming both (`:326–329`). `InvoicePdf::invoice_number` is nonoptional (`query_pdf.rs:39–41`).

**Source:** PA/HPA declares `szamlaszam` as `string minOccurs="0"`, and says optional elements “may not always be included”; headers “may also arrive.” Only `sikeres` is mandatory. The same schema covers failures, so this does **not** prove successful PDF queries can omit the number.

**Reproducer:** HTTP 200, no headers, the complete envelope below, with `BASE64_OF_A_PDF` replaced by a standard-base64 document:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
  <pdf>BASE64_OF_A_PDF</pdf>
</xmlszamlavalasz>
```

Scratch `numberless-pdf.xml` uses a constructed one-page PDF with objects/xref/trailer, not merely `%PDF-`. libxml2 validates the envelope against the freshly extracted PA schema. `QueryInvoicePdf::parse` returns **`Parse(Missing("szamlaszam"))`**. Adding a body number gives the established success path. `%PDF-` controls also isolate the identity requirement independently of document rendering. No rendering/signature assertion is made.

**Impact if permitted by the vendor:** caller loses an otherwise downloadable artifact; order/external-id queries cannot infer the returned invoice's number from their request. Read failure does not mean the document is absent. **Not a confirmed emitted-payload failure:** all inspected examples and available live records are numbered. `docs/research/2026-09-11-agent-vendor-clarification.md:52–60` already asks the exact success-specific question and explicitly has no vendor answer (`:3`). Credit-clearing observations establish no PDF guarantee.

**Recommendation:** obtain the success-specific guarantee. If omission is legitimate, represent an honestly unnumbered fetched artifact rather than substituting a requested number as a reported echo. If guaranteed, document that semantic requirement separately from the shared XSD. Missing PDF itself remains a reasonable failure for an operation whose purpose is fetching a PDF.

### Q-S1 — P3 source conflict: Hungarian PDF request schema

**Code:** `ops/query_pdf.rs:62–79` writes credentials → selected invoice/order number → `valaszVerzio=2` → selected external id.

**Fresh source conflict:**

- PX/PD: `szamlaszam? → rendelesSzam? → valaszVerzio → szamlaKulsoAzon?`.
- HPX: `szamlaszam` mandatory, then `valaszVerzio → rendelesSzam? → szamlaKulsoAzon?`.
- HPX contains `targetNamespace="…xmlszamlapdf"xmlns:tns=…` without separating whitespace. Parsing the untouched extracted schema fails at **line 1, column 140**.
- PR/HPR expressly support order/external-id alternatives. The freshly retrieved historical ZIP's `pdf/xmlszamlapdf.xsd` also agrees with PX/PD, not HPX.

**Impact/reproducer:** validate a generated order-only request against HPX: the schema cannot load. Repair only the missing attribute separator and it still demands a number/another order. By contrast all 12 generated XML/PDF × selector × authentication variants validated against the current downloads.

**Recommendation:** vendor reconciliation; retain current writer. Existing external-id PDF observations (`docs/szamlazz-hu-behaviour.md:83–85`) support the capability but are not a new live ordering check. No implementation defect established.

### Q-H1 — P3 optional hardening: blank XML-query number

**Code:** `ops/query_xml.rs:714–715,775–776` directly reads/projects `InvoiceNumber`; `types.rs:22–32` is an unvalidated transparent string. Unlike PDF parsing, it checks presence but not nonblank content.

**Source:** SD requires `alap/szamlaszam` as `xs:string`, with **no `minLength` restriction**; OUT calls it the invoice's unique number. Blank is schema-valid but not a meaningful numbered invoice identity.

**Fresh reproduction:** in the all-fields SD-derived document replace `alap/szamlaszam` with empty text or `" \t\n "`. Both return `Ok(InvoiceDocument)` preserving the blank string. A nonblank padded `" I-1 "` is preserved too.

**Impact:** a caller equating parse success with usable invoice-number identity can store an empty key or make a subsequent unusable by-number request. `alap/id` remains present, so this is not total identity loss. No official example/live record supplies a blank invoice number; no valid business-payload failure or duplicate issuance is demonstrated.

**Recommendation:** optionally require a non-XML-whitespace character in this response field, preserving nonblank source text. Keep this separate from the worker's bounded invoice-number alphabet and the general wire wrapper. Alternatively state explicitly that parse success guarantees field presence/type, not nonblank identity.

### Q-S2 — Source artifacts must not become parser defects

1. XA/HXA `<pdf>` is English prose (“The receipt .pdf can be found here in BASE64 encoding”). PA/HPA base64 contains `....`. Fresh verbatim examples fail `Pdf::from_base64` with invalid `.` at offsets 10 and 456 respectively. Replacing only the placeholder with `JVBERi0=` makes both parse. This proves XML/base64 integration, **not** that the placeholder is valid PDF or that removing dots reconstructs one.
2. XA omits SD-required `gazdEsemAzon`, `keszpenz`, `katafokonyv`, buyer `lokacio`/`privatePersonIndicator`, item `sztetordering`; `fizmodunified=other` is outside the Hungarian XSD enumeration. The line is `380/76/456`, totals `464/93/557`. The parser correctly permits sparse data/open tokens and preserves reported amounts without recomputation.
3. HXX's example has empty `<pdf>` though its type is boolean; its comment instructs true/false. Current writer uses valid explicit booleans.
4. PA allows omitted `valaszVerzio` as legacy mode while PX/PD requires it. Always sending 2 is compatible with the crate's advertised structured path.
5. OUT sample annotations label `eszamla` “string” and `sztetordering` “double”, while current SD types both `int`. Signed integer storage is appropriate; appearance codes 0/1/2/3 are supported by annotation and the P73 observations.
6. Fresh historical ZIP `xml/szamla/szamla.xsd:118` contains optional `alap/szamlaKulsoAzon`. Current SD/OUT has removed it and added newer fields. `docs/szamlazz-hu-behaviour.md:87` records no echoed external id. Its omission from the current model is **not** a missing current documented field. A reader requiring historical raw preservation needs a different archival contract.

### Q-B1 — Explicit domain and projection boundaries

- **Dates:** `xml.rs:667–727` retains the printed civil date, not an instant. Every one of the 11 positions accepts modern dates, leap day, `Z`, offsets through ±14:00, and padding. Optional missing/empty dates are `None`; invalid nonblank dates fail the whole Agent query. This intentionally differs from Adatkapcsolat's content-tolerant date handling. Checked slicing prevents multibyte panic. Current README `:264–268` explicitly retains the finite legacy date domain. Fresh controls accept `2024-02-29T12:00:00` as a date (legacy Jiff interpretation), reject `-0001-02-28`/`10000-01-01`, junk and multibyte nonsense. The first two refusals are representational limits, not claimed malformed XML or modern invoice defects. Year zero/compact dates are intentional leniency, not strict XSD validation.
- **Decimals:** `number.rs:61–141`, `xml.rs:647–665`, `envelope.rs:344–370` accept exact representable signed finite decimal/exponent spellings, including normalized long trailing-zero mantissas. Nonfinite double values and values outside Decimal's magnitude/precision are refused, not silently rounded. This is explicitly documented in README `:465` and tested in `tests/numeric_fidelity.rs:112–185`. XSD `double` is broader than a financial Decimal; `1e-29`/`1e100` refusals are known limits, not newly discovered lexical bugs.
- **Integers:** all ten expanded integer positions fit signed `i64`; every current declaration is `xs:int`. Signed 32-bit extrema and positive signs parsed in the fresh matrix. Wider response width is deliberate (`query_xml.rs:10–25`); `InvoiceAppearance::Unknown` keeps future codes. `afalevon` has no documented percentage/unit constraint; no invented interpretation.
- **PDF bytes:** `types.rs:96–117` decodes standard base64 after whitespace removal. It does not inspect `%PDF`, validate document structure/signatures or render content. A fresh control encoding `not a pdf` succeeds as bytes. Unicode whitespace is stripped more broadly than XSD XML whitespace. These are optional artifact/lexical hardening choices, not failures of valid base64 PDF. An invalid nonblank XML-query PDF fails the query; absence/blank remains `None` even when requested (`query_xml.rs:609–612`).
- **Projections:** `InvoicePdf` omits the shared envelope's auxiliary `document_id`, `payment_method` and notification flag. None is a missing PA success XML element. `InvoiceDocument` carries neither raw XML nor unknown fields, and no response `fuvarlevel` is declared by SD. CX's waybill/carrier structures, creation settings and email fields must not be transplanted into a guessed query response. All actual SD fields are accounted for below.
- **Source text:** optional business strings preserve decoded nonblank characters, including padding/NBSP, while XML-whitespace-only content becomes `None` (`xml.rs:745–757`). Required strings and label entries retain their text. These are typed readings, not byte-preserving archives. Unknown document/payment/VAT tokens remain open; language/currency/unified-payment text is not rejected because a vocabulary list is stale.

## Complete request audit

Order below is schema order. `O` = optional 0..1, `R` = required 1..1. The XSD makes credential/selector elements optional individually; useful authentication and selection are semantic requirements.

| Surface | XML query | PDF query | Current code/result |
|---|---|---|---|
| Action | `action-szamla_agent_xml` | `action-szamla_agent_pdf` | `query_xml.rs:540–548`; `query_pdf.rs:58–65`; exact XR/PR match |
| Root / namespace | `xmlszamlaxml` / `http://www.szamlazz.hu/xmlszamlaxml` | `xmlszamlapdf` / `http://www.szamlazz.hu/xmlszamlapdf` | Exact match |
| `felhasznalo`, `jelszo` | O string each | O string each | Username/password under root in order, no settings wrapper; `xml.rs:628–637` |
| `szamlaagentkulcs` | O string | O string | Alternative credential branch under root |
| `szamlaszam` | O string | O string per PX/PD | InvoiceNumber selector; Q-S1 for HPX |
| `rendelesSzam` | O string | O string before version | Order selector, exact case; response uses different spelling `rendelesszam` |
| `pdf` | O boolean | Not a field | Always emits `include_pdf`, constructor false; `query_xml.rs:65–73,557` |
| `valaszVerzio` | Not a field | R int | Shared `RESPONSE_VERSION`, always 2; `query_pdf.rs:75` |
| `szamlaKulsoAzon` | O string after PDF flag | O string after version | External-id selector only; `query_xml.rs:558–560`, `query_pdf.rs:76–78` |
| Escaping / invalid characters | Shared writer | Shared writer | `xml.rs:586–596`, `wire.rs:405–409`; `<`/`&` in all selector/auth variants validated offline |
| Endpoint / file | HTTPS POST, one multipart XML file | Same | `wire.rs:14,66–100`; schema-location hint need not be emitted |

`InvoiceSelector` (`types.rs:1035–1054`) prevents multiple/no selector **variants**, but empty strings remain constructible. Local empty-selector rejection would be convenience validation, not a missing field. No batch query or combined-selector precedence is documented. Order/external-id queries return one matching document; they do not prove ownership or uniqueness. Preserve exact query input: recorded padded order queries return 7 (`behaviour.md:61`); create-time trimming does not imply query-time trimming.

## Complete XML document field audit

All SD declarations, including reused structures, are enumerated here. `s` = string; `i` = int; `d` = double; `b` = boolean; `date` = date. `?` in a Rust mapping means `Option`. Named vocabulary restrictions remain open at the Rust response boundary. Containers have maximum 1 unless a list is specified.

### Root and reusable structures

Code: `query_xml.rs:80–147,328–343,595–613,623–707,807–830`.

| Wire path / type | SD | Rust reading |
|---|---|---|
| `szamla/szallito` | R container | `supplier: Supplier` |
| `szamla/alap` | R container | `info: InvoiceInfo` |
| `szamla/vevo` | R container | `buyer: BuyerInfo` |
| `szamla/tetelek` | R container | `items`; wrapper required |
| `szamla/qutetek` | O container | `financial_items`; absent → `[]` |
| `szamla/cimkek` | O container | `labels`; absent → `[]` |
| `szamla/osszegek` | R container | `totals: Totals` |
| `szamla/kifizetesek` | O container | `credit_entries`; absent → `[]` |
| `szamla/pdf` | O s | `pdf: ?Pdf`, nonblank base64 decoded |
| `cimTipus/orszag` | O s | `Address.country: ?String` |
| `cimTipus/{irsz,telepules,cim}` | R s each | `Address.{zip,city,address}: String` |
| `cimpostaTipus/{nev,orszag,irsz,telepules,cim}` | O s each | `BuyerPostalAddress.{name,country,zip,city,address}: ?String` |
| `bankTipus/{nev,bankszamla}` | O s each | `Bank.{name,account}: ?String` |

Seller postal address uses **`cimTipus`**, hence still requires ZIP/city/address if present. Only buyer postal address is the all-optional `cimpostaTipus`; treating these as identical would introduce a false optionality finding.

### Seller: 8 declarations

Code: `query_xml.rs:129–147,677–707`.

| `szallito/…` | SD | `Supplier` field |
|---|---|---|
| `id` | R i | `id: ?i64`; intentionally relaxed absence/empty |
| `nev` | R s | `name: String` |
| `cim` | R `cimTipus` | `address: Address` |
| `postacim` | O `cimTipus` | `postal_address: ?Address` |
| `adoszam` | R s | `tax_number: ?String`; relaxed absence/empty |
| `csoportazonosito` | O s | `group_id: ?String` |
| `adoszameu` | O s | `eu_tax_number: ?String` |
| `bank` | O `bankTipus` | `bank: ?Bank` |

No account-identity stability guarantee inferred from seller id; see behaviour record `:164`.

### Core invoice data: 28 declarations

Code: `query_xml.rs:149–325,713–804`.

| `alap/…` | SD | `InvoiceInfo` field / reading |
|---|---|---|
| `id` | R i | `id: i64` |
| `szamlaszam` | R s | `invoice_number: InvoiceNumber`; Q-H1 |
| `gazdEsemAzon` | R i | `economic_event_id: ?i64`; relaxed absence |
| `forras` | O i | `source: ?i64`; shared-schema field, not expanded retrieval |
| `iktatoszam` | O s | `registration_number: ?String` |
| `tipus` | R s | `document_type: DocumentType`, open token |
| `eszamla` | R i | `appearance: InvoiceAppearance`: 0 not invoice, 1 paper, 2/3 electronic, else Unknown |
| `hivszamlaszam` | O s | `referenced_invoice_number: ?InvoiceNumber` |
| `hivdijbekszam` | O s | `referenced_proforma_number: ?InvoiceNumber` |
| `kelt` | R date | `issue_date: ?Date`; relaxed absence/empty |
| `telj` | R date | `fulfillment_date: ?Date`; relaxed absence/empty |
| `fizh` | R date | `due_date: ?Date`; relaxed absence/empty |
| `fizmod` | R s | `payment_method: ?PaymentMethod`; arbitrary tokens retained |
| `fizmodunified` | R vocabulary | `unified_payment_method: ?String` |
| `keszpenz` | R b | `cash_payment: ?bool`; missing is not false |
| `rendelesszam` | O s | `order_number: ?String` |
| `nyelv` | R vocabulary | `language: ?String` |
| `devizanem` | R s | `currency: ?Currency` |
| `devizabank` | O s | `exchange_bank: ?String` |
| `devizaarf` | O d | `exchange_rate: ?Decimal` |
| `megjegyzes` | O s | `comment: ?String` |
| `afatipus` | O s | `vat_type: ?String` |
| `penzforg` | R b | `cash_accounting: ?bool` |
| `kata` | R b | `kata: ?bool` |
| `katafokonyv` | R b | `kata_ledger: ?bool` |
| `email` | O s | `email: ?String`, document-associated address |
| `teszt` | R b | `test: ?bool`; no invented live-account default |
| `sztornozott` | O b | `reversed: ?bool`; absent/false/true distinct |

### Buyer and buyer ledger: 12 + 6 declarations

Code: `query_xml.rs:345–403,833–918`.

| `vevo/…` | SD | `BuyerInfo` field |
|---|---|---|
| `id` | O i | `id: ?i64` |
| `nev` | R s | `name: String` |
| `azonosito` | O s | `identifier: ?String`, distinct from numeric id |
| `cim` | R `cimTipus` | `address: ?Address`; wrapper relaxed |
| `postacim` | O `cimpostaTipus` | `postal_address: ?BuyerPostalAddress` |
| `email` | O s | `email: ?String` |
| `adoszam` | R s | `tax_number: ?String`; relaxed |
| `csoportazonosito` | O s | `group_id: ?String` |
| `adoszameu` | O s | `eu_tax_number: ?String` |
| `lokacio` | R i | `location: ?i64`; no closed-enum rejection |
| `privatePersonIndicator` | R b | `private_person: ?bool`; relaxed |
| `fokonyv` | O container | `ledger: ?BuyerLedgerInfo` |
| `fokonyv/vevo` | O s | `account: ?String` |
| `fokonyv/vevoazon` | O s | `buyer_id: ?String` |
| `fokonyv/datum` | O date | `date: ?Date` |
| `fokonyv/folyamatostelj` | O b | `continuous_fulfillment: ?bool` |
| `fokonyv/elszDatTol` | O date | `settlement_from: ?Date` |
| `fokonyv/elszDatIg` | O date | `settlement_to: ?Date` |

The mixed-case buyer settlement element names are explicitly renamed. Buyer data may reflect later partner-master changes; current rustdoc `:364–369` and behaviour record `:130` correctly avoid immutable-at-issuance semantics.

### Printed items and their ledger: 14 + 6 declarations, plus the item list

Code: `query_xml.rs:405–467,920–1003`.

| `tetelek/tetel/…` | SD | `DocumentItem` field |
|---|---|---|
| `nev` | R s | `name: String` |
| `azonosito` | O s | `id: ?String` |
| `mennyiseg` | R d | `quantity: Decimal` |
| `mennyisegiegyseg` | R s | `unit: String` |
| `nettoegysegar` | R d | `unit_price: Decimal` |
| `afatipus` | O vocabulary | `vat_type: ?String` |
| `afakulcs` | R d restricted ≥0 | `vat_rate_code: String`, raw token; helper interprets it |
| `netto` | R d | `net_value: Decimal` |
| `arresafaalap` | O d | `margin_vat_base: ?Decimal` |
| `afa` | R d | `vat_value: Decimal` |
| `brutto` | R d | `gross_value: Decimal` |
| `megjegyzes` | O s | `comment: ?String` |
| `sztetordering` | R i | `ordering: ?i64`; relaxed absence/empty |
| `fokonyv` | O container | `ledger: ?DocumentItemLedger` |
| `fokonyv/arbevetel` | O s | `revenue_account: ?String` |
| `fokonyv/afa` | O s | `vat_account: ?String`, not an amount |
| `fokonyv/gazdasagiesemeny` | O s | `economic_event: ?String` |
| `fokonyv/gazdasagiesemenyafa` | O s | `vat_economic_event: ?String` |
| `fokonyv/elszdattol` | O date | `settlement_from: ?Date` |
| `fokonyv/elszdatig` | O date | `settlement_to: ?Date` |

The **14 item fields plus `tetelek/tetel` list declaration** account for 15 declarations; `tetel` is 1..unbounded in SD, but a present empty wrapper becomes `[]`. Item-ledger dates use lowercase spelling. `vat_rate()` prefers a nonblank special type to the numeric token, preserving unknown tokens and reading padded/scientific numeric rates correctly. No totals are recalculated.

### Financial items and labels

Code: `query_xml.rs:470–510,1005–1055`.

| Wire field | SD | `FinancialItem` / list reading |
|---|---|---|
| `qutetek/qutet` | 0..unbounded | `financial_items: Vec<FinancialItem>` |
| `qutet/nev` | R s | `name: String` |
| `qutet/afatipus` | O vocabulary | `vat_type: ?String` |
| `qutet/afakulcs` | R d restricted ≥0 | `vat_rate_code: String`; same special-type precedence |
| `qutet/netto` | R d | `net: Decimal` |
| `qutet/afa` | R d | `vat: Decimal` |
| `qutet/brutto` | R d | `gross: Decimal` |
| `qutet/elszdattol` | O date | `settlement_from: ?Date` |
| `qutet/elszdatig` | O date | `settlement_to: ?Date` |
| `qutet/afalevon` | R i | `deductible_vat: i64`; uninterpreted value |
| `qutet/cimkek` | O container | `labels: Vec<String>` |
| `cimkek/cimke` (shared type) | 0..1 s | `Vec<String>` at invoice and financial-item level; accepts more labels than schema |

The label-list widening is leniency, not a loss of a schema-supported value. Financial items are not printed price/quantity lines and carry no such fields in SD.

### Totals and credit entries

Code: `xml.rs:811–894`, `types.rs:1056–1108`, `query_xml.rs:512–538,1057–1092`.

| Wire field | SD | Rust reading |
|---|---|---|
| `osszegek/afakulcsossz` | 1..unbounded | `Totals.by_vat_rate: Vec<VatTotal>`; absent → `[]` |
| `afakulcsossz/afatipus` | O vocabulary | `vat_type: ?String` |
| `afakulcsossz/afakulcs` | R d restricted ≥0 | `vat_rate_code: String`; special-type precedence helper |
| `afakulcsossz/{netto,afa,brutto}` | R d each | `VatTotal.{net,vat,gross}: Decimal` |
| `osszegek/totalossz` | R container | `Totals.total: GrandTotal` |
| `totalossz/{netto,afa,brutto}` | R d each | `GrandTotal.{net,vat,gross}: Decimal` |
| `kifizetesek/kifizetes` | 1..unbounded | `Vec<RecordedCreditEntry>`; present empty wrapper accepted |
| `kifizetes/datum` | R date | `date: Date`; missing/blank refused |
| `kifizetes/jogcim` | R s | `title: PaymentMethod`; open token |
| `kifizetes/osszeg` | R d | `amount: Decimal` |
| `kifizetes/megjegyzes` | O s | `comment: ?String` |
| `kifizetes/bankszamlaszam` | O s | `bank_account: ?String` |
| `kifizetes/banktranzid` | O i | `bank_transaction_id: ?i64` |
| `kifizetes/devizaarf` | O d | `exchange_rate: ?Decimal` |

OUT's bank-account comment allows either the sender's account or the invoice's account when sender is unknown; rustdoc `:530–532` preserves that ambiguity. Credit-entry order is not submission order (`behaviour.md:154`); reversal can remove entries (`:99`). SD has no outstanding amount; deriving it from gross minus entries would not restore a missing documented query field.

**Coverage count:** root 9; address 4; buyer-postal 5; bank 2; seller 8; core 28; buyer-ledger 6; buyer 12; item-ledger 6; item 14; item-list 1; VAT subtotal 5; grand total 3; totals container 2; credit entry 7; credit-list 1; labels 1; financial-list 1; financial item 10 = **125**.

## PDF response and error interaction audit

Code: `query_pdf.rs:36–95`, `envelope.rs:100–248,275–315,344–370`, `xml.rs:461–553`, `wire.rs:251–311`.

| PA envelope field | XSD | Current reading / outcome |
|---|---|---|
| `sikeres` | R b | Required scalar, `true/1/false/0`; missing/empty/invalid is parse failure |
| `hibakod` | O s | On false: typed known code, `Unknown` for future token, `Absent` for missing/blank |
| `hibauzenet` | O s | Decoded diagnostic; absent/unusable diagnostic does not erase readable refusal |
| `szamlaszam` | O s | Nonblank body before decoded header; required for `InvoicePdf`, Q-A1 |
| `szamlanetto` | O d | `net_total: ?Decimal`, body before raw numeric header |
| `szamlabrutto` | O d | `gross_total: ?Decimal`, body before raw numeric header |
| `kintlevoseg` | O d | `outstanding: ?Decimal`; absent is not zero |
| `vevoifiokurl` | O s | `customer_account_url: ?String`; XML entity decoding only, header decoded once as fallback |
| `pdf` | O base64Binary | Mandatory artifact for successful `InvoicePdf`; empty/missing → `Missing("pdf")`, nonblank malformed → base64 parse error |

- XML query checks headers, then allows exactly `szamla` or error `xmlszamlavalasz` under their own namespaces (`query_xml.rs:568–593`). A **successful** envelope is not a full XML query answer and is refused. Body-only code 7 works for both operations; it includes consumed/deleted proformas, not just never-existing numbers (`behaviour.md:90,161–162`).
- Shared precedence is **nonblank `szlahu_down` → nonblank error header → known non-2xx status → body**. Headerless XML error at HTTP 200 is `Api`; at HTTP 500 it is `HttpStatus` before body interpretation. This is the explicit library policy, not a newly inferred vendor guarantee. The query-specific review does not override the shared transport contract.
- Within a well-formed error envelope, refusal is read before optional payload conversion: fresh false/7 with `<pdf>bad!</pdf>` returns code 7, not a base64 error. Future code and absent code remain honest unknown/absent errors. Invalid required verdict never establishes success merely because a body number exists.
- PDF parsing inherits the numbered-56 issuance exception: false/56 plus number and decodable PDF returns `Ok(InvoicePdf)`; the notification flag is dropped by its projection. Freshly reproduced. PA documents no query notification action or code-56 query emission. This is a shared-parser policy/exposure question, **not an established valid PDF-query refusal swallowed in production**. XML query does not make the same exception. If query-specific 56 semantics matter, seek vendor evidence rather than assuming creation behavior occurs during retrieval.
- Ordinary success reads optional amounts strictly; malformed nonblank body amount does not fall back to header. Absent/blank body values can use headers. XML decimal point versus raw header comma/dot/exponent grammar is deliberate; textual headers are decoded once, numeric signs are not URL-decoded away.
- Root namespace checks support equivalent prefix aliases and reject wrong/unbound roots. Foreign subtrees do not supply invoice identity/reversal or verdicts. Complete XML is checked through EOF; duplicate singleton fields, nested scalar content, multiple roots, truncation and malformed trailing syntax fail. Unknown well-formed children are ignored; repeated protocol rows retain order across aliases/extensions. This is parsing, not full XSD validation.
- Version 1 raw PDF / `[ERR]…` is deliberately neither selectable nor decoded by these typed query operations. Always requesting version 2 supplies the same download capability without a second response-mode contract. Creation's `xmlagentresponse=DONE;…` is not a query success format.

## Verification and evidence limits

### Locked repository checks

```sh
cargo test -p szamlazz-agent --locked --offline --lib ops::query -- --nocapture
cargo test -p szamlazz-agent --locked --offline --test response_booleans --test response_namespaces --test response_headers --test numeric_fidelity --test business_text --test response_completion
```

Results: **29 query unit tests passed**, then **40 integration tests passed** (2 business-text, 6 numeric, 3 boolean, 4 completion, 14 header, 11 namespace). These selected suites include shared-helper controls; no live/probe test target or ignored test was executed.

### Independent fresh-schema matrix

Scripts and throwaway Rust public-API executable:

- `/tmp/opencode/queries-77d53c5-acquire.py`
- `/tmp/opencode/queries-77d53c5-check/{Cargo.toml,src/main.rs}`
- `/tmp/opencode/queries-77d53c5-matrix.py`
- `/tmp/opencode/queries-77d53c5-evidence.py`
- `/tmp/opencode/queries-77d53c5-edges.py`

Built offline with a path dependency to the unchanged baseline crate. Scratch Cargo resolved its own locally cached dependency lock; the locked repository tests above remain the authoritative workspace-lock check. Generated one synthetic record from **every field of fresh SD**, 135 expanded element nodes including reused address structures/root, then tested:

- All **65 optional element occurrences** independently omitted, including complete optional containers: all parsed.
- All boolean positions with four valid spellings/padding; all 11 date positions with leap-day/timezones/padding; all 10 integer positions with signed extrema/positive sign/padding; all Decimal positions with exponent, signs, exact normalization; all three VAT token positions with numeric variants.
- Whole-document namespace prefix aliasing and wrapped base64.
- 12 writer variants: two operations × three selectors × key/password authentication, with literal `<`/`&` in supplied text; all validated against fresh XD/PD.
- Fresh official success examples verbatim and with base64 placeholders replaced, and Q-A1's numberless success.

**299 matrix rows: 296 successful checks; three expected/qualified refusals:** two invalid example artifacts and Q-A1. The 296 include 12 request-schema checks, with the remainder being response parses. Of 274 rows positively validated by libxml2 against the freshly downloaded/extracted XSDs, only Q-A1 was refused; its semantic ambiguity is stated above. Four example rows were not XSD-validated. **21 padded integer/date rows were accepted by Rust but reported invalid by the available libxml2 validator**; these are recorded as validator observations, not an independent proof of invalid XSD lexical forms or a crate defect. They are excluded from the 274 positive-validation count. No silent schema repair was used.

`python`, `lxml`, `xmllint`, and `pdfinfo` were unavailable. Acquisition/matrix ran with `python3`; actual schema validation used installed **libxml2 via ctypes**, not ElementTree masquerading as XSD validation. ElementTree was used for extraction/generation/structural comparison. PDF syntax/rendering was not independently validated by a PDF tool. Edge controls separately reconfirmed blank XML number, numberless/empty PDF, false/7 with bad artifact, future/absent code, numbered-56 behavior, byte-only PDF decoding, invalid date rejection and accepted legacy date forms.

### Recorded live facts used, not re-executed

`docs/szamlazz-hu-behaviour.md`: exact/case-sensitive order queries `:59–64`; newest external-id XML/PDF holder and no echo `:82–90`; reversal marker/credit removal `:96–99`; fulfillment date and paper/electronic appearance `:115–117`; mutable buyer data `:130`; credit order `:154`; body-only errors `:161–162`. Historical raw logs are not in this repository (`:11–12`), so no captured full live query corpus is claimed.

`docs/research/2026-09-11-credit-clearing-live.md:14–32,64–74` records successful pre/post XML queries and emptied credit-entry lists, with parsed acknowledgement numbers but no raw HTTP channel capture. It cannot prove PDF success optionality. Current vendor clarification `:3,52–60` remains unsent/unanswered. These evidence boundaries are why Q-A1/Q-H1 and historical-schema differences are not promoted into confirmed normal-payload defects.

## Recommended disposition

1. Keep query writers and the current full document mapping: fresh source/field validation supports them.
2. Resolve Q-A1 and Q-S1 with vendor documentation/clarification; no query-specific live experiment was performed here.
3. Consider Q-H1 as a small explicit identity contract decision, independently of schema conformance.
4. Preserve the distinctions between current source conflict, malformed illustrative artifacts, broader XSD value spaces and verified modern invoice behavior when aggregating this specialist report.
