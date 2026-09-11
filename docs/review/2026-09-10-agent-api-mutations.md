# Számla Agent mutation and structured-reply review

**Reviewed revision:** `382cf7615aca1d64a05c7c3f77110248dde51950`  
**Review / public-source acquisition date:** 2026-09-10  
**Result:** one confirmed P3 robustness defect; no confirmed P0/P1/P2 defect in the assigned scope. The mutation request writers cover the current documented fields and order. The relevant earlier monetary-header, response-field and storno-heuristic fixes are present.

## 1. Scope and evidence boundary

Reviewed all fields, constructors, writers, parsers and local tests in:

- `crates/szamlazz-agent/src/ops/storno.rs`
- `crates/szamlazz-agent/src/ops/credit_entry.rs`
- `crates/szamlazz-agent/src/ops/proforma.rs`
- `crates/szamlazz-agent/src/ops/envelope.rs`

Followed the shared response boundary through `xml.rs`, textual/numeric header access, `Pdf::from_base64`, and `invoice.rs:923–935` for preview/result classification. Invoice request construction, queried XML/PDF models, receipts, taxpayer, and general transport/error-catalogue review belong to the other reviewers. Tests spanning those surfaces were run where they exercise the shared parser, without extending this report's completeness claim to those operations.

The requested SHA was HEAD at entry. An unrelated commit advanced HEAD to `7da23b44cd006783eb47e60aa54ab8969bdd1c2c` during review. A comparison against the requested SHA showed **no changes to the agent crate, `fixtures/SOURCES.md`, `docs/szamlazz-hu-behaviour.md`, or the vendor-question document**. The concurrent Cargo changes concern externalization of the Restate e2e harness. This report's code lines refer to the requested SHA, not a new review baseline. No production files or repository tests were edited. The only repository file created by this review is this report.

Sources were evaluated against:

- `CONTEXT.md`: Storno invoice, Credit entry, Response version, Invoice appearance, Found/Issued document, Outcome class, Unconfirmed, and the explicit source/behavior qualifications.
- `docs/szamlazz-hu-behaviour.md`: especially lines 59–110, 129–145, 153, 160–161, 174–215. These are bounded observations on one test account; the underlying historical exchange logs are not in the repository.
- `fixtures/SOURCES.md:34–168,193–247`: historical versus current acquisitions, project-authored golden files, modified schemas, and defective published examples.
- `docs/research/2026-09-10-agent-vendor-questions.md`: draft, **not sent**, and not a vendor answer. Combined preview/simple-items ordering and template naming remain unresolved.
- Earlier untracked `docs/review/2026-09-09-agent-api/FINAL.md` and relevant judgment sections: candidate history only, not evidence that a finding still exists.

All network accesses were unauthenticated documentation/schema/package GETs. **No live Számla Agent call, account query, issue, reversal, credit entry, deletion or email send was made.**

## 2. Confirmed actionable finding

### M1 — Optional-payload deserialization can erase a readable body refusal under header code 56

**Severity:** P3 (response robustness / contradictory-input handling).  
**Confidence:** High in the implementation defect and reproduction; High in the bounded recommendation. **Live occurrence: unobserved.** The triggering reply combines contradictory header/body codes with a structurally malformed optional payload; it is not asserted to be a normal vendor response.

**Exact locations:**

- `crates/szamlazz-agent/src/ops/envelope.rs:188–200`: every error from `parse_envelope` is discarded when the header error is 56, replacing the body with `Body::default()` and losing the verdict.
- `crates/szamlazz-agent/src/ops/envelope.rs:272–278`: the verdict is successfully deserialized first, but the following payload deserialization can fail and discard that already-known verdict.
- `crates/szamlazz-agent/src/ops/envelope.rs:203–242`: the header number then yields `Reply::Issued` with `notification_delivery_failed=true`.

**Official contract / quotes:**

- [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response#error-handling) and [invoice-generation response](https://docs.szamlazz.hu/agent/generating_invoice/response#error-handling) specify the version-2 error form: **“`<sikeres>false</sikeres>` with `<hibakod>` and `<hibauzenet>` in the XML response.”** Their unsuccessful example carries `<hibakod>3</hibakod>`.
- Their response XSD declares `<element name="szamlabrutto" type="double" maxOccurs="1" minOccurs="0">`: this amount is optional and is not the verdict.
- [First-party notification handling](https://docs.szamlazz.hu/php/valasz-feldolgozas#handling-custom-errors) explicitly distinguishes successful issuance from notification failure. The freshly fetched [PHP 2.12.4 source](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), `src/szamlaagent/Response/InvoiceResponse.php:319–322`, says **“Ha a számlaértesítő kézbesítése sikertelen volt, de a válasz tartalmaz számlaszámot, akkor a számla kiállítása sikeres.”** Translation: if notification delivery failed but the reply contains an invoice number, issuance succeeded.

The official pages do **not** define precedence for contradictory codes. The defect is narrower: the crate already chooses to preserve a readable non-56 body refusal (`envelope.rs:197–200`), and unrelated payload shape changes reverse that decision. No new universal header/body precedence rule is inferred from the docs.

**Offline reproduction:** use `StornoInvoice::new("I-1").parse(&raw)` with headers:

```text
szlahu_error_code: 56
szlahu_szamlaszam: I-2
```

and this complete, well-formed XML document:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres>
  <hibakod>3</hibakod>
  <hibauzenet>login refused</hibauzenet>
  <szamlabrutto><bad/></szamlabrutto>
</xmlszamlavalasz>
```

Observed result: `Ok(CreatedInvoice { invoice_number: "I-2", notification_delivery_failed: true, ... })`. Two sibling `<szamlabrutto>` elements reproduce the same result. In contrast, omitting the amount or supplying `<szamlabrutto>bad</szamlabrutto>` returns the expected `Err(Api(InvalidCredentials))` and preserves `login refused`.

This exercises the actual public operation parser, not a copy of the implementation. The scratch source is `/tmp/opencode/mutations-review-probe/src/main.rs`; it contains all four cases and assertions. The case was also run directly against an rlib from the focused build. It requires no credentials or HTTP call.

**User impact:** a caller can receive a successful issued-document result despite a recognized explicit refusal in the response body. That can suppress operator attention and cause the caller to record completion using only the conflicting header number. This applies to creation and storno through their shared envelope. Credit-entry registration and deletion do not take this numbered-56 promotion path.

**Minimal recommendation:** retain the successfully parsed verdict independently of optional payload deserialization. Apply the existing non-56 body-error check before any payload-error fallback. Preserve the header-56/non-XML-body fallback and malformed optional numeric/PDF softness for genuine numbered-56 answers. Add the nested and duplicate optional-amount cases beside the existing ordinary-refusal and malformed-text controls. Do not require gross totals, drop storno echoes, or add a full XSD validator to fix this ordering problem.

## 3. Coverage inventory: mutation requests

In the tables, `?` means optional in the vendor schema, not a promise that omission and an empty value have identical server behavior. Every listed child has `maxOccurs=1` except the explicitly repeated credit-entry blocks. All three writers emit XML 1.0/UTF-8 with the correct root/default namespace. They omit `xsi:schemaLocation`, which is a validation hint rather than an operation field. Values pass through the shared escaping writer.

### 3.1 Storno (`xmlszamlast`)

Official source set: [request](https://docs.szamlazz.hu/agent/reversing_invoice/request), [current example + inline XSD](https://docs.szamlazz.hu/agent/reversing_invoice/xml), [separate XSD page](https://docs.szamlazz.hu/agent/reversing_invoice/xsd), [download](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd).

| Ordered wire fields | Current representation/default and result of inspection |
|---|---|
| Multipart `action-szamla_agent_st` | Exact match, `storno.rs:162–169`. |
| Root children `beallitasok`, `fejlec`, `elado?`, `vevo?` | Exact order, lines 171–206. Both optional containers are present and empty by default, valid under the XSD; no omission-equivalence claim needed. |
| `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?` | Shared credentials writer emits username/password or agent key in schema order (`xml.rs:456–465`). Both credential alternatives fit the optional sequence. |
| `eszamla`, `szamlaLetoltes` | Required booleans explicitly written; both default false. Paper is the constructor's choice, not automatic inheritance from the original. |
| `szamlaLetoltesPld?` | Optional `u8`, omitted by default; narrower than XSD `int`, but current docs say **“our system no longer processes it.”** No useful capability defect established. |
| `aggregator?`, `guardian?`, `valaszVerzio?`, `szamlaKulsoAzon?` | Optional string/bool, fixed response version `RESPONSE_VERSION` = 2, optional external id. Correct specialized order, lines 178–183. Do not substitute the invoice-create settings order. |
| Header `szamlaszam`, `keltDatum?`, `teljesitesDatum?`, `megjegyzes?`, `tipus?`, `szamlaSablon?` | Required original number; optional civil dates/comment; fixed `SS`; optional open template token. Exact order, lines 185–194. Dates default absent. |
| Seller `emailReplyto?`, `emailTargy?`, `emailSzoveg?` | Every supported child exposed through `SellerEmail`, lines 195–201; omitted individually when None. |
| Buyer `email?`, `adoszam?`, `adoszamEU?` | All three exposed and ordered, lines 202–206. The XSD notes that missing original buyer tax numbers may be supplied here; these are not arbitrary original-invoice editing fields. |

**Result:** no missing current request field or ordering defect. The external-id meaning is qualified in `storno.rs:86–98` against recorded behavior. `issue_date`, `fulfillment_date`, and appearance guidance preserve the live-backed behavior rather than enforcing older example values. Current simplified-image docs say storno **“inherits the state of the original document”**; a new storno `simpleItems` request flag is not missing from this schema.

### 3.2 Register credit entry (`xmlszamlakifiz`)

Official source set: [request](https://docs.szamlazz.hu/agent/credit_entry/request), [current example + inline XSD](https://docs.szamlazz.hu/agent/credit_entry/xml), [separate XSD page](https://docs.szamlazz.hu/agent/credit_entry/xsd), [download](https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd), [Hungarian example/XSD](https://docs.szamlazz.hu/hu/agent/credit_entry/xml).

| Ordered wire fields | Current representation/default and result of inspection |
|---|---|
| Multipart `action-szamla_agent_kifiz` | Exact match, `credit_entry.rs:213–228`. |
| Root `beallitasok`, `kifizetes` × 0–5 | Exact order. `CreditEntries` bounds constructors, push and serde to five. |
| Credentials, `szamlaszam`, `adoszam?`, `additiv`, `aggregator?`, `valaszVerzio?` | Correct order, lines 230–237. Required number and explicit additive=false; optional issuer tax number/aggregator; fixed response version 2. |
| Entry `datum`, `jogcim`, `osszeg`, `leiras?` | Required civil date, open `PaymentMethod`, finite exact Decimal, optional description; exact order, lines 239–244. No sign restriction or automatic rounding invented. |
| Replace/additive behavior | Current inline annotation: **“If true, former credit entries are retained; otherwise they are replaced.”** Constructor selects replacement. Empty replacement is deliberately refused at validation; empty additive is representable. |

**Result:** no missing current request field or ordering defect. The Hungarian annotation supports issuer-tax-number wording: `ha megadod a kiállító adószámát, a rendszer a bejövő kifizetést a megfelelő számlához rendeli` (provide issuer tax number to associate the incoming credit entry with the appropriate invoice). The English example's “incoming receipt” wording is not evidence of a different receipt operation.

### 3.3 Delete proforma (`xmlszamladbkdel`)

Official source set: [request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), [current examples + inline XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml), [separate XSD page](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xsd), [Hungarian batch-scope note](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml).

| Ordered wire fields | Current representation/default and result of inspection |
|---|---|
| Multipart `action-szamla_agent_dijbekero_torlese` | Exact match, `proforma.rs:60–67`. |
| Root `beallitasok`, `fejlec` | Both required and correctly ordered, lines 69–77. |
| Settings credentials | All schema children supported through the shared credential alternatives. No response-version field exists here; none is written. |
| Header `szamlaszam?`, `rendelesszam?` | `ProformaSelector` emits exactly one. Current examples demonstrate both forms; the model deliberately excludes both/neither even though the raw sequence does not express that exclusivity. |

**Result:** no missing current request field or ordering defect. The documented all-matches effect is already explicit in `proforma.rs:1–2,25–27,35–41`. Hungarian source: **“Ha azonos rendelésszámmal több díjbekérő is van a számlázási fiókban, akkor a törlés az összes díjbekérőre vonatkozik.”** Translation: if several proformas have the same order number in the account, deletion applies to all of them. Paid-proforma deletion remains supported and explicitly documented as a bounded observation.

## 4. Coverage inventory: results, precedence and previews

Primary sources: [create response](https://docs.szamlazz.hu/agent/generating_invoice/response), [storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response), [credit response](https://docs.szamlazz.hu/agent/credit_entry/response), [shared response download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd), [deletion response and inline XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response).

### 4.1 Structured invoice/storno/credit envelope

| Wire field / behavior | Current interpretation and coverage |
|---|---|
| Root `xmlszamlavalasz`, namespace `http://www.szamlazz.hu/xmlszamlavalasz` | Verified before serde. Namespace aliases accepted; foreign namespace fields/subtrees do not provide protocol identity or values. Whole-document completion checked. |
| `sikeres` (required boolean) | Shared `Verdict` reads true/false and 1/0; missing verdict rejected. Empty is retained legacy false/error behavior, not success. |
| `hibakod?`, `hibauzenet?` | Failed verdict becomes typed API error; no code is represented as `Absent`, not fabricated. Body-only errors supported. Current fixed non-56 refusal behavior has M1's payload-error exception. |
| `szamlaszam?` | Body first, then URL-decoded `szlahu_szamlaszam`; both trimmed and blank treated absent (`envelope.rs:120–133,289–292`). Issued/create/storno/credit results require a usable number. Credit does not invent the requested number as response evidence. |
| `szamlanetto?`, `szamlabrutto?`, `kintlevoseg?` | Optional exact Decimals. Missing/empty/blank body falls back to `szlahu_nettovegosszeg`, `szlahu_bruttovegosszeg`, `szlahu_kintlevoseg` respectively. Finite dot/scientific XML forms; unrepresentable values refused instead of silently rounded. No total recomputation or sign-based success gate. |
| Monetary header grammar | Raw, not percent-decoded. Ungrouped comma or dot decimal, optional exponent and HTTP space/tab padding; mixed/repeated separators and grouping rejected. Valid body masks a bad header; malformed nonblank body is not rescued by a valid header. Missing header is None; present blank is a parse error on ordinary success. |
| `vevoifiokurl?` | Optional body-first URL, then decoded `szlahu_vevoifiokurl`. XML entity decoding only on body; no extra URL decoding. Optional envelope text retains scalar-style trimming, distinct from queried business-text preservation. No observed URL defect established from surrounding whitespace. |
| `pdf?` | Present on create/storno schema; not on credit's operation-specific inline schema. Optional `Pdf`, standard base64 with whitespace removal. Not mandatory solely because `download_pdf=true`; schema itself makes it optional. No PDF signature validation promised. |
| `szlahu_fizetesmod` | Now exposed as optional open `PaymentMethod` on both `CreatedInvoice` and `InvoiceBalance`, using one encoded-text reader. No invented XML payment-method field. Unknown strings preserved. |
| `szlahu_id` | Auxiliary optional nonnegative i64 on `CreatedInvoice`, lenient on absent/invalid/negative header. Historical captures establish it as document id. No response-body element invented. |
| Successful numbered result | `CreatedInvoice` for storno; `CreationOutcome::Issued` for create. May echo an already-existing document; does not prove issuance by this call or reversal identity. Credit returns `InvoiceBalance`, not a new invoice. |
| Ordinary bad optional amount/PDF | Parse error, hence not evidence that the write failed to land. Deliberate existing policy; sparse omitted fields remain accepted. |

The published header tables list number, net, gross, error, error code, payment method, and customer URL. They omit historical auxiliary `szlahu_id` / `szlahu_kintlevoseg`; omission from the table is not grounds to remove supported observed headers. The XML schemas contain no VAT-total or currency field in these replies, so not returning them is not an omission defect.

### 4.2 Numbered code 56

- Nonblank `szlahu_down` has precedence; otherwise header API error precedes non-2xx status, then the operation reads the body. This is existing library policy, not a newly inferred vendor precedence guarantee.
- Header 56 is allowed through to the operation, including non-2xx and non-XML bodies. A number is still required before it becomes an issued result.
- Body-only failed verdict 56 with a number is likewise accepted under ordinary successful/unspecified HTTP status. Other readable body errors are retained, subject to M1.
- Numbered 56 sets `notification_delivery_failed=true`; invalid optional amounts or base64 become None, preserving issuance evidence. A malformed amount is not required metadata.
- Unnumbered 56 remains an error, not a preview or evidence that nothing was issued.
- Credit and deletion keep ordinary verdict handling; a number plus 56 is not promoted into successful credit registration or deletion merely because issuing operations have that exception.
- The current EN/HU general error catalogues fetched here do not list 56. The specific handling remains supported by first-party PHP documentation/source and historical project policy. The behavior notes explicitly say attempts to trigger it failed; no live 56 capture is claimed.

The fresh PHP ZIP hash is `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. It was inspected in memory, without executing PHP. `InvoiceResponse.php:14–17` names notification failure as 56; lines 319–322 apply the number condition. This is first-party implementation evidence, not proof of every header/body combination emitted by the service.

### 4.3 Previews

[Current generating-invoice inline XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml) describes `elonezetpdf`: **“Preview PDF of the document (no actual document is created).”** Its response schema makes `szamlaszam` and `pdf` independently optional; it does not supply a dedicated complete preview response example.

Current classification (`envelope.rs:205–211,264–268`; `invoice.rs:923–935`):

| Response | Result |
|---|---|
| Successful numbered reply, even to a preview request | `Issued`; never hides reported issuance as preview. |
| Success without number, preview requested, valid PDF | `Preview(InvoicePreview)`; no `CreatedInvoice` manufactured. |
| Success without number, preview requested, PDF absent/blank | Missing PDF error. |
| Success without number, preview not requested | Missing invoice-number error. |
| Success without number to storno | Missing invoice-number error; storno has no preview request field. |
| Unnumbered 56 | API error; does not enter the preview arm. |

No confirmed preview-result defect found. Combined preview/simple-items request ordering remains the existing vendor question, owned by invoice requests. The current report does not infer live preview acceptance from a synthetic response test or the locally modified request XSD.

### 4.4 Deletion reply

`proforma.rs:82–87` uses its own root/namespace: `xmlszamladbkdelvalasz` / `http://www.szamlazz.hu/xmlszamladbkdelvalasz`. All three documented result fields are consumed: required `sikeres`, optional `hibakod`, optional `hibauzenet`. Success returns `()` because the protocol provides neither deleted number list nor count. The documented failed example's 335 is preserved as `ProformaNotFound`. Critical text/HTML is a bounded unexpected-body error, not success. Header-free success, wrong namespace and body-only failure cases remain handled.

## 5. Preserved deviations and explicit capability boundaries

| Difference from a simplistic schema/example reading | Disposition / evidence |
|---|---|
| Repeated storno returns existing SS, not a new document/error | Preserve. Behavior notes B4, lines 86,95; scoped tests retain the existing number/id/negative totals. |
| Storno of proforma/delivery note reports success and echoes original | Preserve. B5, lines 87–89; do not reject the reply for positive totals or require changed number during parsing. |
| `reverses()` false with missing/positive gross | Inconclusive heuristic, already corrected. `envelope.rs:61–87`; no I/O or new gross requirement. Zero comparison is policy, not live evidence. |
| Storno external id attaches to created SS; repeat ignores new id | Preserve. B6/XPRB/P48, behavior lines 69–70,95; current request docs' optional lookup wording conflicts. Original invoice number remains required. |
| Storno issue/fulfillment dates and appearance | Preserve one-account observations: omitted fulfillment repeats original; explicit mismatching dates accepted; issue date other than today rejected; appearance comes from request rather than automatic original matching. No newly inferred server gate. |
| Fully paid proforma can be deleted | Preserve. Behavior D3, line 110; current docs correctly put paid-retention policy on caller. |
| Sparse totals and header-only auxiliary metadata | Preserve optionality and fallback. Current response XSD also marks totals optional; no need for an arbitrary required amount. |
| Body-only credit refusal 463 / header-free deletion success | Preserve. Behavior lines 109,135,141; current parsers do not depend on headers alone. |
| Empty replacement credit request | Explicit unsupported capability through validated built-in request, not a defect. XSD permits zero entries, but the crate intentionally refuses empty replace; behavior lines 211–215 say actual clearing was not probed. Empty additive behavior is a local accepted-input policy, not a claimed live result. |
| Response version 1 text/raw PDF | Deliberately unsupported by these built-in requests: create/storno/credit always request 2. Rejecting a version-1 body is not a version-2 defect. Deletion has no selectable version. |
| Arbitrary storno `tipus`, external-id-only storno | Not supported; no useful missing capability established. Operation writes `SS`; current schema and request prose require original number. |
| Credit balance omits auxiliary document id | Existing response projection boundary; no operation-specific documented requirement or consumer need established to reopen the prior review's explicit exclusion. |
| Optional email controls on storno | All actual schema fields exposed; no storno `sendEmail` field exists in these sources. Delivery, partial overrides and original-email inheritance remain unverified; do not invent controls or mandatory fields. |
| Request Date/Decimal domains narrower than all XSD lexical possibilities | Deliberate civil-date / finite-money representation. No demonstrated valid business operation requires timezones on outgoing dates, nonfinite money or unrepresentable precision. No capability finding added merely to cover every XSD value. |

## 6. Source conflicts, limits and unknowns

1. **Current and legacy schema pages coexist.** Current request/response/XML pages identify site build `v202608271632`; separately reachable `/xsd` pages identify `v202606031507`. Both were fetched. Their relevant storno/credit/deletion field sequences agree; the build string is not a freshness guarantee for each claim.
2. **Published success examples are not ready-to-parse exchange captures.** Fresh create/storno/credit response pages contain raw `&` inside `vevoifiokurl`; create/storno PDF examples contain `....`. They are not valid complete XML/base64 evidence. Existing dated corpus tests explicitly escape the URL and substitute synthetic PDF bytes, while retaining defective originals. Passing those tests establishes supported fields, not a real vendor PDF.
3. **Positive storno sample is not reversal evidence.** The current page uses positive `30000/38100` and outstanding zero; this formatting sample provides no original/type/reference proof. Do not remove negative-total storno support or assert an observed negative-original reversal from it.
4. **Header/body duplication prose overstates the schemas.** The response pages say **“With an XML response, the same data is also in the XML body”**, but their inline XSDs have no payment-method element. Keep the documented header reader; do not guess `fizetesmod`/`fizmod` in `xmlszamlavalasz`.
5. **Error headers versus numbered 56.** Pages say **“If error codes are present in the header, invoice number and amounts are omitted.”** The specific first-party numbered-56 rule is a documented implementation exception worth preserving. Exact service combinations remain unobserved.
6. **Conflicting response channels have no vendor precedence contract.** Body-first values and the existing refusal ordering are library policy. M1 concerns failure to maintain that policy, not a claim that the server emits mismatching ids/totals or that every malformed response can be classified definitively.
7. **Proforma download links still fail.** Both linked URLs returned 404 on fresh GET: `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd` and `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd`. Inline request/response schemas provide current evidence. No substitute schema was fabricated.
8. **English deletion page omits the Hungarian batch note.** The HU note explicitly establishes all-matches scope and is already reflected in code documentation. EN singular phrasing does not justify narrowing the operation.
9. **Template and combined-preview questions remain open.** The existing vendor-question draft records API/knowledge-base template-label disagreement and inline/download/PHP preview ordering disagreement. It is not an answer. No blind schema replacement or changed writer order recommended here.
10. **Historical behavior notes contain superseded design commentary.** In particular the older line-87 `false → NotStornoable` implication must be read with line 79, current `CONTEXT.md`, and the #196 agent guidance. The underlying same-number no-op observation stands; the obsolete broad implication is not grounds to reopen the already-fixed agent heuristic docs.
11. **No live results for unresolved edge cases.** Notification 56, zero/negative originals, arbitrary storno original kinds, partial email behavior, external-id-only targeting, conflicting header/body replies, and combined preview options were not probed here. Source uncertainty is not counted as an implementation failure.

Two exploratory old/guessed paths returned 403 (`/agent/basics/errors`, `/php/invoice/response`); they were not used as sources. Their working current counterparts were fetched instead. No conclusion relies on inaccessible content.

## 7. Verified prior fixes at the requested HEAD

| Earlier candidate | Current evidence and disposition |
|---|---|
| FINAL F3: comma monetary fallback rejected | Fixed. `envelope.rs:314–334` normalizes the header separator and uses exact numeric parsing. `response_headers.rs:265–303,372–448` freshly passed comma/sign/exponent, missing/blank, body precedence and numbered-56 controls. Historical comma emission remains bounded to P60-E1/E3; combined sparse/comma input is synthetic. |
| FINAL E.E4: create/storno payment method discarded | Fixed by `0612948`. `CreatedInvoice::payment_method` at `envelope.rs:49–53`; population at 239; shared helper at 281–287. `response_headers.rs:9–71` freshly passed known/unknown/absent values, create/storno/credit consistency and old JSON defaults. |
| FINAL E.E5a: false storno heuristic presented as no-op proof | Agent guidance fixed by `14b80f2`: `envelope.rs:61–87`, `storno.rs:24–28`. Optional gross and observed no-op/repeat support retained. Worker reconciliation was not re-reviewed in this assigned scope; no claim of independently verifying that implementation here. |
| FINAL R1 shared XML completion | Fixed in the shared path, with additional namespace enforcement at requested HEAD. `parse_envelope` calls `response_text` and `protocol_text`; `response_completion` / `response_namespaces` freshly passed. M1 is a distinct error-ordering hole, not a claim that the old unrestricted ordinary-success parser remains. |
| FINAL D6: “unpaid proforma” promise | Fixed by `04effe8`: `proforma.rs:4–6,35–39` now explicitly says no paid-state check and qualifies the observed paid deletion. |
| FINAL E.E6: only historical text storno/credit examples recorded | Qualified/fixed: `fixtures/SOURCES.md:149–170` retains July history and adds September structured examples, hashes and defects. Fresh current response pages agree; the dated example tests passed. |
| Prior request-field/order coverage | Current aggregator/guardian/template support and dedicated writer assertions are present. No stale review claim overrides direct inspection of `storno.rs:260–271` and `credit_entry.rs:312–320`. |

F1 queried dates, general error-catalogue additions, cookie handling, other business-text models, invoice-request capabilities and PDF/taxpayer projections remain with their assigned owners. Their appearance in the older report is not an additional finding here.

## 8. Verification performed

**68 existing tests passed**, no failures, under the agent's default feature-free configuration. Commands used `--offline --locked`; build artifacts were directed to `/tmp/opencode/mutations-review-target`. No ignored live tests were selected.

```text
cargo test --offline --locked -p szamlazz-agent --target-dir /tmp/opencode/mutations-review-target --lib ops::envelope::
cargo test --offline --locked -p szamlazz-agent --target-dir /tmp/opencode/mutations-review-target --lib ops::storno::
cargo test --offline --locked -p szamlazz-agent --target-dir /tmp/opencode/mutations-review-target --lib ops::credit_entry::
cargo test --offline --locked -p szamlazz-agent --target-dir /tmp/opencode/mutations-review-target --lib ops::proforma::
cargo test --offline --locked -p szamlazz-agent --target-dir /tmp/opencode/mutations-review-target --lib ops::invoice::tests::a_preview_is_the_pdf_and_no_document
cargo test --offline --locked -p szamlazz-agent --target-dir /tmp/opencode/mutations-review-target --test response_headers --test response_completion --test response_namespaces --test numeric_fidelity --test upstream
```

Counts: envelope 4, storno 11, credit 12, deletion 7, preview 1; response headers 9, completion 1, namespaces 6, numeric fidelity 6, upstream 11. The workspace corpus was present: the corpus suites exercised it rather than taking their absent-corpus skip path. These include some cross-operation controls; they do not imply a full review of the other agents' surfaces.

The four-case scratch reproduction for M1 passed its assertions and printed the erroneous success in both malformed-payload cases. It ran once through its isolated Cargo manifest and once directly against a built agent rlib. Scratch Cargo dependency resolution is separate from the workspace lock; the focused repository tests above are the locked verification record.

Request ordering was inspected against fresh inline and downloaded schema sequences plus the existing generated-output tests. **No full XSD validator or live server acceptance run was performed.** The upstream request-outline comparator intentionally omits empty containers and surrounding text; its passing result is corroboration, not proof of exact XML fidelity. The field inventory above independently accounts for required flags, containers and optional fields.

## Conclusion

Keep the current mutation writers, sparse replies, observed storno no-ops/repeats and conditional numbered-56 support. Fix M1 by retaining the body verdict before optional-payload parsing. The earlier in-scope headline findings are already repaired; conflicting examples and unverified capabilities should not be reintroduced as production defects.
