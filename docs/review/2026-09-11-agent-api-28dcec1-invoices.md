# Számla Agent invoice creation — whole-surface review

**Date:** 2026-09-11. **Start and inspected HEAD:** `28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`.

## Result

**No confirmed runtime bug found in the reviewed invoice-creation surface.** Two low-priority capability limitations remain: the typed request cannot emit explicit `fizetve=false`, and the calculation helper supports net-first, not gross-first, derivation. Neither establishes incorrect server execution. Three important source conflicts remain open: combined preview/simple-items order, missing declarations in the downloadable XSD, and template labels.

This is a current whole-surface review, not a diff review: the specified start commit equals HEAD, so `git diff 28dcec1456cc08d50089ed8f9c9d15f877c7c3d2...HEAD` and the intervening commit list are empty. Tracked source was clean at entry. Existing untracked `77d53c5` reports were preserved and were not used as authority or read to seed findings. Current source, freshly fetched public documentation, and the repository's execution records were inspected independently. No additional agents, authenticated requests, or live tests were used. This review added only this report to the repository; verification artifacts are under `/tmp/opencode/invoices-28dcec1-*`. At completion HEAD was unchanged and tracked source remained clean; concurrently added queries/transport reports were left untouched.

Severity vocabulary: **P2** = material issue to resolve in normal engineering work; **P3** = limited capability/documentation follow-up. For source conflicts, priority describes the clarification work, not a proven product defect. Confidence in a source discrepancy is separate from confidence in its runtime effect.

## Findings and capability limits

### C-01 — P3: explicit unpaid emission is not representable

**Classification:** missing wire capability / known deliberate omission policy. **Confidence:** high for representation; unknown for any distinction in server behavior.

- **Current code:** `crates/szamlazz-agent/src/ops/invoice.rs:178-180` makes `paid` a `bool`; `:252` defaults it to false; `:823-825` writes `fizetve` only when true. The rustdoc accurately says false omits the element; `src/ops.rs:18-22` explicitly records the PHP policy and unverified equivalence.
- **Official contract:** both [EN inline XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml) and [HU inline XSD](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml), and the [download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd), declare an optional boolean `fejlec/fizetve`, without an XSD default. Thus absent, explicit false, and explicit true are distinct representable XML shapes.
- **Independent corroboration:** freshly downloaded [official PHP 2.12.4](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), member `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/Header/InvoiceHeader.php:393`, also emits only true. PHP is evidence of a client policy, not a server guarantee.
- **User impact:** a caller cannot reproduce an explicit-false XML request using `CreateInvoice`. **Not established:** that omission makes a cash invoice paid, that explicit false suppresses automatic paid treatment, or that any accepted document's outstanding amount is wrong. Those are hypotheses, not findings.
- **Offline reproduction:** set `header.paid = false`, call `to_wire` with a dummy key, inspect the first multipart file: no `fizetve`. True emits `<fizetve>true</fizetve>`. The scratch control reproduced both.
- **Existing evidence:** `docs/research/2026-09-10-agent-vendor-questions.md:108-119` records the unsent omission-versus-false question; no contrasting execution is recorded in `docs/szamlazz-hu-behaviour.md` or the September 11 live records.
- **Disposition:** retain as a low-priority representational gap; obtain the vendor's default/override semantics before claiming a financial bug or changing default emission.

### C-02 — P3: gross-first HUF derivation is caller-owned

**Classification:** missing convenience capability, not missing invoice capability or an arithmetic implementation bug. **Confidence:** high.

- **Current code:** `crates/szamlazz-agent/src/item.rs:163-222` exposes one derived constructor, taking **net** unit price. It rounds net, computes and rounds VAT, then adds gross (`:191-211`). `LineItem::new` (`:134-160`) accepts all three asserted amounts. `invoice.rs:908-917` preserves those amounts on the wire.
- **Official contract:** [EN rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding) and [HU rounding](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding) describe both net-first and gross-first methods. The gross-first method rounds extended gross, derives rounded VAT as `gross × rate / (100 + rate)`, then subtracts VAT for net. EN says B2C “have to”; HU says “valószínűleg” (“probably”). This is not evidence that the crate must select an arithmetic method based on buyer status.
- **User impact:** integrations starting with a fixed consumer gross price must calculate that split themselves and use `LineItem::new`. Converting to a rounded net price and using `try_calculated` can change the intended gross.
- **Meaningful offline illustration:** a 2 HUF gross row at 27% gives rounded VAT `round(2 × 27 / 127) = 0`, net `2`. Explicit `2 / 0 / 2` is representable and passes local `to_wire`. Supplying net unit price `2` to the net-first helper correctly produces `2 / 1 / 3`. The scratch control reproduced these outputs; it proves the methods differ, **not** server acceptance of this small-value illustration.
- **Existing evidence:** P60 in `docs/szamlazz-hu-behaviour.md:175-182` verifies selected net-first HUF/EUR behavior and EUR storage rounding, not universal equivalence of the two methods. The official 3 × 500 gross example (`unit net 393.66`, `net 1181`, `VAT 319`, `gross 1500`) is fully expressible with `LineItem::new`.
- **Disposition:** optional gross-first helper or a worked explicit-constructor example would improve usability. Do not call the existing correctly documented net-first constructor defective.

## Source conflicts and unresolved questions

### U-01 — P2 clarification: preview/simple-items sequence has two official answers

**Confidence:** high in the conflict and offline validity results; runtime acceptance and non-issuance remain unverified.

`invoice.rs:832-847` writes `szamlaSablon → elonezetpdf → simpleItems`. This agrees with the freshly fetched [downloaded XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) and official PHP `InvoiceHeader.php:398-404`. Both current [EN](https://docs.szamlazz.hu/agent/generating_invoice/xml) and [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) inline schemas specify `szamlaSablon → simpleItems → elonezetpdf`.

When both optional fields are present, **including explicit false values**, the current writer fails either inline schema at `simpleItems`; the same tail passes the download. When either is absent there is no tail conflict. The [tour-operator rule/example](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) places `simpleItems` after the template but does not include preview, so it cannot settle combined order.

**Impact:** a combined request cannot satisfy both published sequences. Rejection or ignored settings are plausible because the [sending rules](https://docs.szamlazz.hu/agent/basics/sending-requests#xml-validation-and-misspelled-tags) explicitly allow either refusal or ignored mistyped settings, but neither outcome has been observed for this correctly spelled combination. Do not infer that it issues an invoice, that it is refused, or that swapping the fields is a proven fix.

The policy and uncertainty are already disclosed at `invoice.rs:220-222,843-844`. `docs/research/2026-09-11-agent-vendor-clarification.md:62-85,93-98,129-134` is an unsent question, not a vendor answer. Both September 11 execution records explicitly exclude a combined-preview experiment. Any future execution needs to establish non-issuance as well as PDF rendering; a successful parser test is not that evidence.

### U-02 — P2 clarification: the downloadable invoice XSD omits supported fields

**Confidence:** high in published capability and schema discrepancy; account-specific runtime behavior not exercised.

The [download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) omits `vevo/csoportazonosito` and `tetelek/tetel/torloKod`; both [inline schemas](https://docs.szamlazz.hu/agent/generating_invoice/xml) contain them. `invoice.rs:874` and `:935-937` write them at the inline-defined positions, and the [erasure-code rule](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code) plus [linked HU guidance](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor) explicitly document the latter.

**Impact:** these requests fail validation against the download despite implementing current documented capabilities. Removing the fields to make that validator green would discard functionality. The current matrix explicitly expects these failures (`tests/schema_requests.rs:44-52`); independent validation confirmed the exact unexpected-element diagnostics, not merely any nonzero exit. This remains the open authoritative-schema question in `docs/research/2026-09-11-agent-vendor-clarification.md:74-85`.

### U-03 — P3 clarification: traditional/envelope-friendly template names conflict

**Confidence:** high in source conflict; no rendered-token execution evidence.

The [EN template table](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template) and [HU table](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template) label `SzlaAlap` traditional and `SzlaNoEnv` envelope-friendly. Their [linked knowledge-base article](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok) labels them the reverse, including its explicit token examples and PDF filenames. Its XML tag/token casing also differs (`szamlasablon`, `szlafuvarlevelesalap`); the XSD/Agent spelling is `szamlaSablon`, `SzlaFuvarlevelesAlap`.

`types.rs:989-1019` exposes every named token without swapping it. `InvoiceTemplate::Default` is explicitly documented as the named `SzlaAlap`, not omission (`:992-994`); omission is available via `header.template=None`. **Impact:** choosing by prose/UI label may yield an unexpected layout. No evidence supports changing the token mapping. `docs/research/2026-09-10-agent-vendor-questions.md:27-41` already records the open question. Public sample PDF filenames support the existence of the conflict; no fresh generated-template visual comparison was performed.

### Other discrepancies adjudicated without product findings

| Topic | Fresh source and current code | Assessment |
|---|---|---|
| “Every field in the example is mandatory” | EN/HU XML prose says this, while the same pages' XSDs mark many fields `minOccurs=0`; `invoice.rs:759-941` emits mandatory containers and omits absent options. | Source contradiction. Use actual schema cardinalities; do not add empty optional tags wholesale. EN's `teljesitesDatum` comment says “payment date”; HU correctly says fulfillment date. Rust's name is correct. |
| Delivery note versus waybill layout | Document-types prose emphasizes `SzlaFuvarlevelesAlap`; XSD separately exposes `szallitolevel` and permits `fuvarlevel` on invoices with a compatible layout. `invoice.rs:815-816,832-835,900-902`; `waybill.rs:87-112`. | Both delivery-note flag and layout are sent. Ordinary invoices may carry the layout/waybill. Template alone is not a reliable document-kind discriminator. Forced layout is disclosed at `invoice.rs:193-196`; no proven alternate-layout capability is lost. |
| KBAUK/KBAET meanings | EN/HU VAT tables abbreviate UK/ET and EN expands them as destinations. The linked [vendor VAT PDF](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf), freshly downloaded and text-extracted, describes new means of transport for KBAUK and exempt intra-EU goods for KBAET. | `types.rs:232-237` agrees with the detailed source; not a UK/Estonia-mapping bug. |
| K.AFA subtype selection | That PDF describes searching invoice comment, item name and item comment for exact phrases, defaulting to used goods. | `types.rs:203-210` documents it; all three text fields are available. No structured NAV subtype field is missing from the request schema. Matching precedence and runtime application remain untested. |
| Tour-operator availability | The linked [HU knowledge-base article](https://tudastar.szamlazz.hu/gyik/utazasszervezoi-szamla-kiallitasa) has an explicit Agent `simpleItems` section but also a stale sentence saying only manually issued UI invoices are supported. | Internal source conflict; the Agent rule, schema, and PHP implement the feature. Not grounds to remove Rust support. |
| Fractional HUF | EN/HU rounding pages prescribe integer net/VAT/gross but also describe server rounding for all eight integer/fractional combinations. | Local acceptance of fractional asserted values is not a demonstrated bug. P60's HUF requests were whole-valued; receipt fractional-amount observations do not establish invoice behavior. |

## Coverage inventory

Paths below are relative to `crates/szamlazz-agent/src/`. Each listed field was traced from public data to its actual XML position, not merely found by name. Schema validation establishes structure and lexical types, not correct business content, rendering or tax reporting.

### All six invoice-operation kinds

| Kind | Representation and emission | Validation, links and evidence |
|---|---|---|
| Invoice | `ops/invoice.rs:35-40,800`; no special flag | Optional proforma number emitted before all kind flags (`:793-798`). Explicit D → SZ linking has C2-3/D4 evidence. |
| Proforma | `:41-43,815`; `dijbekero=true` | Full buyer/header/items required. No dedicated deletion behavior inferred here. |
| Delivery note | `:44-46,816,832-835`; `szallitolevel=true`, forced waybill template | Optional waybill, all monetary data still supplied. C1 records SL creation; B5's storno no-op does not make it an invoice. |
| Prepayment | `:47-59,801`; `elolegszamla=true` | Optional proforma reference exposed and correctly ordered. C1-3 establishes implicit consumption by order; explicit D → ES remains unverified (`docs/szamlazz-hu-behaviour.md:255-261`). |
| Final | `:60-78,802-810`; `vegszamla=true`, optional `elolegSzamlaszam` | `:707-723` requires a nonblank prepayment number or order number. Optional proforma reference also emitted. One prepayment per final matches the rule. C6 verifies implicit prepayment linking, no automatic deduction, and refusal of another final against settled ES. Caller supplies negative prepayment lines. Explicit D → VS remains unverified. |
| Corrective | `:79-84,811-814`; flag plus `helyesbitettSzamlaszam` | Reference member is mandatory by construction but `InvoiceNumber` permits blank strings (`types.rs:22-38`); server validates reference content. Negative values/order preserved. B7/C5 document accepted corrective relationships; no broader relationship prohibition invented. |

Independent flags/reference combinations permitted syntactically by the XSD are intentionally narrowed to these six kinds. In particular, no proforma reference on corrective/proforma/delivery-note variants and no multiple simultaneous kind flags are exposed. **No documented valid business use was found for the excluded combinations**, so they are not elevated to missing capabilities. Storno is a separate Számla Agent operation, not a seventh variant here.

### Fields, nesting, order and options

| Surface | Complete covered field group | Current code |
|---|---|---|
| Envelope/settings | UTF-8 declaration; `xmlszamla` namespace; ordered `beallitasok`, `fejlec`, `elado`, `vevo`, optional `fuvarlevel`, `tetelek`; agent key or username/password; explicit `eszamla`, `szamlaLetoltes`; optional `szamlaLetoltesPld`, `aggregator`, `guardian`, `cikkazoninvoice`, `szamlaKulsoAzon`; response version 2 | `ops/invoice.rs:529-556,759-776`; `xml.rs:159-178,630-636`; `ops.rs` response-version constant. Copies are explicitly documented deprecated (`invoice.rs:543-547`), so the u8 subset of XSD int has no established current effect. |
| Header base | Optional `keltDatum`; mandatory fulfillment/due dates, payment method, currency, language; optional comment, bank/rate, order; all kind flags/references; extra logo, prefix, payable adjustment | `ops/invoice.rs:140-177,779-821`; exact field capitalization, decimal formatting and sequence verified. |
| Header tail | `fizetve`, `arresAfa`, `eusAfa`, template, preview, `simpleItems` | `ops/invoice.rs:178-225,823-847`; true-only paid policy C-01; other optional booleans retain absent/false/true; U-01 tail conflict. |
| Seller | Bank, account, reply-to, subject, body, signer; seller container always present even when empty | `ops/invoice.rs:262-275,849-858`; `types.rs:1023-1033`. Seller identity comes from the account; request schema has no seller name/address override. |
| Buyer | Name, country, ZIP, city, street, email, send-email, taxpayer status, tax number, VAT-group identifier, EU tax number; five postal fields; buyer ledger; partner identifier, signer, phone, comment | `ops/invoice.rs:277-347,859-899`. Postal address fields are flattened under `vevo`, not placed in a new wrapper. Partner-id update/access effects are documented (`:329-339`). |
| Buyer ledger | Accounting date, buyer id, buyer account, continuous fulfillment, settlement start/end, inside `vevoFokonyv` | `ops/invoice.rs:120-135,883-893`. |
| Waybill common | Destination, carrier, general barcode, comment, then TOF, PPP, Sprinter, MPL blocks | `ops/waybill.rs:94-112,132-178`; `ops/invoice.rs:900-902`. `uticel` unused status and barcode fallback documented. Carrier free text can carry TOF/PPP/SPRINTER/FOXPOST/MPL/GLS/EMPTY; no extra child blocks are declared for FOXPOST/GLS. |
| TOF | Identifier, shipment ID, parcel count, country code, ZIP, service | `ops/waybill.rs:11-24,137-147`. |
| PPP | Barcode prefix, per-document suffix | `ops/waybill.rs:28-33,149-154`. |
| Sprinter | Identifier, sender code, routing code, parcel count, barcode suffix, delivery time | `ops/waybill.rs:37-50,155-166`. Carrier-specific lengths remain server/content rules; u32 counts are checked against XSD int maximum. |
| MPL | Mandatory customer code, barcode, weight string; optional extra-service configuration, declared value | `ops/waybill.rs:54-84,167-176`; no Default bypass of the three mandatory members. Weight is correctly string, not double. |
| Items | Name, id, quantity, unit, net unit price, VAT token, optional margin VAT base, net/VAT/gross, comment, ledger, erasure count | `item.rs:90-126`; `ops/invoice.rs:903-940`. Caller row order and repeated rows retained. No item maximum imposed except conditional vendor simple-items rules. |
| Item ledger | Economic event, VAT economic event, revenue account, VAT account, settlement start/end under `tetelFokonyv` | `item.rs:59-72`; `ops/invoice.rs:919-934`. |
| Shared values | All 15 language tokens including vendor-specific `cz` and `si`; taxpayer statuses 7/6/1/0/−1; all six template tokens plus Other; open currency/payment/VAT sets; explicit/automatic MNB | `types.rs:178-365,369-478,480-585,588-686,690-755,943-1033`. All published currency/rate values are representable; no obsolete local whitelist blocks them. |
| Attachments | Up to five, at most 2,000,000 bytes each; filename, MIME type and raw bytes; separate multipart parts `attachfile1`…`attachfile5` | `ops/invoice.rs:379-516,959-970`; `wire.rs:66-125`. Push, TryFrom and serde preserve the bounds; no mutable slice exposure bypass. Decimal MB interpretation is stated. |
| Preview/reply | Request preview independent of PDF-download flag; numbered reply → Issued; unnumbered successful reply + requested preview + PDF → Preview; missing number otherwise fails | `ops/invoice.rs:621-676,944-956`; `ops/envelope.rs:179-248`. Optional totals, PDF, id, outstanding, payment method and customer account URL retained; numbered 56 preserves issuance. Synthetic coverage does not verify real preview output. |

### Request validation and arithmetic

- `ops/invoice.rs:682-703` validates all eight date positions (three header, three buyer ledger, two item ledger) across all items. `xml.rs:19-35` deliberately restricts outbound dates to positive years. Jiff supplies calendar validity; local checks do not impose chronology or the server's issue-date behavior.
- `ops/invoice.rs:704-751` refuses empty items, a final without either prepayment selector, erasure counts above 400, counts above signed-32-bit maximum, and foreign-currency requests without a usable quoting bank/rate. Automatic MNB is bank `MNB` with the numeric rate omitted. Both the inline and downloaded annotation explicitly allow it despite the general currencies page saying both fields must be supplied.
- The foreign-currency guard covers all six kinds. A rate-free EUR proforma/delivery note/AAM invoice is schema-representable but not known to execute; `docs/szamlazz-hu-behaviour.md:237-244` explicitly leaves it unverified. This is a conservative validation policy, not a confirmed overvalidation bug. Zero/negative numeric rates and account-specific bank availability are left to szamlazz.hu.
- `wire.rs:405-411` runs validation, serialization and XML 1.0 character checks before multipart construction. `client.rs:375-379` invokes this boundary before posting. `write_xml` is explicitly unchecked (`wire.rs:367-373`); calling it directly is not a validation bypass hidden from its contract. Escaping and plain decimal/date formatting are at `xml.rs:586-625`. Namespace declaration is present; `xsi:schemaLocation` is an optional validation hint, not missing protocol data.
- `item.rs:191-211` uses exact checked operations, with explicit half-away-from-zero rounding (`:42-48`), and preserves gross = net + VAT. `number.rs:6-59` cancels decimal factors and rejects unrepresentable intermediates; checked failure before later rounding is documented at `item.rs:178-182`. Numeric `VatRate::Other` tokens are interpreted numerically rather than silently assigned zero VAT (`:194-200`). Non-numeric special codes yield zero VAT as documented; margin calculations remain caller-owned through explicit fields.
- No local arithmetic revalidation is promised (`item.rs:74-87`). Negative-price positive-quantity discount lines, immediately following the discounted row, are representable as the [discount rule](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount) requires. There is no omitted percentage-discount wire field.
- `simpleItems` does not strip monetary data. `invoice.rs:199-218` documents allowed kinds, two/four-item limits, allowed VAT tokens, final inheritance/rate matching, OSS/Hungarian-tax-number prerequisites, template override and K.AFA comment requirements. `validate` intentionally leaves these business rules to szamlazz.hu (`tests/simple_items.rs:72-88`); accepted local validation is not evidence of accepted vendor content.
- Email absence/false/true behavior, comma-separated recipients, BBCode/newlines and dynamic labels are representable. The vendor ignores attachments when email is not requested, and sends valid attachments while separately reporting invalid ones. The collection's early size refusal is a disclosed stricter local policy, not proof the vendor would refuse the invoice. Actual invoice attachment processing and delivery were not executed here.

## Execution evidence and known deviations

The following records were inspected **before adjudicating deviations**. Their observations apply to their stated accounts and dates; existing test source and an unsent clarification are not execution evidence.

| Record | What it establishes for this review | Boundary / current interpretation |
|---|---|---|
| `docs/szamlazz-hu-behaviour.md:54-88` | Per-kind order-number duplicate checks, exact-request replay, order trimming/case, external-id nonuniqueness/latest-holder behavior | `CreateInvoice` exposes the identifiers but offers no idempotency guarantee. A replay may not attach a newly supplied external id. No client-side automatic replay inferred. |
| `:109-117` (P48/P73) | Yesterday's create issue date replaced by today on one paper test-account request; e_invoice false/true queried as appearance 1/3 | `invoice.rs:141-148,532-537` correctly distinguishes request from stored fact. Not a date-serialization or boolean-mapping bug. |
| `:123-139` (C1/C2/C6/D4/D5) | Implicit proforma consumption; explicit SZ conversion; vanished/consumed proforma reference can be silently dropped; final does not deduct prepayment | Correct references are sent; link success cannot be established from the create reply alone. Negative deduction belongs in caller line items, as `InvoiceKind::Final` says. |
| `:143-146` | Corrective accepted with base reference and negative totals | Supports the exposed corrective kind; does not prove every possible base chain. |
| `:173,179-182,270-275` (D6/P60) | Bad invoice email did not produce 56; chosen HUF arithmetic tolerance; independent two-decimal rounding of EUR amounts; 27.0/27.00 accepted | `Rounding::Exact` explicitly warns about the EUR storage discrepancy. Do not extrapolate invoice observations to receipts, all currencies, tiny values, or live-account delivery. The exact tolerance remains unknown. |
| `docs/research/2026-09-11-credit-clearing-live.md:14-32,53-75` | Two ordinary test invoices created before successful clearing probes; returned expected numbers and outstanding amounts | Parsed observations, no raw response channels. No preview, attachment, or unpaid-false contrast. |
| `docs/research/2026-09-11-receipts-live.md:3-16,44-73,108-118,139-144` | Executed receipt lifecycle/MNB/email observations, with operator-confirmed inbox delivery | Receipt-specific. Not invoice fractional-HUF, invoice template, invoice attachment or combined-preview evidence. |
| `docs/research/2026-09-10-agent-vendor-questions.md`; `2026-09-11-agent-vendor-clarification.md` | Existing open questions on schemas, templates, paid-state and success identity | Not sent/no vendor answer. Their linked prior review claims were not adopted as evidence. Fresh source acquisition independently confirms the conflicts reported here. |

## Verification performed

### Targeted existing offline checks

Run from the repository root, with no client feature and no vendor credentials:

```sh
SZAMLAZZ_SCHEMA_OUTPUT=/tmp/opencode/invoices-28dcec1-matrix.json cargo test -p szamlazz-agent --locked --offline --test schema_requests -- --ignored --exact emit_request_matrix
cargo test -p szamlazz-agent --locked --offline --lib ops::invoice::tests::
cargo test -p szamlazz-agent --locked --offline --lib item::tests::
cargo test -p szamlazz-agent --locked --offline --test simple_items
```

Results: **1 export test, 31 invoice unit tests, 8 arithmetic unit tests and 2 simple-items tests passed.** The exporter emits the existing all-operation matrix; only its **134 invoice requests** were validated here. The exporter is explicitly a synthetic offline test despite `--ignored`; no `live` or `probes` target ran. No broad workspace/all-feature suite was duplicated.

### Fresh XSD comparison, with a real validator

`/tmp/opencode/invoices-28dcec1-acquire.py` fetched public EN/HU HTML, extracted their `<pre>` XSD text, and fetched the downloadable XSD, PHP ZIP and VAT PDF anew. No prior scratch/report files supplied source bodies. `/tmp/opencode/invoices-28dcec1-validate.py` ran libxml2 **2.15.3** `xmllint --nonet --noout --schema … -` separately against each unmodified schema. Expected conflicts were checked by exit code **3** and the exact multiset of unexpected-element diagnostics; unrelated validation failures would fail verification.

| Fresh source | Valid generated requests | Expected source-conflict requests | Declared element paths occurring in fully valid requests |
|---|---:|---:|---:|
| EN inline | 124 | 10 | 125/125 |
| HU inline | 124 | 10 | 125/125 |
| Download | 120 | 14 | 123/123 |

Thus **402 schema evaluations** produced no unexpected result. Nine negative controls (missing seller container, invalid boolean, invalid money, each against all three schemas) were refused. Matrix cases include both credential forms, all six kinds and link alternatives, full/common/isolated blocks, empty containers, all preview/simple-items presence combinations, all languages/templates/statuses, erasure boundaries, MNB/explicit EUR rates, multiple rows and five attachments. Path coverage is not exhaustive combinatorial or business-rule coverage.

The first scratch coverage walker incorrectly treated a referenced simple type as a complex type and stopped with `KeyError`; it was corrected and the complete verification passed. `xmllint` and `pdftotext` were absent from PATH and Python lxml was unavailable; existing Nix-store binaries were located and used. No tool installation or fixture edit was required.

`/tmp/opencode/invoices-28dcec1-controls.rs` was compiled against the current crate artifact identified by `cargo build -p szamlazz-agent --locked --offline --message-format=json`. It verified C-01 emission and C-02's explicit/derived distinction. It contains no network client. The official VAT PDF was converted with Poppler `pdftotext -layout` for inspection of KBAUK/KBAET and K.AFA explanations.

### Acquisition identity

All URLs were fetched on **2026-09-11**. Agent/PHP pages displayed documentation version **v202608271632**; the PHP landing page advertised **2.12.4 (2026-08-12)**.

| Source artifact | SHA-256 |
|---|---|
| EN invoice XML page, raw HTML | `f7313b38dde363781d3fafe04def67d135f6f1531653b760d6b5715a0bdaf812` |
| HU invoice XML page, raw HTML | `8c75694ee699d136d0f09c016c36fc57b0213c6717c83a92c58643110dfeb367` |
| EN inline XSD, extracted text | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` |
| HU inline XSD, extracted text | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` |
| Downloaded xmlszamla.xsd | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` |
| Official PHP 2.12.4 ZIP | `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| Vendor VAT PDF, 2025-11-04 | `bb5a52eda87e383be3276870fee937c6a34d87f7a2a3542a03a0bf90a63d9465` |

## Official-source inventory and limits

Fresh reading started at <https://docs.szamlazz.hu/agent/> and followed the invoice category. In addition to URLs cited above, these were fetched and inspected:

- [Request, action field and attachments](https://docs.szamlazz.hu/agent/generating_invoice/request).
- [Response, examples and inline response XSD](https://docs.szamlazz.hu/agent/generating_invoice/response).
- [Settings/rules index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules), and **all ten** linked rules: [document types](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types), [tour operators](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency), [VAT rates](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), [rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding), [currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies), [templates/languages](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template), [order number](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number), [discounts](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount), [email](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification), [erasure codes](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code).
- HU [document types](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types), [VAT](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates), [rounding](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding), [templates](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template), and full HU request XML/XSD for ambiguity checks.
- [Sending rules](https://docs.szamlazz.hu/agent/basics/sending-requests), [error table](https://docs.szamlazz.hu/agent/basics/error-handling), [PHP download landing page](https://docs.szamlazz.hu/php/).
- Linked HU knowledge-base guidance for [VAT](https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere), [template examples](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok), [erasure codes](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor), [tour operators](https://tudastar.szamlazz.hu/gyik/utazasszervezoi-szamla-kiallitasa), and [dynamic email fields](https://tudastar.szamlazz.hu/gyik/szamlaertesito-egyedi-mezok), plus the linked VAT PDF.

Limits: public documentation describes a broader set of acceptable content than any offline suite can execute. No current-account entitlement, NAV submission, barcode rendering, combined preview, real attachment delivery, layout default, cash paid-state or cross-kind reference experiment was performed. Public sample PDFs for each layout were not individually downloaded/rendered, and the linked knowledge base was followed for invoice-specific facts rather than recursively crawling every tax/UI article. The response parser was traced for creation/preview semantics; a full adversarial review of shared transport and unrelated response operations is outside this invoice-focused report. No universal guarantee is inferred from XSD optionality or from a handful of historical successful requests.
