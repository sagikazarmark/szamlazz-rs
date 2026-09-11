# Számla Agent compliance review: mutation requests and invoice-creation responses

Reviewed **2026-09-11**, against **HEAD `2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81`** and freshly fetched official documentation.

## Result

**One medium-severity response-contract defect:** a successful credit-registration acknowledgement without an echoed invoice number is rejected, although the official response schema permits it. Confidence is high in the code/schema mismatch; occurrence on an actual account is unverified.

The storno, credit-entry and proforma-deletion request fields, their order, namespaces and multipart routing match the fetched schemas. Invoice creation and storno preserve numbered code-56 issuance evidence, distinguish preview responses, and support the documented response metadata. Deletion correctly exposes all-matches order selection. The observed no-op storno and other account-tested exceptions must be retained.

This is an independent review of the current files, not a diff review or a reuse of the September 10 reports. Those reports were not used as evidence. This review owns `ops/envelope.rs`, `ops/storno.rs`, `ops/credit_entry.rs`, `ops/proforma.rs`, and the response side of `ops/invoice.rs`; shared XML/wire helpers were traced where necessary. Full crate tests belong to the lead review; the targeted checks executed here are recorded below. No vendor account was invoked and no source or tests were edited. No further delegation was performed.

## Fresh official sources and navigation

All links below were fetched during this review. Documentation pages displayed footer version **`v202608271632`**; this is the site's displayed version, not a claim that all content was last modified then. Page citations refer to the named section or inline XML/XSD tab; the rendered pages have no stable source line numbers. Repository line references below are at the pinned HEAD.

Navigation started at [What is Számla Agent?](https://docs.szamlazz.hu/agent/basics/what-is). Its links and the expanded operation sidebars establish the actual paths: deletion is **`deleting_pro_forma_invoice`**, not `deleting_proforma`. Storno and deletion have request/response/XML pages, with no separate settings page in their navigation. Credit has those three plus `other` (IPN); its request settings are in its XML page. An initial request to the categoryless `/agent/reversing_invoice` returned 403; the actual navigated `/request`, `/response`, and `/xml` pages all succeeded.

| ID | Official source, freshly fetched | Use |
|---|---|---|
| S1 | [Storno request EN](https://docs.szamlazz.hu/agent/reversing_invoice/request), [HU](https://docs.szamlazz.hu/hu/agent/reversing_invoice/request) | Endpoint, POST/file routing, required original number, ambiguous external-id prose |
| S2 | [Storno XML/XSD EN](https://docs.szamlazz.hu/agent/reversing_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml) | Every request field, sequence, cardinality and type |
| S3 | [Storno response EN](https://docs.szamlazz.hu/agent/reversing_invoice/response), [HU](https://docs.szamlazz.hu/hu/agent/reversing_invoice/response) | Response versions, headers, envelope, success/error examples and response XSD |
| S4 | [Credit request](https://docs.szamlazz.hu/agent/credit_entry/request) | Multipart route |
| S5 | [Credit XML/XSD EN](https://docs.szamlazz.hu/agent/credit_entry/xml), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/xml) | Settings, additive/replacing semantics, zero-to-five entries |
| S6 | [Credit response EN](https://docs.szamlazz.hu/agent/credit_entry/response), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/response) | Optional echoed number and metadata, mandatory verdict |
| S7 | [Credit IPN page](https://docs.szamlazz.hu/agent/credit_entry/other) | Adjacent account setting and asynchronous payment-status notifications; not the synchronous credit acknowledgement |
| S8 | [Deletion request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request) | Multipart route |
| S9 | [Deletion XML/XSD EN](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml) | Selection fields; HU explicitly says all matching proformas |
| S10 | [Deletion response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response) | Dedicated XML acknowledgement, 335, critical text/HTML errors |
| S11 | [Invoice creation response EN](https://docs.szamlazz.hu/agent/generating_invoice/response), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/response) | Response formats, all headers, envelope/XSD |
| S12 | [Invoice request](https://docs.szamlazz.hu/agent/generating_invoice/request), [XML/XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml) | Route, version/PDF settings, `elonezetpdf` means no actual document |
| S13 | [Settings navigation](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules), [document types](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types), [template](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template), [notification](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification), [simpleItems](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Relevant response/request settings; storno inherits simplified-image state |
| S14 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests), [error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | Route cross-check, XML validation, version-1 errors, five-total-send limit |
| S15 | [PHP response handling](https://docs.szamlazz.hu/php/valasz-feldolgozas), [storno](https://docs.szamlazz.hu/php/sztorno-szamla-generalas), [credit](https://docs.szamlazz.hu/php/jovairas), [deletion](https://docs.szamlazz.hu/php/dijbekero-torles) | First-party corroboration of notification failure and operation semantics |
| S16 | [Downloadable storno XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd), [credit XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd) | Both fetched successfully; field sequences agree with inline schemas |
| S17 | [PHP package landing page](https://docs.szamlazz.hu/php/), [2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Fresh first-party implementation evidence for numbered 56 |

S17 ZIP SHA-256: `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Paths inside the archive:

- `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/Response/InvoiceResponse.php:17`: notification-failure constant is 56.
- Same file `:314–323`: an error is overridden only when `hasInvoiceNumber()` and `hasInvoiceNotificationSendError()` both hold; `:199–200` tests a nonblank number; `:427–431` classifies the code.
- `PHPApiAgent-2.12.4/szamlaagent/examples/document/invoice/create_reverse_invoice.php:51–60`: explicitly checks success and notification failure on a storno.

The literal deletion schema locations in S9/S10, upgraded to HTTPS, returned **404**:

- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd`
- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd`

Consequently deletion was compared against the complete inline schemas, not a successfully downloaded deletion XSD. No guessed replacement path is treated as authoritative.

## Finding M-01 — Numberless successful credit acknowledgement becomes a parse failure

**Severity:** medium. **Confidence:** high in contract mismatch and reproduction; live frequency unknown. **Classification:** real response-contract limitation, not an observed account regression.

**Location:** `crates/szamlazz-agent/src/ops/credit_entry.rs:250–256`; public response shape at `:191–210`.

S6 says additional headers **“may also arrive”**, and its response XSD requires only `sikeres`. In particular:

```xml
<element name="sikeres" type="boolean" maxOccurs="1" minOccurs="1"/>
<element name="szamlaszam" type="string" maxOccurs="1" minOccurs="0"/>
```

The page also explicitly says elements marked `minOccurs="0"` may not always be included. A completed success with no number in either channel is therefore within the published schema. Credit registration operates on an already named invoice; the success need not establish the identity of a newly created document.

After successfully validating that envelope and its `sikeres=true`, the implementation calls `body.invoice_number(response).ok_or(ParseError::Missing("szamlaszam"))?`. This changes a successful acknowledgement to an error solely because optional echoed metadata is absent. Neither README nor the live-behavior notes establish a deliberate number-required policy for credit acknowledgements, or a stronger always-numbered vendor guarantee. D7 describes populated responses, not all permissible responses.

### Concrete reproduction

Give `RegisterCreditEntry::parse` HTTP 200, no `szlahu_*` headers, and:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
  <kintlevoseg>0</kintlevoseg>
</xmlszamlavalasz>
```

Minimal standalone Rust, using the public interface:

```rust
use szamlazz_agent::ops::credit_entry::RegisterCreditEntry;
use szamlazz_agent::wire::{AgentRequest, RawResponse};

let request = RegisterCreditEntry::new("I-1");
let raw = RawResponse::new::<&str, &str>([], br#"
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres><kintlevoseg>0</kintlevoseg>
</xmlszamlavalasz>"#.to_vec()).with_status(200);
assert!(matches!(request.parse(&raw),
    Err(szamlazz_agent::ResponseError::Parse(
        szamlazz_agent::ParseError::Missing("szamlaszam")))));
```

The executed scratch reproduction additionally populated a valid credit entry, verified `to_wire` succeeds, and exercised **both `additive=false` and `additive=true`**. Both returned exactly `Err(Parse(Missing("szamlaszam")))`. Adding header `szlahu_szamlaszam: I-1` to a verdict-only success returned `Ok(InvoiceBalance { … })`, isolating the missing-number condition.

**Impact:** the caller loses a successful acknowledgement and its balance. A caller that resends on that apparent failure could append the credit twice, or replace newer entries with an older intended set. The crate itself performs no automatic resend; this review does not claim duplication occurs automatically.

**Recommended correction:** permit success without an echoed number, either by representing the echo as optional or by explicitly documenting a request-number fallback as request identity rather than server-verified metadata. Continue requiring a real success verdict and retain current error/status checks. This recommendation applies to credit registration, not to inventing a number for invoice creation or storno.

## Request field and operation coverage

`?` means optional in the source schema. Arrows are actual child order, not just a list of supported fields. All rows were inspected against the current writer and the fresh source; golden/upstream tests are supporting regression evidence, not proof that the vendor accepted a request.

### Common routing and scalar emission

`wire.rs:7–14,66–99` builds a multipart file part with the operation's action as `name`, a filename, `Content-Type: text/xml`, and correct CRLF framing. The single target is `https://www.szamlazz.hu/szamla/`. The core hands bytes to the transport; operation methods do not issue network calls themselves. UTF-8 declaration and namespace are written by `xml.rs:132–153`. Text is escaped, booleans are `true`/`false`, Decimal is plain numeric text, and dates are civil-date text (`xml.rs:561–613`). Credential alternatives emit `szamlaagentkulcs` or `felhasznalo → jelszo` in schema order. Schema-location attributes in examples are validation hints, not mandatory request fields.

| Operation | Multipart field | XML root / namespace suffix | Result |
|---|---|---|---|
| Invoice creation | `action-xmlagentxmlfile` | `xmlszamla` | Route/version cross-check only; detailed create request outside this report |
| Storno | `action-szamla_agent_st` | `xmlszamlast` | Match; `storno.rs:162–183` |
| Credit registration | `action-szamla_agent_kifiz` | `xmlszamlakifiz` | Match; `credit_entry.rs:213–236` |
| Proforma deletion | `action-szamla_agent_dijbekero_torlese` | `xmlszamladbkdel` | Match; `proforma.rs:60–79` |

All namespaces are exactly `http://www.szamlazz.hu/{root}`. These HTTP namespace identifiers are not request endpoints and must not be rewritten to HTTPS.

### Storno — S1/S2/S16

| Block | Exact sequence and representation | Current coverage |
|---|---|---|
| Root | `beallitasok → fejlec → elado? → vevo?` | `storno.rs:171–206`; empty seller/buyer blocks are emitted, valid because all children optional |
| Settings | credentials → `eszamla` → `szamlaLetoltes` → `szamlaLetoltesPld?` → `aggregator?` → `guardian?` → `valaszVerzio?` → `szamlaKulsoAzon?` | All supported in exact order, `:171–183`; booleans required and emitted; version pinned to 2 |
| Header | `szamlaszam` → `keltDatum?` → `teljesitesDatum?` → `megjegyzes?` → `tipus?` → `szamlaSablon?` | `:185–193`; original number required in Rust, `tipus=SS` explicitly emitted |
| Seller | `emailReplyto? → emailTargy? → emailSzoveg?` | `:195–201`; all supported |
| Buyer | `email? → adoszam? → adoszamEU?` | `:202–206`; all supported. XSD annotation describes tax-number supplementation when absent on original |
| Template | `SzlaMost`, `SzlaAlap`, `SzlaNoEnv`, `Szla8cm`, `SzlaTomb`, `SzlaFuvarlevelesAlap` | Shared `InvoiceTemplate`, passed through `as_wire`; no integer is emitted merely because S15's PHP table says `int` |

The deprecated copies field has an XSD `int` domain and a narrower `Option<u8>` library domain; S2 says the server ignores it. This is an inconsequential API narrowing, not a request defect. No `simpleItems` field belongs in the storno XSD: S13 says storno **inherits** the original's simplified-image state. Likewise, invoice-create `sendEmail` and attachment fields are not silently missing storno fields; the storno schema does not declare them.

### Credit registration — S4/S5/S16

| Block | Exact sequence and representation | Current coverage |
|---|---|---|
| Root | `beallitasok → kifizetes{0..5}` | `credit_entry.rs:230–245` |
| Settings | credentials → `szamlaszam` → `adoszam?` → `additiv` → `aggregator?` → `valaszVerzio?` | `:230–237`; all represented, exact case/order, response version 2 |
| Each entry | `datum → jogcim → osszeg → leiras?` | `:239–244`; Date, open `PaymentMethod`, Decimal and optional description |
| Collection | Maximum five | `CreditEntries::push`, `TryFrom<Vec<_>>`, custom `Deserialize` enforce the same bound (`:48–132`) |
| Replace/additive | false replaces; true retains existing entries and appends | Explicitly serialized boolean; false default (`:163–187`), agrees with wire example and D7 |
| Empty replacing request | Schema permits zero; crate refuses it | `:217–222`, intentional policy described below |

S15's PHP credit table defaults `additive` to **true**. That is the PHP wrapper's default, not a contradictory wire rule: this crate always emits its explicit false/true value. `adoszam` is the issuer's tax number; the EN example's “incoming receipt” translation is less precise than the HU note about assigning the incoming payment to the corresponding invoice. The crate represents and sends the field; no account-level matching behavior was tested here.

### Proforma deletion — S8/S9/S10

| Block | Exact sequence and representation | Current coverage |
|---|---|---|
| Root | `beallitasok → fejlec` | `proforma.rs:65–78` |
| Settings | credentials only | `:69–71`; correctly no `valaszVerzio`, PDF or other settings |
| Header | `szamlaszam? → rendelesszam?` | `:72–77`; `ProformaSelector` selects exactly one (`:14–30`) |
| Selection scope | Number: one; order: **all** matches | Explicit at `:1–6,25–27,35–43`; no latest-query narrowing or paid-state gate |
| Success | Dedicated `xmlszamladbkdelvalasz`, `sikeres=true` | Returns `()`, `:82–87`; no fabricated count or number list |

The XSD permits both selection fields or neither structurally; the enum intentionally narrows it to one usable selection. S9's examples document either selector; no guaranteed precedence for supplying both was found. S15 additionally says a failing member of batch deletion causes rollback. The crate neither implements a client-side loop nor claims to verify that server rollback; it submits one operation and reports its acknowledgement.

## Response coverage and exact precedence

### Envelope fields

S3/S11 declare `sikeres → hibakod? → hibauzenet? → szamlaszam? → szamlanetto? → szamlabrutto? → kintlevoseg? → vevoifiokurl? → pdf?`. S6 has the same sequence **without PDF**; S10 has only `sikeres → hibakod? → hibauzenet?`.

| Wire field/channel | Implementation and assessment |
|---|---|
| `sikeres` | Required unique scalar boolean, supports true/false/1/0; `xml.rs:453–475`. No default success for a missing verdict |
| `hibakod`, `hibauzenet` | Read before success payload; false without code is `ErrorCode::Absent`, not invented zero (`xml.rs:477–496`). Unusable optional diagnostic cannot erase a readable refusal |
| `szamlaszam` / `szlahu_szamlaszam` | Body nonblank value first, decoded header second, trimmed on both (`envelope.rs:120–133,329–332`). Create/storno require a number except requested create preview; credit mismatch is M-01 |
| `szamlanetto` / `szlahu_nettovegosszeg` | Body first, header fallback (`envelope.rs:151–168,229–234`); exposed on created document and credit balance |
| `szamlabrutto` / `szlahu_bruttovegosszeg` | Same (`:235–240`); negative and zero retained |
| `kintlevoseg` / `szlahu_kintlevoseg` | Same (`:241–246`); latter header omitted from current page table but supported by account observations |
| `vevoifiokurl` / `szlahu_vevoifiokurl` | Body first; XML entity decoding only. Header decoded once (`:135–143,247`; `wire.rs:239–249,343–350`) |
| `szlahu_fizetesmod` | Decoded once, open `PaymentMethod`, exposed on create/storno and credit (`envelope.rs:248,321–327`; `credit_entry.rs:275`). No invented XML counterpart |
| `szlahu_id` | Auxiliary header-only `Option<i64>`; malformed/negative is absent (`envelope.rs:334–345`). Not part of S3/S11's current table; observed document-id provenance preserved |
| `pdf` | Optional base64 decoded to bytes. Absent/blank is `None`. Malformed on ordinary success errors; numbered 56 drops invalid PDF (`envelope.rs:145–148,249`). Credit consumes shared raw Body but does not decode PDF or expose it |
| Unknown elements/namespaces | Well-formed extensions allowed; foreign fields cannot supply verdict/identity. Complete-root, XML lexical and namespace checks precede payload extraction (`xml.rs:156–355`) |

The reader does not enforce response XSD element order: this is intentional tolerant parsing, not request-writer noncompliance. Ordinary malformed nonblank body money never falls back to a valid header. Blank body money is absent; a present blank header is malformed. Headers accept comma or dot decimal separator and exponent without grouping; XML uses its separate numeric grammar. Finite exact Decimal representability is a library policy narrower than XSD `double`, which includes non-finite/extreme values. README `:227–237,253–276,394–401` documents these decisions.

### Verdict/status order

The following is actual behavior, not a vendor-specified conflict-resolution rule:

1. Nonblank decoded `szlahu_down` → `ServiceUnavailable`.
2. Nonblank raw `szlahu_error_code` → operation judges that code. All codes fail except the create/storno numbered-56 exception.
3. With no such error header, a supplied non-2xx status → `HttpStatus`, **before** reading a body-only error or success.
4. Then require a complete expected XML envelope and read the body verdict; a non-56 body refusal wins over header 56.
5. Only then interpret payload and number/PDF requirements.

Locations: `wire.rs:262–310`, `xml.rs:503–539`, `envelope.rs:179–251`. Thus a body-only 56 at HTTP 500 remains `HttpStatus`; header 56 can be considered even at HTTP 500. A normal number header does not bypass status. Conflict precedence is explicitly documented in README `:310` and is not contradicted by a fresh source rule specifying another order.

### Numbered 56, preview and storno no-op

- Numbered header/body 56 yields the document with `notification_delivery_failed=true`. Numberless 56 remains `Api(InvoiceNotificationDeliveryFailed)`, hence unknown outcome. The S17 implementation independently corroborates this distinction; the generic S3/S11 statement that error headers omit numbers is not grounds to delete it. D6 failed to trigger code 56; it is **not live-tested evidence** of its exact shape.
- The parser reads the body verdict separately from optional payload. Nested/duplicate optional totals/PDF cannot hide a readable non-56 refusal. A unique body-only number can survive malformed optional structure under 56 (`envelope.rs:288–318`). A malformed XML envelope cannot be promoted by a numbered header; plain notification text or an empty body can (`:188–196,254–257`).
- A numberless successful create is `Preview` **only when the request set `preview_pdf=Some(true)` and a PDF exists**. Otherwise missing number/PDF errors remain. A numbered reply is `Issued`, even if a preview was requested: the library does not conceal a returned document (`invoice.rs:923–935`). S12 documents non-issuing preview; detailed real preview response shape was not live-probed.
- `StornoInvoice::parse` reports the numbered wire reply. Same-number, positive-total echoes remain successful wire responses, and `CreatedInvoice::reverses` returns false. This preserves B5's no-op behavior; it is not a missed refusal. `reverses` is explicitly a heuristic: changed number plus known gross ≤0, false otherwise (`envelope.rs:61–87`). Missing/positive gross with a changed number requires identity verification, not a conclusion that no reversal happened.

## Preserved live-tested exceptions and intentional policy

The provenance below is `docs/szamlazz-hu-behaviour.md` at HEAD, whose `:3–28` scopes the evidence to one TEST account on the listed days. Original probe logs are explicitly **not in this repository** (`:11–24`). This review read the recorded observations; it did not independently replay vendor calls. Older worker design-consequence prose is not promoted into a new vendor guarantee.

| Exception/policy | Precise provenance | Review decision |
|---|---|---|
| Repeat storno returns existing SS, no second document | B4-repeat-storno/B4-query-order-after-repeat, 2026-09-03; behavior `:86` | Preserve `storno.rs:30–36`, not invoice-create-style duplicate error handling |
| Proforma/delivery-note storno succeeds but changes nothing | B5-storno-proforma/B5-storno-delivery-note, 2026-09-03; `:87` | Preserve wire success and helper distinction (`storno.rs:43–46`) |
| Storno external id attaches to the new SS, not original; repeat ignores new id | B6, B4x; 2026-09-03; XPRB-P4, 2026-09-06; `:69–70`; P48-P6 `:95` | Preserve `storno.rs:86–98` despite S1 wording |
| Storno issue date other than today refused as 352 on paper | B3-storno-earlier-kelt, 2026-09-03; `:90` | Preserve omission recommendation; XSD date optional is not a guarantee any date is accepted |
| Omitted fulfillment date inherits original; explicit wrong date accepted silently | P48-P1/P2/P3/P4/P5, 2026-09-06; `:92–94` | Preserve `storno.rs:108–119`. The raw client exposes the date; worker derives it separately |
| Storno form is request `eszamla`, mismatch accepted | P73-EE/EP/PE/PP, 2026-09-07, repeated twice; `:18–24,97–98` | Preserve explicit request flag and advice to derive original appearance |
| Storno removes original credit entries; SS outstanding full negative gross | B8, 2026-09-03; `:80` | Preserve response reading; do not infer remaining balance from old credits |
| Deletion success has no headers; repeat/missing/consumed 335; paid D deletable | D1/D2/D3/D4, 2026-09-03; `:105,109–110` | Preserve dedicated verdict parser and caller-owned paid-state policy |
| Replace leaves latest set; additive appends; five entries accepted in different query order | D7, 2026-09-03; `:133–134` | Preserve false default and no deduplication or entry-order promise |
| Credit on reversed invoice is body-only 463 | D8, 2026-09-03; `:135,141` | Preserve body verdict parsing; do not require error headers |
| Empty replacing credit disallowed | README `:375`; behavior `:211–215`; explicitly **not probed** | Intentional library policy. Clearing is inferred from replace semantics, not established by an executed zero-entry request |
| Zero-total reversal predicate | Behavior `:79,176–179`; `envelope.rs:79–82` | Deliberate ≤0 comparison, not proof that zero-total originals are accepted live |
| Metadata/header extensions | A1/D1/D3, behavior `:144–145`; P60-E3 `:160` for comma monetary header | Preserve `szlahu_id`, outstanding and comma support despite incomplete generic header tables |

## Source inconsistencies and unresolved ambiguities

These are **not additional confirmed defects**.

1. **Storno external id:** S1 EN/HU calls it a reference to the original; S2 describes later query identity; B6/XPRB establish assignment to SS when the original number is present. Behavior with missing/empty original number or conflicting identifiers is not established. The schema requires `szamlaszam`; no external-id-only storno interface should be invented from this ambiguity.
2. **Number/total headers alongside an error:** generic pages say omitted; S17 proves the first-party numbered-56 exception. Exact header/body/status combinations remain unobserved live. README `:151,369` correctly qualifies that evidence.
3. **Body `sikeres=true` plus `hibakod=56`:** a synthetic check returned `Issued` with `notification_delivery_failed=false`. `xml::Verdict::api_error` intentionally ignores codes on a successful verdict (`xml.rs:481–484`). S17 classifies notification failure by code, but S3/S11 describe codes on false verdicts and do not establish this true-plus-code wire combination. Record as an unresolved warning-classification policy, not a demonstrated vendor response bug.
4. **Ambiguous body identity plus usable header identity under 56:** a synthetic response with body numbers I-2 and I-3, header number I-4 and code 56 returns I-4. The optional-payload failure falls back to empty Body (`envelope.rs:208–212`), then uses header identity. No ambiguous body number is selected, consistent with the narrow README wording at `:235–237`; however the complete response is contradictory. Decide explicitly whether that header remains sufficient evidence or the whole result must be unknown. Fresh sources define no conflict priority and do not show this malformed body in production.
5. **Current success examples are not executable fixtures verbatim:** S3/S11 have literal unescaped `&` in `vevoifiokurl` and an abbreviated `....` inside base64. S6 has the unescaped URL too. Rejecting those literal examples is correct XML/base64 handling, not a compliance bug. Any normalized reproduction must identify the `&amp;` repair and replacement/omission of abbreviated PDF explicitly. The existing upstream tests distinguish source defects from normalized controls.
6. **Header metadata parity claim:** S3/S6/S11 say the same data is in XML, but no `fizetesmod` element appears in their response XSDs. Header-only payment-method exposure is source-backed; adding an undocumented body spelling would be speculation. Exact encoding of payment-method/URL headers is also underspecified in the current page tables; existing decode-once behavior is documented library policy. S17's PHP URL handling uses `rawurldecode` at `InvoiceResponse.php:137`, whereas the crate treats `+` as space; no captured literal-plus URL header settles which interpretation is required.
7. **Deletion translation and rollback:** HU S9 says “a törlés az összes díjbekérőre vonatkozik” (deletion applies to all proformas); EN S9 omits it. S15 corroborates multiple deletion and claims rollback if one fails. That rollback is a first-party statement, not a live atomicity test or proof that a lost acknowledgement means nothing was deleted.
8. **Preview details:** S12 establishes no actual document, but does not demonstrate every header/body combination. In particular, live acceptance of combined `simpleItems` and preview ordering is unresolved as README `:408` states; this report does not reopen the create-request writer's intentionally selected order. No rule was found requiring `szamlaLetoltes=true` alongside `elonezetpdf=true`.
9. **S6 number optionality:** the published schema allows omission; its normal example has a number. A vendor confirmation that every successful version-2 credit acknowledgement always supplies a number would narrow M-01's practical exposure, but is absent from the fetched sources. Do not confuse a shared success/error XSD with proof that numberless replies have actually occurred.
10. **Specialized settings:** aggregator/guardian are schema-declared and correctly emitted. Their contracted server semantics are not explained by these pages; syntactic compliance does not establish an integration's entitlement or operational behavior.

## Executed checks

- `git rev-parse HEAD` → exact pinned SHA. Initial `git status --short` showed no modifications under `crates/szamlazz-agent`; unrelated working-tree changes were present.
- `cargo test -p szamlazz-agent --locked --test response_headers --test upstream` → **24 passed, 0 failed** (13 response-header tests + 11 upstream/outline tests). These tests use repository fixtures, not freshly downloaded account responses. They cover existing header precedence, monetary grammars, malformed optional diagnostics/payload, numbered 56, request examples and current source-defect controls.
- Scratch program: `cargo run --manifest-path /tmp/opencode/mutations-2ba5-repro/Cargo.toml --offline` → success, exercising public current-code parsers only. Relevant dependency versions match the workspace lock: quick-xml 0.42.0, serde 1.0.229, rust_decimal 1.43.0, jiff 0.2.35, thiserror 2.0.20. Rustc was `1.98.0 (88d9e12ae 2026-08-18)`.
- Fresh S17 ZIP was fetched and inspected in memory by `python3 /tmp/opencode/mutations-2ba5-source.py`; package code was not executed.

Scratch parser results:

| Case | Observed result |
|---|---|
| Numberless credit, populated replace | `Parse(Missing("szamlaszam"))` |
| Numberless credit, populated additive | Same |
| Credit verdict-only body + number header | `Ok(InvoiceBalance)` |
| Body false/56 + number + invalid gross | `Ok(CreatedInvoice)`, warning true, gross absent |
| Body false/56 without number | `Api(InvoiceNotificationDeliveryFailed)` |
| Header 56 + number, body false/3 | `Api(InvalidCredentials)` |
| Duplicate body numbers + numbered header 56 | Header number selected; ambiguity 4 above |
| Body true/56 + number | Issued, warning false; ambiguity 3 above |
| Same-number positive-total storno echo | Wire success, `reverses=false` |
| Numberless success + PDF, preview not requested | `Missing("szamlaszam")` |
| Same body, preview requested | `Preview`, decoded bytes `%PDF-` |
| Preview requested, no PDF/number | `Missing("pdf")` |
| Order-number deletion serialization | Only `rendelesszam` in header; one multipart operation |
| Headerless successful deletion XML | `Ok(())` |

No live tests, full crate suite, or external XML-schema validator were run by this review. Exact request sequences were manually compared against both inline schemas and the two successfully downloaded XSDs; the lead's full-suite result should be attached to the aggregate review rather than inferred from these targeted passes.
