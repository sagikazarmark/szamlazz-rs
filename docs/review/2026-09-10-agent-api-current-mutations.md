# Current Számla Agent mutation and response-envelope review

**Revision:** `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78` (HEAD verified before and after inspection).  
**Reviewed / official sources freshly fetched:** 2026-09-10.  
**Result:** the earlier envelope M1 is **closed**. One different **P3 malformed-response robustness defect** remains: optional payload structure can erase a valid body-only number on a notification-failure reply. No confirmed normal-response P0/P1/P2 defect or mutation request-field/order defect was found. A schema-permitted numberless credit-entry success is reproducibly rejected, but its actual success-path contract remains ambiguous.

## 1. Scope and method

Read all request fields, constructors, writers, parsers and unit tests in `crates/szamlazz-agent/src/ops/{storno,credit_entry,proforma,envelope}.rs`. Followed `wire.rs`, `xml.rs`, `number.rs`, `types.rs::Pdf`, `invoice.rs`'s response classification, response integration tests and fixture provenance. All source references below name **this revision**. Short paths beginning `src/` or `tests/` are relative to `crates/szamlazz-agent/`.

Reviewed independently against fresh vendor pages before reading [the stale mutations review](2026-09-10-agent-api-mutations.md), whose baseline was `382cf7615aca1d64a05c7c3f77110248dde51950`. Its conclusions were subsequently checked for closure, not adopted as current findings.

Only this report was added to the repository by this review. Scratch probes and a freshly downloaded first-party PHP package are under `/tmp/opencode/current-mutations-fbda137/`. Existing untracked documents and concurrent Restate contract/test/design work were left alone. No production source, repository test, fixture or lockfile was edited by this review. The final scoped diff confirmed that the agent crate, fixture provenance, behaviour notes and lockfile still matched HEAD. No subagents were used.

All network access was unauthenticated documentation/XSD/PHP-package GETs. **No live Számla Agent request, account query, issue, storno, credit-entry registration, deletion or email send was made.** Offline public-parser probes do not establish server emission or acceptance.

## 2. Fresh primary-source register

The requested category pages and their request/response/XML links were fetched. Current rendered pages identify site build `v202608271632`; this is not a date of every statement.

| ID | Sources fetched | Material used |
|---|---|---|
| S | [Storno category](https://docs.szamlazz.hu/agent/category/reversing-invoice), [request](https://docs.szamlazz.hu/agent/reversing_invoice/request), [response](https://docs.szamlazz.hu/agent/reversing_invoice/response), [XML/example/XSD](https://docs.szamlazz.hu/agent/reversing_invoice/xml), [HU XML/XSD](https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml) | Multipart operation, every request field/sequence, structured success/error examples and reply schema. |
| SX | [Storno request XSD download](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd) | Download agrees with the inline field sequence. |
| C | [Credit category](https://docs.szamlazz.hu/agent/category/registering-credit-entry), [request](https://docs.szamlazz.hu/agent/credit_entry/request), [response](https://docs.szamlazz.hu/agent/credit_entry/response), [XML/example/XSD](https://docs.szamlazz.hu/agent/credit_entry/xml), [HU response](https://docs.szamlazz.hu/hu/agent/credit_entry/response), [HU XML/XSD](https://docs.szamlazz.hu/hu/agent/credit_entry/xml) | Replace/additive, issuer tax number, 0–5 entries, response optionality. |
| CX | [Credit request XSD download](https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd) | Same request fields/cardinality as inline. |
| D | [Deletion category](https://docs.szamlazz.hu/agent/category/deleting-a-pro-forma-invoice), [request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), [response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response), [XML/examples/XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml), [HU XML/XSD](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml) | Both selectors, all-matches scope, own response root, 335, critical text/HTML. |
| I | [Create response](https://docs.szamlazz.hu/agent/generating_invoice/response), [request](https://docs.szamlazz.hu/agent/generating_invoice/request), [XML/example/XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml), [settings index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules), [notification](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | Shared envelope, header encoding, preview meaning, test-account email qualification. Full create-request compliance is outside this review. |
| IX | [Shared reply XSD download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Required boolean verdict; optional code/message/number/totals/URL/base64 PDF. |
| B | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication), [error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | Credential alternatives, error versus retry semantics. Current general table does not list 56. |
| P | [PHP index](https://docs.szamlazz.hu/php/), [response handling](https://docs.szamlazz.hu/php/valasz-feldolgozas), [proforma deletion](https://docs.szamlazz.hu/php/dijbekero-torles), [linked PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Specific numbered-56 exception and corroborating deletion scope. PHP was read, never executed. |

Both deletion schema URLs embedded in the examples returned **404** after HTTPS upgrade:

- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd`
- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd`

The corresponding `/szamla/docs/xsds/szamladbkdel/` variants also returned 404. Current inline request and response schemas supply the evidence; no replacement schema was fabricated.

Fresh PHP ZIP SHA-256: `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Within `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/Response/InvoiceResponse.php`, line 17 defines `INVOICE_NOTIFICATION_SEND_FAILED = 56`; lines 319–322 explicitly condition successful issuance on a number:

> “Ha a számlaértesítő kézbesítése sikertelen volt, de a válasz tartalmaz számlaszámot, akkor a számla kiállítása sikeres.”

Translation: if notification delivery failed but the response contains an invoice number, invoice issuance succeeded. The PHP response page independently says an invoice can be **“successfully issued”** despite notification delivery failure. This is first-party implementation/documentation evidence, **not a live capture of code 56**.

## 3. Reproduced actionable finding

### CM1 — Structural failure in optional metadata erases a body-only number on code 56

**Severity:** P3 — malformed-response robustness.  
**Confidence:** high in the reproduction and cause. **Vendor occurrence:** unobserved.  
**Affected:** invoice creation and storno through the shared issuing envelope. Not the credit/deletion verdict path.

**Locations:**

- `src/ops/envelope.rs:104–118`: document identity and all optional metadata are deserialized together as `Body`.
- `src/ops/envelope.rs:203–210`: any payload-deserialization failure under notification failure substitutes `Body::default()`.
- `src/ops/envelope.rs:212–217`: number lookup on that default body now depends entirely on a number header; without one, the reply becomes an API error 56.
- `src/ops/envelope.rs:281–287`: the new split retains the verdict, but payload parsing remains all-or-nothing.
- Entry points: `src/ops/invoice.rs:927–935`, `src/ops/storno.rs:211–213`.

**Minimal input:** HTTP 200, no `szlahu_*` headers:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres>
  <hibakod>56</hibakod>
  <szamlaszam>I-2</szamlaszam>
  <szamlabrutto><bad/></szamlabrutto>
</xmlszamlavalasz>
```

`StornoInvoice::new("I-1").parse(&raw)` returns:

```text
Err(Api(ApiError { code: InvoiceNotificationDeliveryFailed, message: "" }))
```

Two `<pdf>` elements containing valid base64 reproduce the same identity loss. Controls:

1. Change the nested amount to `<szamlabrutto>bad</szamlabrutto>`: result is the numbered document with `notification_delivery_failed=true`, gross `None`.
2. Add `szlahu_szamlaszam: I-2`: the structurally malformed case also returns the numbered document, because the header rescues the discarded identity.
3. Give a readable non-56 body refusal and header 56: the refusal now survives malformed payloads (the **closed** old M1).

**Why actionable:** S/I/IX make the amount optional (`type="double"`, `minOccurs="0"`, `maxOccurs="1"`); P establishes the number-conditioned warning rule. The input above is not XSD-valid, so this is not failure to parse a compliant vendor document. It is an inconsistency with the crate's explicit robustness promise at `envelope.rs:173–177`: malformed optional data should not hide numbered notification-failure issuance. A unique valid number, like the verdict, can be retained independently of an unrelated metadata decoding failure.

**Impact:** callers lose the reported document number and get an uncertain API error instead of the usable issued-document warning. That forces unnecessary reconciliation and can encourage duplicate issuance in a caller that incorrectly resends on errors. The crate itself does not automatically resend, and 56 remains `OutcomeClass::Unknown`, so this is not evidence of an automatic duplicate-write bug. Existing safe recovery advice still requires reconciliation.

**Recommended fix:** retain a separately parsed identity result alongside the verdict before optional payload decoding. Recover only a unique, usable, correctly namespaced number; duplicated/malformed identity must not be silently selected. Preserve complete-document and namespace checks, non-56 refusal precedence, and the intentional header-56/non-XML fallback. Test body-only number + body/header 56 with nested/duplicated metadata, plus bad/duplicate identity and non-56 refusal controls. Do not solve this by requiring a header or a gross total.

## 4. Reproduced boundaries that are not confirmed normal-wire defects

### CA1 — Numberless credit-entry success: schema-compatible, success-path guarantee ambiguous

**Priority:** P3 clarification/capability decision; potential P2 interoperability impact if emitted. **Not counted as a confirmed vendor-contract defect.**

At `src/ops/credit_entry.rs:250–256`, the parser accepts the verdict then requires an echoed invoice number. HTTP 200 with no number header and this complete XML:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
</xmlszamlavalasz>
```

reproducibly returns `Err(Parse(Missing("szamlaszam")))`. Adding a number header makes it succeed. The C response schema allows this body: only `sikeres` is mandatory. The page says **“Elements marked `minOccurs="0"` may not always be included”** and that header data **“may also arrive”**; HU says the same. This merits explicit discussion rather than treating the echo as schema-required.

However, the schema combines success and error shapes. The optional number may accommodate failures, and every fetched successful example and recorded registration observation has a number. No source explicitly demonstrates a *successful* numberless registration. The existing `InvoiceBalance` intentionally requires response identity (`credit_entry.rs:195–210`); that design alone is not a vendor guarantee, but neither is XSD optionality proof of actual success-path omission.

**Impact if emitted:** a settled successful mutation becomes a parse error after credits may have changed. Repeating additive entries may double them; repeating replacement may overwrite intervening state. Reconcile rather than resend. **Resolution:** ask whether successful version-2 registration always echoes a nonblank number in at least one channel. If not, represent a successful acknowledgement with optional reported identity, or explicitly distinguish the known requested number from an echoed number. Do not silently fabricate response evidence. Keep the create/storno new-number requirement independent of this decision.

### CH1 — Ordinary success can still be hidden by malformed optional totals/PDF

**Priority:** P3 hardening, not normal-response noncompliance. `envelope.rs:222–247,253–261` is lenient only for numbered 56; an ordinary numbered success containing `szamlabrutto=junk` or invalid base64 returns a parse error. Credit totals are similarly strict at `credit_entry.rs:257–274`. Scratch controls and existing `envelope::tests::parse_issued_table` reproduce this deliberate policy.

The schemas make these fields optional, but require the correct lexical type when present. Rejecting malformed content is defensible. If the API is strengthened, retain independently established issuance/registration evidence and expose unusable metadata separately; do not let a repair imply that a send is safe to repeat. For previews, an unusable PDF cannot satisfy the requested preview result. CM1 is narrower because numbered-56 leniency is already an explicit promise.

### CH2 — Header-56 fallback covers an unreadable verdict as well as non-XML text

**Priority:** P3 policy clarification/hardening; not a reopened old M1. At `envelope.rs:188–193`, any failure to establish a complete envelope/verdict under header 56 falls back to header evidence. A numbered header 56 plus `<sikeres>bogus</sikeres><hibakod>3</hibakod>` returns an issued warning, reproduced offline. Here the body verdict is invalid; no valid refusal was established and later erased. The same fallback deliberately supports a non-XML notification-failure body.

The docs specify no conflict precedence. Retain the valid-body refusal fix. Consider distinguishing an absent/non-envelope body from a recognizable but invalid envelope if future policy requires an uncertainty diagnostic; do not infer a settled refusal from arbitrary `<hibakod>` text inside an invalid document. A truncated body likewise does not establish that the write failed.

## 5. Request field coverage

`?` means optional in the fetched schema. All named singleton elements have `maxOccurs=1`. Shared serialization emits UTF-8 XML 1.0 with the proper default namespace (`src/xml.rs:19–40`), escaped text (`414–424`), credential alternatives in order (`456–465`) and multipart file transport (`src/wire.rs:66–99,402–408`). `xsi:schemaLocation` is a schema hint, not a missing business field. B explicitly allows **either** agent key **or** username/password; the deletion example showing both does not require both.

### Storno

| Ordered wire field/block | Current mapping / coverage |
|---|---|
| `action-szamla_agent_st`; `xmlszamlast`, namespace `http://www.szamlazz.hu/xmlszamlast` | Exact, `src/ops/storno.rs:162–170`. |
| Root `beallitasok`, `fejlec`, `elado?`, `vevo?` | Exact order, `171–206`. Optional seller/buyer containers are emitted empty by default, which is schema-valid; no claim that omission has identical email behavior. |
| Settings `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?` | Shared credentials, `172`. |
| `eszamla`, `szamlaLetoltes` | Explicit bools; constructor false/false, `145–146,173–174`. Appearance is caller-selected, not inherited locally. |
| `szamlaLetoltesPld?` | Optional `u8`, `175–177`; narrower than XSD int, but S says **“our system no longer processes it.”** No useful missing capability established. |
| `aggregator?`, `guardian?`, `valaszVerzio?`, `szamlaKulsoAzon?` | String/bool, fixed shared version 2, external id, `178–183`; specialized storno order is correct. |
| Header `szamlaszam`, `keltDatum?`, `teljesitesDatum?`, `megjegyzes?`, `tipus?`, `szamlaSablon?` | Number, civil dates, comment, fixed `SS`, open template token, `185–194`; all supported. |
| Seller `emailReplyto?`, `emailTargy?`, `emailSzoveg?` | `SellerEmail`, `195–201`; each optional. |
| Buyer `email?`, `adoszam?`, `adoszamEU?` | All supported in order, `202–206`. S annotates tax fields as supplying a number missing from the original, not arbitrary invoice editing. |

### Credit-entry registration

| Ordered wire field/block | Current mapping / coverage |
|---|---|
| `action-szamla_agent_kifiz`; `xmlszamlakifiz`, namespace `http://www.szamlazz.hu/xmlszamlakifiz` | Exact, `src/ops/credit_entry.rs:213–228`. |
| Root `beallitasok`, then `kifizetes` × 0–5 | Exact, `230–245`; collection bounds push, vector conversion and serde at `69–74,116–132`. |
| Credentials; `szamlaszam`, `adoszam?`, `additiv`, `aggregator?`, `valaszVerzio?` | Required number, optional issuer tax number, explicit bool, optional aggregator, version 2, `230–237`. |
| Entry `datum`, `jogcim`, `osszeg`, `leiras?` | Civil date, open payment-method token, finite Decimal, optional description, `238–244`. No invented nonnegative restriction or silent rounding. |
| Replacement/addition | C: **“If true, former credit entries are retained; otherwise they are replaced.”** Constructor defaults to replacement (`180–187`); empty replacement is deliberately refused by validated transport (`217–223`). Empty additive is accepted locally. |

The HU example says **“ha megadod a kiállító adószámát, a rendszer a bejövő kifizetést a megfelelő számlához rendeli”**: supply the issuer tax number to associate the incoming credit entry with its invoice. This supports `issuer_tax_number`; the English example's “incoming receipt” is not a separate receipt operation.

### Proforma deletion

| Ordered wire field/block | Current mapping / coverage |
|---|---|
| `action-szamla_agent_dijbekero_torlese`; `xmlszamladbkdel`, matching namespace | Exact, `src/ops/proforma.rs:60–68`. |
| Root `beallitasok`, `fejlec` | Required and correctly ordered, `69–77`. |
| Settings credentials only | All supported; no response-version element exists or is invented. |
| Header `szamlaszam?`, `rendelesszam?` | `ProformaSelector` emits exactly one (`18–29,72–77`), matching the two examples; both/neither are excluded by the enum though the raw XSD sequence does not express exclusivity. |

HU D explicitly states **“Ha azonos rendelésszámmal több díjbekérő is van a számlázási fiókban, akkor a törlés az összes díjbekérőre vonatkozik.”** Deletion applies to **all** matching proformas. P corroborates multiple deletion and describes rollback if a member fails. Current rustdoc correctly describes all-matches scope (`proforma.rs:1–6,25–27,35–41`); no deletion count/list is promised. The rollback claim was not tested and does not settle a lost reply.

**Request result:** no omitted current field, wrong multipart action, wrong root/namespace, or field-order defect found. Version-1 responses and arbitrary storno `tipus` are intentional capability boundaries, not omissions in these version-2 built-in operations.

## 6. Response field and precedence coverage

### Shared create/storno envelope and credit balance

| Wire field | Current implementation and disposition |
|---|---|
| Root `xmlszamlavalasz`, matching namespace | `envelope.rs:19–21,281–287`; `xml.rs:63–167` checks complete document/root/namespace, `174–228` excludes foreign subtrees before serde. Namespace aliases work; foreign local names cannot supply identity/verdict. |
| `sikeres` | Required shared verdict (`xml.rs:319–327`), true/false/1/0 (`588–598`). Missing is rejected. Empty is legacy false, not success; outside the boolean XSD lexical set. |
| `hibakod?`, `hibauzenet?` | Failed verdict becomes `ApiError`; missing/blank code is `Absent`, unknown token preserved, message retained (`xml.rs:329–350`). Non-56 body refusal now wins before optional payload failure (`envelope.rs:195–209`). |
| `szamlaszam?` | Body first, then once-decoded number header; whitespace trimmed, blanks absent (`envelope.rs:120–133,298–301`). Required for numbered results; CA1 for credit; CM1 for payload failure. |
| `szamlanetto?`, `szamlabrutto?`, `kintlevoseg?` | `net_total`, `gross_total`, `outstanding`, body first then respective monetary header (`envelope.rs:158–168,227–244`; `credit_entry.rs:257–274`). Omitted/blank body falls back; absent both is `None`. No recomputation or sign gate. |
| Numeric lexical forms | XML finite dot/scientific numbers; exact Decimal conversion (`number.rs:63–141`). Headers accept ungrouped comma/dot and exponent with HTTP SP/HTAB padding (`envelope.rs:316–343`). No percent decoding, grouping, or silent precision loss. Valid body takes precedence over invalid header; invalid nonblank body does not fall back to a valid header. |
| `vevoifiokurl?` | Body first, otherwise decoded `szlahu_vevoifiokurl` (`envelope.rs:135–143`). XML receives entity decoding only. Existing scalar trimming remains. |
| `pdf?` | Create/storno optional PDF, standard base64 with whitespace removed (`envelope.rs:145–147,247`; `types.rs:104–117`). Credit-specific inline reply schema contains no PDF; registration does not decode one. No PDF signature/rendering validation promised. |
| `szlahu_fizetesmod` | Open `PaymentMethod` on both `CreatedInvoice` and `InvoiceBalance`; one helper (`envelope.rs:49–53,246,290–296`; `credit_entry.rs:275`). Unknown strings retained. No invented XML `fizetesmod` field. |
| `szlahu_id` | Optional nonnegative i64 document id on `CreatedInvoice`; absent/invalid/negative is `None` (`envelope.rs:303–314`). Observed auxiliary metadata, not account identity. Credit projection omits this undocumented auxiliary field deliberately. |

The fetched header tables list `szlahu_szamlaszam`, net/gross, error/error-code, payment method and customer URL. They explicitly mark number/error as URL-encoded and totals/code as not encoded. The crate uses one-pass form-style decoding for textual headers (`wire.rs:239–249,343–350`), case-insensitive names, raw numeric access. Observed `szlahu_id`/`szlahu_kintlevoseg` are supported even though omitted from the table. No VAT-total/currency element exists in this envelope to expose.

### Decision order (library policy, not a vendor conflict guarantee)

1. Nonblank decoded `szlahu_down` → `ServiceUnavailable`.
2. Nonblank raw `szlahu_error_code` → API error, with issuing operations allowed to consider 56.
3. Without such an error header, known non-2xx status → `HttpStatus`, before XML.
4. Otherwise complete structured body and its verdict decide.

This order is explicit at `wire.rs:291–310`. A body-only error at HTTP 200 is read; at HTTP 500 it is an HTTP-status error, including body-only numbered 56. A bare success-number header does not bypass non-2xx. Header 56 with a number can succeed even at HTTP 500 or with non-XML notification text. A readable non-56 body refusal overrides provisional header 56. Ordinary error headers take precedence over contradictory body success. All of those distinctions are covered by existing tests and/or scratch controls.

For numbered 56, invalid optional scalar totals/PDF become `None`; unnumbered 56 is an uncertain error, never preview. Credit/deletion do not promote 56 to mutation success. P supports the issuing exception; the Agent pages' generic **“If error codes are present in the header, invoice number and amounts are omitted”** does not invalidate that more specific first-party rule. Exact conflicting-channel emission remains unknown.

### Preview

I's request XSD describes `elonezetpdf` as **“Preview PDF of the document (no actual document is created).”** No complete dedicated preview response example was supplied. Current `invoice.rs:923–935` correctly distinguishes:

| Input | Result |
|---|---|
| Numbered success, even if preview requested | `CreationOutcome::Issued`; reported issuance is not hidden. |
| Unnumbered success, preview requested, valid PDF | `Preview`. |
| Unnumbered success, preview requested, missing/blank PDF | Missing PDF error. |
| Unnumbered success without preview request | Missing invoice number. |
| Unnumbered storno success | Missing invoice number (`envelope.rs:271–275`). |
| Unnumbered 56 | API error, not preview. |

No confirmed preview-result defect. Missing PDF on an otherwise numbered create/storno, even when download was requested, stays `None`; the XSD marks it optional. A future stronger download-completeness signal should preserve the numbered write result rather than imply it did not land.

### Deletion response

`proforma.rs:82–87` selects `xmlszamladbkdelvalasz` in its own namespace. `sikeres`, `hibakod`, `hibauzenet` cover all D reply fields. True/1 yields `()`; false/0 yields an API error; the documented 335 is `ProformaNotFound`, not successful replay. Missing/invalid verdict, wrong namespace, incomplete XML and critical text/HTML are errors. D says **“On critical error, a plain text/html error message may be returned instead”**; bounded unexpected-body diagnostics preserve that evidence. No number/count is discarded from a successful deletion because none is defined.

## 7. Live deviations, provenance and unresolved documentation

Read [behaviour notes](../szamlazz-hu-behaviour.md), especially lines 1–28, 69–110, 129–145, 153, 160–161, 174–215. Their observations concern one test account on the listed dates; underlying historical exchange logs are not in the repository. Current Rust correctly preserves:

- Repeat storno success echoing the existing SS, and proforma/delivery-note storno success-shaped same-number no-ops (`storno.rs:24–50`; notes B4/B5).
- External id attaching to the **created storno**, not selecting the original when a number is supplied; repeat storno discards a new id (`storno.rs:86–98`; notes B6/XPRB/P48). Current request prose ambiguously says it can reference the original. Do not change targeting from this prose alone.
- Fulfillment defaulting to the original, explicitly mismatched fulfillment accepted, non-today storno issue date rejected with 352, and request-selected appearance accepted even when mismatched (`storno.rs:59–73,99–121`; P48/P73). These are not bugs in field emission or grounds to prohibit caller values in the low-level crate.
- `reverses()` is expressly a **heuristic**, not identity or issuance-by-this-call proof. False with missing/positive gross is inconclusive; zero acceptance is synthetic policy (`envelope.rs:61–87`). No positive-gross schema sample establishes a negative-original reversal.
- Paid proforma deletion and header-free deletion success (notes D1/D3), body-only credit error 463 (D8), additive/replace semantics (D7), and observed comma totals (P60). Do not “fix” these to match an oversimplified example.

[Fixture provenance](../../fixtures/SOURCES.md) distinguishes synthetic fixtures/golden output from official corpus. Relevant sections are lines 23–32, 34–74, 90–169, 193–247. Fresh S/C/I pages still contain raw `&` in URL example text, and create/storno PDFs abbreviated with `....`. These examples are not valid complete XML/base64 exchange captures. Existing September corpus tests (`tests/upstream.rs:445–529`) preserve originals, escape the example URL explicitly and substitute synthetic `%PDF-` bytes for structural checks. Their passing result is not live evidence or reconstruction of an original PDF.

Additional source qualifications:

1. **56 was never triggered in account probes** (notes 153,171,180–181). Fresh I notification docs say test-account email is routed to the account-settings address, which qualifies hypotheses based on invalid buyer email. No live 56 shape was established here.
2. **Header/body duplication prose is broader than the XSD.** “With an XML response, the same data is also in the XML body” does not name an XML payment-method field. None exists in the fetched schema; preserve the header reader.
3. **Payment-method/customer-URL header encoding is under-specified.** Their table rows lack an explicit encoding declaration. PHP reads URL with `rawurldecode` (136–137) and decodes again in its getter (347–348). This is not evidence to introduce double decoding or change literal-plus handling in Rust. Current once-decoded URL tests are policy controls, not wire captures.
4. **Preview plus `simpleItems` request ordering remains a separate known source disagreement.** Fixture provenance records inline versus download/PHP order. The current chosen writer and qualification at `invoice.rs:220–222,819–825` are not changed or newly adjudicated by a response review. No combined-preview live claim.
5. **Empty replacing credit request** is schema-permitted but intentionally unsupported by validated transport. Notes 211–215 explicitly say clearing was not probed. Empty additive acceptance is likewise an input policy, not a new live result. A sixth entry cannot enter the public bounded collection.
6. **No arbitrary storno email/target features inferred:** no `sendEmail` or external-id-only storno selector is established by S; schema and request prose require `szamlaszam`. Partial email overrides/inheritance and later notification delivery remain unverified.
7. **Finite exact money/civil dates are deliberate domains.** The broader XSD double/date lexical space does not require accepting nonfinite monetary values, silently rounded decimals, or caller timezone-bearing dates. No demonstrated business request is missing for that reason.

## 8. Prior finding closure

| Prior claim | Current result |
|---|---|
| Stale mutations **M1**: header 56 + readable non-56 refusal + structurally malformed metadata falsely returns issued | **Closed by `fbda137`.** `envelope.rs:281–287` returns the established verdict independently of payload failure; `195–209` evaluates it first. Existing regression `tests/response_headers.rs:432–450` passed. Independent scratch added create as well as storno and nested PDF as well as nested/duplicate totals: all preserve code 3 and message. CM1 above is a different, opposite-direction identity-loss case, not a claim the fix failed. |
| Comma monetary fallback | Remains fixed, `envelope.rs:333–343`; header integration tests pass dot/comma/exponent/sign/blank/body precedence/numbered-56 controls. |
| Create/storno payment method discarded | Remains fixed, `envelope.rs:49–53,246,290–296`; existing public create/storno/credit test includes unknown tokens and old JSON default. |
| False storno heuristic presented as proof of no reversal | Agent guidance remains fixed at `envelope.rs:61–87`, `storno.rs:24–28`. Worker reconciliation implementation is outside scope. |
| Shared ordinary-success XML completion/namespace gap | Current completion/namespace tests pass; expected whole-document and foreign-field behavior retained. Intentional header-56 fallback is separately qualified in CH2. |
| “Unpaid proforma” promise | Remains fixed, `proforma.rs:4–6,35–39`; no paid-state precondition falsely promised. |
| Only historical text storno/credit examples recorded | Current provenance and dated structured examples remain present; fresh source fetch agrees on their fields and defects. |
| Stale report's required credit echo description | Still accurately describes implementation; CA1 makes the schema-versus-success-path distinction explicit instead of claiming the schema requires it. |

The earlier **transport** header-loss finding is not closed by the envelope fix: supporting inspection still finds `Client::send` awaiting `response.bytes().await?` before constructing `RawResponse` (`src/client.rs:349–364`). Interrupted-body transport behavior was not independently reproduced in this parser-focused review and is not added to its confirmed finding count. Successful offline `RawResponse` parsing does not establish preservation of headers through body-transfer failure.

## 9. Verification results and limits

### Existing tests

Executed against the working revision with the existing workspace dependency resolution:

```text
cargo test --offline -p szamlazz-agent --lib --test response_headers --test response_completion --test response_namespaces --test numeric_fidelity --test upstream
```

**220 passed, 0 failed:** unit tests 185; response headers 10; completion 2; namespaces 6; numeric fidelity 6; upstream corpus 11. Corpus directories were present and the relevant tests read them; absent-corpus skipping is not the basis of this result. The unit run includes all mutation/envelope/preview unit tests. Other operations' unit tests passing does not extend this review's coverage claim to their full contracts. No ignored live tests were selected. The command did not use `--locked`; `git diff` showed no tracked changes, including no lockfile change.

### Independent scratch probes

Source: `/tmp/opencode/current-mutations-fbda137/probe.rs`; manifest beside it. Six assertion groups exercised:

1. Schema-compatible numberless credit success and header-number rescue.
2. Old M1 closure for create and storno across nested/duplicate optional payloads.
3. CM1 body-only numbered-56 identity loss, structural versus lexical metadata failures, header rescue.
4. Plain-success strict optional-data failures versus numbered-56 leniency.
5. Preview classification, numbered preview-request answer, invalid-verdict header fallback, body-only 56 at HTTP 500.
6. Deletion boolean forms, critical HTML and a local deletion wire build.

```text
cargo test --offline --manifest-path /tmp/opencode/current-mutations-fbda137/Cargo.toml -- --nocapture
```

**6 passed**, where assertions deliberately confirm observed problematic behavior as well as controls. Scratch Cargo resolves its own dependencies. To remove that qualification from the reproductions, also built the actual workspace library and linked the same test source against its rlib and matching jiff artifact:

```text
cargo build --offline -p szamlazz-agent --message-format=json
rustc --edition=2024 --test /tmp/opencode/current-mutations-fbda137/probe.rs -L dependency=/home/laborant/szamlazz-rs/target/debug/deps --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib --extern jiff=/home/laborant/szamlazz-rs/target/debug/deps/libjiff-609abc9c009d4b2c.rlib -o /tmp/opencode/current-mutations-fbda137/probe-locked
/tmp/opencode/current-mutations-fbda137/probe-locked --nocapture
```

**6 passed again**, with the same reproduced errors and controls. The executable name `probe-locked` denotes workspace-linked artifacts, not a claim that the preceding Cargo invocation used `--locked`.

Request sequence/cardinality coverage was inspected against freshly fetched inline/download schemas and existing writer/golden/outline tests. **No full XSD validation was run**: `python3` had no `lxml`, and neither `xmllint` nor `xmlstarlet` was installed; no dependencies were installed for this review. The simple CA1 body's schema compatibility follows directly from the fetched sequence/occurrence declarations. The request-outline comparator intentionally loses empty containers and surrounding text (`tests/upstream.rs:1222–1234`); its success is corroboration, not exhaustive semantic equivalence.

No live confirmation of email delivery, zero/negative-original storno, external-id-only behavior, conflicting headers, sparse successful credit replies or combined preview options. No wasm or interrupted-body transport execution was performed. No production fix or proposed-fix test run is claimed.

## Conclusion

Keep the current mutation writers, observed storno echoes/no-ops, sparse totals, header decoding and conditional numbered-56 behavior. The known refusal-erasure issue is fixed. The next bounded robustness improvement is **CM1: retain valid body identity independently of optional metadata**. Clarify successful credit-entry echo guarantees before relaxing that response contract, and preserve the distinction between malformed-response hardening, documentation ambiguity and observed vendor behavior.
