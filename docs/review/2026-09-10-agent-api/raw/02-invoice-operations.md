# Invoice operations and shared reply envelope — current-source review

**Reviewed:** 2026-09-10. **Code baseline inspected:** `f54dac78cd1f7f981cd70d2d29ee3376be5b9bd3` plus the working tree. The coordinator README names an earlier baseline; the locations and closure decisions below refer to the actual current files inspected here.

**Owned scope:** `crates/szamlazz-agent/src/ops/{storno,credit_entry,proforma,query_pdf,envelope}.rs`. Supporting reads of `xml.rs`, `types.rs`, `wire.rs` interfaces, `error.rs`, `recovery.md`, relevant tests, `docs/szamlazz-hu-behaviour.md` and `fixtures/SOURCES.md` establish the behavior of these operations. HTTP transport, invoice-create requests, query-XML document projection and receipts remain separately owned. No delegation, live Számla Agent calls, production/test edits, or edits to existing reports.

## Result

**Three actionable findings:** one P2 semantic-documentation gap, one P3 response-parser gap and one P3 semantic-documentation/source-translation correction. **No P0/P1 or missing request field established.** The historical comma-header and optional-response-capability findings are fixed.

| ID | Severity | Classification | Finding |
|---|---|---|---|
| IO-01 | **P2 / medium** | Semantic guidance gap | Order-number proforma deletion is documented upstream as deleting **all** matching proformas; the Rust surface describes a singular target. |
| IO-02 | **P3 / low** | Implementation gap, unexpected-response handling | Correct-root envelopes accept foreign-namespace verdict/payload children as protocol fields. Reproduced through public parsers. |
| IO-03 | **P3 / low** | Semantic guidance gap caused by source translation conflict | `issuer_tax_number` describes matching an incoming invoice to an incoming receipt. Current Hungarian docs and even the English inline XSD describe matching an incoming payment to the invoice. |

P2/P3 are review priorities, not measured incident rates. IO-01's batch effect is a current vendor statement, not a fresh live observation. IO-02 is an offline counterexample, not evidence that the vendor normally emits foreign-namespace fields. IO-03 is misleading domain guidance, not a serialization defect.

## Current primary-source register

All URLs in this register were fetched during this review using public GETs. The pages display **`v202608271632`**; this is a site build, not a last-updated date for every assertion. `/xml` pages contain both the example and inline XSD tabs; no cached July schema was substituted for a current fetch.

| Ref | Exact URLs | Evidence used |
|---|---|---|
| S-R | https://docs.szamlazz.hu/agent/reversing_invoice/request · https://docs.szamlazz.hu/hu/agent/reversing_invoice/request | Multipart action, required original number, conflicting external-ID prose. “the invoice number (`szamlaszam`) … is required”. |
| S-X | https://docs.szamlazz.hu/agent/reversing_invoice/xml · https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml | Example and complete request sequence/cardinalities. “the order of the fields is fixed”; buyer-tax-number annotation; deprecated copy count. |
| S-O | https://docs.szamlazz.hu/agent/reversing_invoice/response · https://docs.szamlazz.hu/hu/agent/reversing_invoice/response | Version 1/2, headers, success/error examples, response XSD. “Structured `xmlszamlavalasz` with optional base64 PDF”. |
| S-D | https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd | Current downloadable storno request XSD; structural sequence agrees with S-X. |
| C-R | https://docs.szamlazz.hu/agent/credit_entry/request · https://docs.szamlazz.hu/hu/agent/credit_entry/request | `action-szamla_agent_kifiz`; POST multipart XML. |
| C-X | https://docs.szamlazz.hu/agent/credit_entry/xml · https://docs.szamlazz.hu/hu/agent/credit_entry/xml | Both examples/XSDs; `kifizetes maxOccurs="5" minOccurs="0"`; replacement semantics; issuer-tax-number translation discrepancy. |
| C-O | https://docs.szamlazz.hu/agent/credit_entry/response · https://docs.szamlazz.hu/hu/agent/credit_entry/response | Version 1 text `xmlagentresponse=DONE`, version 2 XML, balance fields/header table, error example. |
| C-D | https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd | Current request download; complete field/order match. |
| C-I | https://docs.szamlazz.hu/agent/credit_entry/other | IPN is payment-status notification; paid-amount changes, asynchronous delivery, form-urlencoded body. Consulted for semantic separation, not an IPN implementation review. |
| D-R | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request · https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/request | `action-szamla_agent_dijbekero_torlese`; POST multipart XML. |
| D-X | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml · https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml | Number/order examples and inline XSD. Hungarian-only paragraph explicitly says all proformas with the same order are deleted. |
| D-O | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response · https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/response | Dedicated deletion envelope, `sikeres`, error 335, critical text/HTML failures and response XSD. |
| P-R | https://docs.szamlazz.hu/agent/querying_pdf/request · https://docs.szamlazz.hu/hu/agent/querying_pdf/request | Three selectors, last matching order document, external ID must have been set at creation; multipart action. |
| P-X | https://docs.szamlazz.hu/agent/querying_pdf/xml · https://docs.szamlazz.hu/hu/agent/querying_pdf/xml | Example and conflicting EN/HU request schemas; see SC-01. |
| P-O | https://docs.szamlazz.hu/agent/querying_pdf/response · https://docs.szamlazz.hu/hu/agent/querying_pdf/response | Version 2 base64 PDF, optional balance/URL, error 7 on unknown number/order/external ID. |
| P-D | https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd | Current request download agrees with EN P-X and current writer. |
| E-O | https://docs.szamlazz.hu/agent/generating_invoice/response | Shared invoice envelope only: headers, optional fields, success/error/abbreviated-PDF examples. |
| E-D | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd | Shared reply download: qualified children, required boolean verdict, optional string/double/base64 fields. |
| E-E | https://docs.szamlazz.hu/agent/basics/error-handling | “at most five times”; no retry-until-success; 55 signing failure. Not a claim that every live-observed code appears in this catalogue. |
| S-S | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency | Storno “inherits the state of the original document”; no missing storno `simpleItems` setter. |

Also fetched the four category introductions:

- https://docs.szamlazz.hu/agent/category/reversing-invoice
- https://docs.szamlazz.hu/agent/category/registering-credit-entry
- https://docs.szamlazz.hu/agent/category/deleting-a-pro-forma-invoice
- https://docs.szamlazz.hu/agent/category/query-document-pdf

The deletion download links remain unavailable: each of the following returned **404** in this pass. D-X/D-O inline schemas are the fetched primary evidence for deletion, not a guessed replacement download:

- https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd
- https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd
- https://www.szamlazz.hu/szamla/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd
- https://www.szamlazz.hu/szamla/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd

## Actionable findings

### IO-01 — Document the batch scope of order-number proforma deletion

**P2 / medium; semantic guidance gap. Confidence: high in current source/code mismatch; batch execution unverified live.**

**Code:** `crates/szamlazz-agent/src/ops/proforma.rs:14–27,33–45,68–72`. `ProformaSelector::OrderNumber` only says “By order number”; the enum/operation repeatedly says “the proforma” and “Which proforma to delete.” The writer correctly sends `<rendelesszam>`, without a limit or expected document number. Supporting recovery prose at `crates/szamlazz-agent/src/recovery.md:16` likewise discusses a singular proforma.

**Current primary source:** https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml, directly after the order-number example:

> “Ha azonos rendelésszámmal több díjbekérő is van a számlázási fiókban, akkor a törlés az összes díjbekérőre vonatkozik.”

Translation: **If several proformas in the billing account have the same order number, deletion applies to all of them.** The English counterpart https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml omits this paragraph; it does not state a contradictory one-document guarantee.

**Concrete impact/reproduction scenario:** an account has `D-1` and `D-2` under order `O-1` (e.g. repetitions permitted). The caller means to remove only the proforma most recently retrieved by an order query and invokes:

```rust
DeleteProforma::new(ProformaSelector::OrderNumber("O-1".into()))
```

The offline writer probe emits `<fejlec><rendelesszam>O-1</rendelesszam></fejlec>`. By the vendor's current documented semantics, **both** proformas are targets. The response is only `()` / `<sikeres>true</sikeres>` and supplies neither count nor deleted numbers. A pre-delete query returning one latest document does not establish single-target deletion. Repeating that order-based request later may also target newly created matching proformas.

**Reconcile live record:** `docs/szamlazz-hu-behaviour.md:109` establishes that order-number deletion works, not the multiple-match result. Line 110 establishes paid-proforma deletion on one test account; together these make scope clarity consequential, but do not prove that every matching paid proforma would be deleted in one batch. Do not relabel this as a newly observed batch deletion.

**Bounded recommendation:** explicitly document “all matching proformas” on the order selector and operation/recovery guidance; distinguish it from number-based single-target deletion. No missing wire capability, no parser payload addition, and no automatic pre-query/guard is implied. The upstream operation offers no deleted-number/count response to expose.

### IO-02 — Envelope children are matched by local name, ignoring their namespace

**P3 / low; implementation gap. Confidence: high, reproduced. No normal vendor emission or production incident established.**

**Owned locations:** `crates/szamlazz-agent/src/ops/envelope.rs:104–117,272–275` (plain serde field extraction after root check); `credit_entry.rs:250–256`, `proforma.rs:78–83`, `query_pdf.rs:83–92` (affected operation entry points). **Shared cause:** `crates/szamlazz-agent/src/xml.rs:87–112` validates the root's expanded name but not child names; `xml.rs:213–219,254–257,273` uses local-name serde extraction for verdict/payload. Coordinator should deduplicate any sibling finding on this shared XML boundary.

**Current primary sources:**

- https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd declares `targetNamespace="http://www.szamlazz.hu/xmlszamlavalasz"`, **`elementFormDefault="qualified"`**, and `<element name="sikeres" type="boolean" … minOccurs="1">`.
- https://docs.szamlazz.hu/agent/reversing_invoice/response and https://docs.szamlazz.hu/agent/credit_entry/response repeat those declarations for their envelope fields.
- https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response declares the deletion namespace and the same **`elementFormDefault="qualified"`** / required `sikeres` relationship.

**Minimal public-parser reproduction** (HTTP 200/no error headers, or `RawResponse::new` without a status):

```xml
<xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz">
  <x:sikeres xmlns:x="urn:extension">true</x:sikeres>
</xmlszamladbkdelvalasz>
```

`DeleteProforma::parse` returns **`Ok(())`**, although the required protocol `sikeres` is absent: `{urn:extension}sikeres` is a different XML name. `xmlns=""` on the verdict is also accepted by the invoice-envelope consumers.

Other independently exercised inputs inside a correctly namespaced `xmlszamlavalasz`:

| Input | Actual result |
|---|---|
| Foreign `x:sikeres=true`, ordinary `szamlaszam=I-1` | Storno and credit both return success. |
| Ordinary `sikeres=true`, foreign `x:szamlaszam=FOREIGN-1` | Storno and credit identify `FOREIGN-1` as the protocol document number. |
| Ordinary verdict/number, foreign `x:szamlabrutto=999` | Storno and credit report gross `999`. |
| Ordinary verdict, foreign number and foreign `x:pdf=JVBERi0=` | PDF query returns `InvoicePdf` for `FOREIGN-1` with five decoded bytes. |

**Controls matter:** a wrong *root* namespace is rejected; an alias bound to the correct namespace is accepted; duplicate ordinary `sikeres` is rejected; an unrelated nested extension's gross is ignored; a second root after a deletion success is rejected. This is neither the old trailing-document defect nor a general claim that arbitrary nested fields overwrite data.

**Impact:** schema-incompatible or future-extension fields can satisfy the required success verdict or supply document identity/financial data. Unknown elements should be safely ignorable, not adopted because their local name resembles a protocol field. A namespace-aware check at recognized paths is sufficient; full XSD validation and mandatory auxiliary amounts are unnecessary. Preserve arbitrary prefixes bound to the expected URI, sparse optional content, and intentional numbered-56/header handling.

**Live/deviation disposition:** no behavior-note row records foreign/unqualified envelope children as a required compatibility case. All scoped live snippets and official examples use the declared default namespace. This gap has no live-backed exception. Its low priority reflects the unexpected-response trigger, not uncertainty about the reproduced mechanism.

### IO-03 — Correct the issuer-tax-number matching description

**P3 / low; semantic guidance gap/source translation conflict. Confidence: high for wording discrepancy, medium for detailed incoming-ledger behavior beyond the vendor statement.**

**Code:** `crates/szamlazz-agent/src/ops/credit_entry.rs:159–162`:

> “Tax number of the invoice issuer … matches the incoming invoice with the corresponding incoming receipt.”

The first clause is appropriate; the matching clause describes two documents instead of the credit entry being registered.

**Current primary sources:**

- https://docs.szamlazz.hu/hu/agent/credit_entry/xml, example comment: **“ha megadod a kiállító adószámát, a rendszer a bejövő kifizetést a megfelelő számlához rendeli”** — if the issuer's tax number is supplied, the system assigns the incoming payment to the corresponding invoice.
- https://docs.szamlazz.hu/agent/credit_entry/xml, inline-XSD comment: **“If provided, the system will match the incoming payment with the corresponding invoice.”**
- The **English example on that same page** still says **“match the incoming invoice with the corresponding incoming receipt”**, exactly the mistaken reading the Rust doc repeats. This is an upstream internal/translation discrepancy, not a newly broken writer.

**Impact/reproduction scenario:** an integrator reads `issuer_tax_number` as enabling invoice-to-receipt matching and either expects a receipt association from `RegisterCreditEntry` or omits it because they have no receipt. The actual request merely serializes `<adoszam>` in the registration settings (`credit_entry.rs:232–234`); it has no receipt selector and its result is an invoice balance. There is no basis here to describe receipt matching.

**Bounded recommendation:** retain the field and writer; describe the issuer tax number as assisting assignment of the incoming credit entry/payment to the corresponding invoice, with a source citation. Do not infer tax-number validation, cross-account access, or exact incoming-ledger precedence. The behavior record does not exercise issuer-tax-number matching, so neither incoming-ledger success nor a new runtime gate is established.

## Source conflicts and justified choices (not additional defects)

### SC-01 — Hungarian PDF request XSD conflicts with EN/download

At https://docs.szamlazz.hu/hu/agent/querying_pdf/xml the inline XSD text says:

```xml
<element name="szamlaszam" type="string" maxOccurs="1" minOccurs="1"></element>
<element name="valaszVerzio" type="int" maxOccurs="1" minOccurs="1"></element>
<element name="rendelesSzam" type="string" maxOccurs="1" minOccurs="0"></element>
<element name="szamlaKulsoAzon" type="string" maxOccurs="1" minOccurs="0"></element>
```

EN https://docs.szamlazz.hu/agent/querying_pdf/xml and download https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd instead make `szamlaszam` optional and place `rendelesSzam` **before** `valaszVerzio`. Both EN/HU request prose explicitly offers order/external-ID alternatives. The current writer at `query_pdf.rs:67–78` follows EN/download. The Hungarian block also has missing whitespace between `targetNamespace="…"` and `xmlns:tns="…"`; a standard-library XML parse of the HTML-decoded block failed at column 140 before any XSD validation.

**Disposition:** source conflict, current writer justified. Do not add a fake invoice number, reorder the writer to the Hungarian block, or disable external-ID lookup. Live external-ID PDF lookup is independently recorded (`behaviour.md:64–66`); live order-selector field ordering is not independently established here. Retain the distinct source versions in future drift work, rather than manufacturing a merged “official” XSD.

Fresh acquisition identifiers (code-block hashes have no added newline):

| Source | HTML/download SHA-256 | Inline-XSD SHA-256 |
|---|---|---|
| EN P-X | `bc1f0711108ab37ec15f58003b6e53789e3d27874ce3de5197e581c2391df666` | `24dcfe7f5ea673907061560a70800bf284aa24112971691803ad74ff99db6953` |
| HU P-X | `04fb76c65a80106a45df9bf8d8b3bfc05dd0cece62e7a2c067627b47d60ebc95` | `1c50375b586ddfeac1867af3c6b3b5427c60ded9772352456274d6ddcbda113c` |
| P-D | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` | n/a |

### SC-02 — Storno external ID is not an original selector in the observed numbered request

S-R says an original may also be referenced by external identifier; S-X describes later querying by the key. `storno.rs:86–98` correctly explains the conflict and the observed behavior: the value attaches to the **new SS**, only on actual creation, not on a repeat echo. `behaviour.md:63–70,95` records B6/XPRB-P4/P48-P6; reusing the original's external ID can make its query return the storno. The required original number and separate optional storno identifier remain correct. No external-ID-only storno capability is established by these sources.

### SC-03 — Published success examples are format illustrations, not executable live evidence

Current S-O/C-O/E-O success examples contain unescaped `&` in `vevoifiokurl`; S-O/P-O/E-O PDFs contain abbreviated `....`. Fresh standard-library parsing of the unmodified S-O/C-O success blocks rejects the bare ampersands. Their errors and XSD blocks parse as XML. This agrees with `fixtures/SOURCES.md:149–170`; the S-O/C-O HTML hashes reproduced the recorded `ae60e06a…83c` and `5d041bed…b2d` respectively. No leniency for broken XML or fabricated base64 is warranted.

The positive gross in the S-O success illustration proves neither positive-gross storno execution nor that `reverses()` should treat all numbered replies as reversals. Preserve the live negative-total reversals, same-number no-ops and the helper's explicitly inconclusive false result.

### Other deliberate limits and semantic observations

- **Empty replacement is a deliberate missing capability, not a newly found defect:** C-X/C-D allow zero credit entries, but `credit_entry.rs:217–220` rejects an empty replace. The #70 policy and `behaviour.md:211–215` explicitly defer destructive clearing until observed. Empty additive remains expressible. Do not remove the restriction to maximize XSD acceptance.
- **Version 2 only:** storno, registration and PDF always emit `RESPONSE_VERSION`; omission/version-1 text or raw-PDF success parsing is intentionally not offered. Deletion has no response-version field. Unexpected text/HTML remains diagnostic failure, not false success.
- **`download_copies: Option<u8>` versus XSD int:** smaller domain, but S-X says the server no longer processes the field. Default omission and deprecation docs are correct; no useful missing capability established.
- **Storno date/appearance:** `storno.rs:59–73,99–121` exposes caller values and documents account-bound observations. P48/P73 establish silent fulfillment-date/appearance mismatches and non-today issue-date refusal. The low-level client is not required to query the original or adopt the worker's stricter request contract. An explicit fulfillment date removes reliance on the server default; no schema assertion here proves a future default change itself causes rejection.
- **Buyer tax identifiers:** S-X explicitly says they may be supplied when missing from the original. `storno.rs:131–135` exposes both but omits this usage detail. This is a useful documentation clarification; the current wording does not promise that existing identifiers are overwritten, and no overwrite behavior was verified. Do not add a local gate or claim the fields correct an original invoice.
- **Simplified image:** S-S says storno inherits the original's state and that simplified layout overrides a template. No storno `simpleItems` request field is missing. `template` must not be interpreted as overriding this server rule.
- **Metadata exclusions:** credit balance omits the live-observed document-ID header; PDF omits document ID/payment method/notification-warning status. The current PDF page does not specify those three as distinct PDF-result fields. Preserve the historical bounded scope decision, rather than re-promoting rejected capability expansions. `RawResponse` remains available to custom transports.
- **Finite money/date domains and extra lexical leniency:** Decimal is intentionally not an IEEE NaN/INF type. Ordinary `1.27E3` parses successfully. The envelope accepts empty `sikeres` as false via the shared bool helper; it does not manufacture success. String/code/number trimming, unknown fields, optional totals and invalid auxiliary ID → `None` are existing policies, not grounds for a full XSD validator. Envelope URL trimming was explicitly excluded from historical R2's generic business-text change.

## Exhaustive request coverage inventory

Notation: **!** = required element in the agreeing current schema, **?** = optional. All scalar children are at most one unless noted. A listed constant is an intentional exposed capability boundary, not an omitted field. `string` permits open tokens; no unsupported local enum restriction is inferred from illustrative annotations.

### Common request mechanics

Each writer emits UTF-8 declaration, correct root/default namespace, escaped text, and children in explicit code order. `xml.rs:19–40,306–356` writes credentials either as `szamlaagentkulcs? string` or `felhasznalo? string → jelszo? string`, respecting schema positions. Examples sometimes show all credential elements; the exclusive credential modes are intentional. `xsi:schemaLocation`/`xmlns:xsi` are example validation aids, not required request fields. Multipart action constants match every operation's current request page; shared HTTP construction itself is separately owned.

### Storno — `storno.rs:162–207`, S-R/S-X/S-D

| Container, exact order | Type/cardinality and mapping | Assessment |
|---|---|---|
| Root `xmlszamlast` | namespace `http://www.szamlazz.hu/xmlszamlast`; action `action-szamla_agent_st` | Match. |
| `beallitasok!` | credentials → `eszamla! boolean` → `szamlaLetoltes! boolean` → `szamlaLetoltesPld? int` → `aggregator? string` → `guardian? boolean` → `valaszVerzio? int` → `szamlaKulsoAzon? string` | Every field supported; explicit false flags, optional count/aggregator/guardian/ID, version constant 2. Order matches EN/HU/download. |
| `fejlec!` | `szamlaszam! string` → `keltDatum? date` → `teljesitesDatum? date` → `megjegyzes? string` → `tipus? string` → `szamlaSablon? string` | Every field supported; original number required by model, dates optional, `tipus=SS`, open template enum covers all six annotated tokens. |
| `elado?` | `emailReplyto? string` → `emailTargy? string` → `emailSzoveg? string` | `SellerEmail`; container always emitted, children omitted when absent. Present empty optional container is schema-valid. |
| `vevo?` | `email? string` → `adoszam? string` → `adoszamEU? string` | All supported in order; same empty-container policy. No `sendEmail`/order selector/line-item block in this operation's XSD. |

### Credit entries — `credit_entry.rs:213–247`, C-R/C-X/C-D

| Container, exact order | Type/cardinality and mapping | Assessment |
|---|---|---|
| Root `xmlszamlakifiz` | namespace `http://www.szamlazz.hu/xmlszamlakifiz`; action `action-szamla_agent_kifiz` | Match. |
| `beallitasok!` | credentials → `szamlaszam! string` → `adoszam? string` → `additiv! boolean` → `aggregator? string` → `valaszVerzio? int` | Every field supported; number-only selector, optional issuer tax number, replace default (`false`), optional aggregator, constant 2. IO-03 concerns prose only. |
| `kifizetes` 0..5 | `datum! date` → `jogcim! string` → `osszeg! double` → `leiras? string` | `CreditEntry::{date,title,amount,description}`; Decimal writes finite numeric text; open `PaymentMethod` preserves future title tokens. Bounded collection guards construction, push and serde. |
| Empty replacement | schema permits zero; useful semantics not live-probed | Deliberately refused by `validate`; empty additive allowed. |

No order/external-ID selector, per-entry bank-account field, per-entry currency/exchange-rate setter, receipt association, or more-than-five single-request capability is in the fetched request schemas. Richer queried credit-entry fields do not imply corresponding write fields.

### Delete proforma — `proforma.rs:56–75`, D-R/D-X

| Container, exact order | Type/cardinality and mapping | Assessment |
|---|---|---|
| Root `xmlszamladbkdel` | namespace `http://www.szamlazz.hu/xmlszamladbkdel`; action `action-szamla_agent_dijbekero_torlese` | Match. |
| `beallitasok!` | credentials only | Match; no version, aggregator or guardian element to add. |
| `fejlec!` | `szamlaszam? string` → `rendelesszam? string` | Exclusive enum emits one alternative; **lowercase `rendelesszam`** is correct here. IO-01: order alternative is batch scope. |

Both/neither are schema-representable but not a documented useful selector combination. No external-ID deletion selector exists in the fetched schema. No paid-state guard/count/expected-number conjunction is on the wire.

### Query PDF — `query_pdf.rs:58–80`, P-R/P-X/P-D

| Exact root sequence | Type/cardinality and mapping | Assessment |
|---|---|---|
| `xmlszamlapdf` | namespace `http://www.szamlazz.hu/xmlszamlapdf`; action `action-szamla_agent_pdf` | Match. No `beallitasok` wrapper. |
| credentials → `szamlaszam?` → `rendelesSzam?` → `valaszVerzio!` → `szamlaKulsoAzon?` | strings, except int version; exclusive selector; constant 2 | Match EN/download; SC-01 records HU disagreement. Capital `S` in `rendelesSzam` is correct. |

P-R's last-order-match guidance is already present on the linked `InvoiceSelector::OrderNumber` (`types.rs:1038–1041`), so not a fresh missing-guidance finding. The query is a document lookup, not a guarantee of a plain invoice or completion of an earlier write. Recorded external-ID queries choose the latest holder; the recovery table tells callers to check identity/type/reversal state. No PDF-template/copy-count/preview/attachment selector is in this request XSD.

## Exhaustive response/header/verdict coverage inventory

### Shared invoice envelope — E-D/E-O/S-O/C-O/P-O

Expected root `xmlszamlavalasz`, namespace `http://www.szamlazz.hu/xmlszamlavalasz`. XSD sequence is exactly the following table; credit response schema stops before PDF. XML readers do not enforce sequence order on responses, an existing tolerant-reader policy, while request writers do.

| Response field | Current code | Verdict |
|---|---|---|
| `sikeres! boolean` | `xml::Verdict`; required; true/false/1/0; empty additionally treated as false | Normal verdict handled; IO-02 namespace issue. Missing verdict rejected except the deliberate header-56 fallback. |
| `hibakod? string` | `xml::Verdict` → open `ErrorCode` | False body parsed even with no headers. Missing/blank code → `Absent`; unknown token retained. Known body code is not evidence about earlier sends. |
| `hibauzenet? string` | `xml::Verdict` → `ApiError.message` | Text/CDATA decoded; missing text becomes empty message. Not exposed as success metadata. |
| `szamlaszam? string` | `envelope.rs:123–132,288–291` | Body before decoded header; trimmed/nonblank; required by storno/credit/PDF result. |
| `szamlanetto? double` | `envelope.rs:158–167,220–225`; credit equivalent | Optional Decimal, body first, header fallback; ordinary exponent works. |
| `szamlabrutto? double` | `envelope.rs:226–231`; credit equivalent | Same; negative storno totals retained. |
| `kintlevoseg? double` | `envelope.rs:232–237`; `credit_entry.rs:269–274`; `query_pdf.rs:90` | All results expose it; absent not zero. |
| `vevoifiokurl? string` | `envelope.rs:137–142`; all three result projections | Body first, otherwise decoded header; body URL not percent-decoded. Optional and opaque; no inferred portal activation. |
| `pdf? base64Binary` | `envelope.rs:146–147`; `types.rs:110–116` | Standard base64 with wrapping whitespace removed; optional for storno/create, required by `InvoicePdf` at `query_pdf.rs:92`. Credit ignores unexpected PDF since not in its operation response schema. |

| Header / metadata | Current handling | Evidence and boundary |
|---|---|---|
| `szlahu_szamlaszam` | Decode once, body fallback, trim/nonblank | Explicitly URL-encoded in S-O/C-O/E-O. |
| `szlahu_nettovegosszeg`, `szlahu_bruttovegosszeg` | Raw numeric headers; strict ungrouped dot/comma grammar, signed/exponent/HTTP SP-HTAB accepted | Explicitly **not URL encoded** in operation docs; comma form supplied by P60 observation. F3 fixed. |
| `szlahu_kintlevoseg` | Same numeric reader, optional | Recorded D7/body agreement; absent from the short current header table, not a reason to remove it. |
| `szlahu_vevoifiokurl` | Decoded textual fallback | Documented customer-account URL; no second decoding of body text. |
| `szlahu_fizetesmod` | Shared `header_payment_method`, open `PaymentMethod` | Present on `CreatedInvoice` and `InvoiceBalance`; historical E.E4 fixed. No payment-method XML element appears in actual envelope XSD despite prose “same data … in the XML body.” |
| `szlahu_id` | `CreatedInvoice.document_id`; raw signed integer parse, negative/invalid/blank → None | Recorded live document id, not seller/account id; auxiliary omission intentionally nonfatal. |
| `szlahu_error_code`, `szlahu_error` | Shared header verdict; raw code, decoded text; non-56 takes precedence | Explicit operation error channels. Header-free credit errors remain supported. |
| `szlahu_down`, HTTP status | Shared header policy consumed at envelope boundary | Existing down → error header → status → body policy; transport reviewer owns deeper behavior. No status-policy change proposed here. |

**Operation-specific verdicts:**

- **Storno:** `parse_issued` requires a number. `Ok(CreatedInvoice)` is wire success, not proof of a new reversal: repeat SS echoes and D/SL no-ops are retained. `reverses()` only checks different number/nonpositive known gross and expressly labels false inconclusive. Numbered 56 can return success with warning, dropping bad optional metadata; unnumbered 56 stays an API error. Non-56 body refusal is retained even alongside a header 56 when both parse normally.
- **Credit:** `xml::valasz` checks failure before projecting balance. No special 56-to-success rule. Current credit examples return number, net/gross, outstanding and URL, all represented. Live 463 body-only failure remains typed; replace/additive semantics concern current entries, not a new payment transaction response.
- **PDF:** same invoice envelope, number plus successfully decoded PDF required. Missing PDF/number fails, even though their XSD cardinality permits absence on an error or sparse reply. C2's two optional fields now survive projection. No live PDF-specific 56 emission is established; shared parsing does not justify expanding the public PDF warning fields.
- **Deletion:** dedicated `xmlszamladbkdelvalasz`, namespace `http://www.szamlazz.hu/xmlszamladbkdelvalasz`; sequence `sikeres! boolean → hibakod? int → hibauzenet? string`. Success yields `()` without requiring any headers; false yields code/message. Code 335 remains a refusal (`ProformaNotFound`), not automatically converted to replay success. Unknown code strings are retained beyond the int schema. Critical text/HTML yields bounded diagnostic failure. No deleted id/count payload is promised.

## Historical-findings closure

`docs/review/2026-09-09-agent-api/FINAL.md` was treated as a checklist, not current evidence. Supporting historical raw/round-2 material was consulted for exclusions and provenance.

| Historical ID / topic | Current disposition and evidence |
|---|---|
| **F3**, raw O4-02 — comma header fallback | **Closed.** `envelope.rs:316–375` separates header grammar, normalizes comma and handles exponents; body precedence retained. Fresh storno/PDF probe returns `100.01`. Existing `tests/response_headers.rs:265–303,373–448` covers cross-operation grammar/precedence/56; read, not suite-rerun. |
| **C2** — PDF outstanding and URL dropped | **Closed.** `query_pdf.rs:46–53,90–91` carries both with serde defaults. Fresh probe returned outstanding `5` and exact `opaque:a%2Bb+c&x=1`. |
| **E.E4** — create/storno payment method missing | **Closed.** `envelope.rs:49–53,239,280–285`; fresh encoded transfer header returned `Some(Transfer)`. Shared create path inspected; no invoice-create request audit claimed. |
| **E.E5a** — false storno heuristic described as conclusive | **Closed on the owned agent surface.** `envelope.rs:61–82`, `storno.rs:24–28,43–46` explicitly preserve uncertainty and recommend type/reference verification. Fresh sparse reply remains accepted and helper false, as intended. Worker implementation closure is outside this assignment. |
| **D6**, raw O4-04 — “unpaid” proforma restriction | **Closed.** `proforma.rs:1–6,33–35` now states paid-state policy belongs to the caller and cites the bounded observation. IO-01 is a distinct batch-target issue, not reopening paid-state guard semantics. |
| **R1**, raw O4-01 — shared trailing/incomplete XML | **Closed for the historical shared trailing-root case.** `xml.rs:75–140` traverses to completion; fresh deletion + `<other/>` fails. IO-02 is the narrower remaining child-namespace issue; do not claim the old last-root/truncation behavior persists. Taxpayer-specific closures are separately owned. |
| **R2** — optional business-text loss | Shared business-text helper now exists (`xml.rs:445–456`); invoice/receipt document audit separately owned. Historical judgment explicitly retained envelope number/URL/base64-specific policy. No mechanical envelope-string reopening here. |
| **D3 / B.E01 / B.E02** — recovery/provenance | Current `recovery.md:4–17,29–48` distinguishes operations, five total sends, 55 uncertainty, numbered-56 PHP evidence and unproved stalled issuance. Preserve these fixes. IO-01 additionally needs selector-specific deletion scope. |
| **E.E6**, current storno/credit examples | **Closed for owned provenance slice.** `fixtures/SOURCES.md:142–170` preserves July acquisition history and dates September additions, raw ampersands/abbreviated PDF and positive-storno example limits. Fresh S-O/C-O hashes and XML checks corroborate it. |
| **D10 / D11** housekeeping | PDF credentials already explicitly documented at root (`query_pdf.rs:15–19`); storno constructor correctly says optional fields absent and tests show required false flags and empty seller/buyer blocks. No stale omission claim found in these modules. |
| Rejected PDF id/payment/warning expansion | **Still excluded** on current evidence; do not count it as C2 still open. |
| F1/F2, invoice-create capabilities/guidance, receipts, taxpayer, cookie/transport items | Outside owned implementation scope. Supporting code/source reads are not a complete closure review of these packages. |

## Verification and limits

### Executed

1. Public GETs of the sources listed above, including all four operations' EN/HU request/example/inline-XSD/response pages and available downloads. No authentication or Számla Agent POST.
2. Standalone external consumer in **`/tmp/opencode/invoice-ops-review-20260910-02/`**, referencing the current workspace agent crate by path, with only synthetic `RawResponse` parsing and serialization:

   ```text
   cargo run --manifest-path /tmp/opencode/invoice-ops-review-20260910-02/Cargo.toml --offline
   ```

   Ran initially and again after adding PDF/namespace controls and assertions; final run succeeded. Its resolved parser-relevant versions (`quick-xml 0.42.0`, `rust_decimal 1.43.0`, `serde 1.0.229`, `jiff 0.2.35`) match the inspected workspace lockfile. It is an independent temporary lock/build, not a claim that every transitive package matches.

3. Public-source inspection script:

   ```text
   python3 /tmp/opencode/invoice-ops-review-20260910-02/source_check.py
   ```

   HTML-decoded `<pre>` extraction, SHA-256 and standard-library XML parsing: confirmed EN/download PDF sequence, malformed/conflicting HU PDF XSD, well-formed deletion inline schema, and malformed published storno/credit success illustrations. No source content was repaired before those checks. The Hungarian deletion page's acquired HTML hash was `73cada51b2ec49b015d70494e1140469ee70d3b84e5081f93be35486ca2f20bc`.

### Not claimed

- No full/targeted repository test suite run: coordinator owns suite verification. Existing golden, upstream, unit and response-header tests were inspected, not counted as fresh passing runs.
- No automated full XSD validation of every generated request. Field/order/cardinality coverage above is direct source/code comparison. `lxml` was unavailable; standard-library source checks establish XML well-formedness and extracted sequences, not XSD conformance. The temporary Rust probe is not a replacement schema validator.
- No new live evidence for multiple-match deletion, incoming credit matching, field omissions/encoding frequencies, mail delivery or 56, zero/negative-original storno, e-invoice subscription behavior, or PDF-order lookup execution. The recorded one-test-account deviations retain their original scope and dates.
- The inherited NAV-date legal statement in storno rustdoc was not independently re-researched against NAV law in this scoped operation-doc review; P48 behavior and the existing recorded decision were respected.
- Temporary work stayed under `/tmp/opencode`; no temporary test was added to the repository. The only repository file authored by this review is this report. Existing concurrent review/research files were left alone.
