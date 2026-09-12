# Számla Agent mutation conformance review — `77d53c5`

**Reviewed:** 2026-09-11. **Baseline/HEAD at start:** `77d53c553c9ecdc86d5fa72ca932c636256ae807`.
**Scope:** the complete current `crates/szamlazz-agent/src/ops/storno.rs`,
`credit_entry.rs` (registration **and explicit clearing**), `proforma.rs`, their
request/response tests, and the shared envelope/error helpers they actually use.
This is a current-code conformance review, not a diff review. The Agent sources
were clean at inspection; concurrent worker changes were outside this scope.
At completion, another task had advanced HEAD to
`370ff2e5398e9ff4a3aff3b6008ab82e38b9d058`. A baseline-to-HEAD diff confirmed
no changes to `crates/szamlazz-agent`, `Cargo.lock`, the Agent fixture corpus or
the behavior/clearing evidence files. The scoped review and test results therefore
still describe the requested baseline.

## Verdict

**No firm mutation implementation defect established.** All published request
fields are representable, the three action names/root namespaces and element
sequences match, and the current request matrix validates against freshly fetched
EN inline, HU inline and downloadable schemas.

The principal unresolved contract question is the **nonblank invoice-number
requirement on successful credit-entry registration/clearing**. The published
response schema permits omission; the Rust result requires a reported number.
This is reproducible as a schema/parser difference, but neither current docs nor
recorded vendor executions settle the success-specific guarantee. Keep it a
vendor question, not an observed production bug. The analogous storno response
has the same schema optionality, with a stronger need to recover reversal identity.

Explicit empty replacement is **implemented and live-proven within the recorded
test-account scope**. Do not revive an unsupported-clearing finding. Paid
proforma deletion, repeat storno, mismatched storno appearance, and assigning the
external id to the storno are likewise intentional readings of recorded vendor
behavior, not defects inferred from a generic expectation.

### Findings summary

| Category | ID | Priority | Conclusion |
|---|---|---|---|
| Firm implementation findings | — | — | None established |
| Vendor contract question | Q1 | P3 clarification; potentially P2 interoperability impact if confirmed | Successful credit registration/clear may be schema-valid without the nonblank number Rust requires |
| Vendor contract question | Q2 | P3 clarification | Storno external-id prose conflicts with the observed assignment behavior; numberless storno success guarantee also unspecified |
| Vendor documentation defects/questions | Q3 | P3 | Broken deletion schema locations; defective response examples; template-label disagreement |
| Optional hardening / coverage | H1 | P3 | Promote the credit/clear identity edge-case matrix into checked-in regression tests |
| Optional documentation precision | H2 | P3 | State buyer-tax-number supplementation and reconcile-first advice locally on the storno type |

Priorities describe follow-up urgency, not invented vendor incidence. No P0/P1
finding or live failure was observed in this review.

## Sources and acquisition

All URLs below were retrieved on **2026-09-11**, using public documentation GETs
only. The documentation footer reported **`v202608271632`**. Operation routes were
discovered through the Agent category navigation. In particular deletion is
`deleting_pro_forma_invoice`, not `deleting_proforma`.

| Surface | Current primary sources |
|---|---|
| Storno | [Request](https://docs.szamlazz.hu/agent/reversing_invoice/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/reversing_invoice/xml), [response + inline XSD](https://docs.szamlazz.hu/agent/reversing_invoice/response); [HU request](https://docs.szamlazz.hu/hu/agent/reversing_invoice/request), [HU XML](https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml), [HU response](https://docs.szamlazz.hu/hu/agent/reversing_invoice/response) |
| Credit entries | [Request](https://docs.szamlazz.hu/agent/credit_entry/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/credit_entry/xml), [response + inline XSD](https://docs.szamlazz.hu/agent/credit_entry/response); [HU XML](https://docs.szamlazz.hu/hu/agent/credit_entry/xml), [HU response](https://docs.szamlazz.hu/hu/agent/credit_entry/response) |
| Proforma deletion | [Request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml), [response + inline XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response); [HU XML](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml), [HU response](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/response) |
| Common requirements | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests), [errors and retry limit](https://docs.szamlazz.hu/agent/basics/error-handling), [IPN rules](https://docs.szamlazz.hu/agent/credit_entry/other), [invoice templates](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template), [invoice email rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) |

### Downloaded schemas

| Schema | Successful URL | SHA-256 of fresh response bytes |
|---|---|---|
| Storno request | <https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd> | `6f9d5beb6efd205f0509e7413e15fb96660ee89c9f169d27c4972d3b24127a3a` |
| Credit request | <https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd> | `9637a242df2f55f87ecfff1b07dd74ef0b09bdaff47e39bb4d9890d4748a73de` |
| Deletion request | <https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd> | `076b4d98c3cf599a5b5ab30e5e9ff3e522d9c0642d806fb502e8a560ace08641` |
| Shared invoice response | <https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd> | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |

The deletion request download was found via the existing fixture provenance as
a **lead**, then retrieved and checked anew. The sample's advertised
`https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd` returned 404;
inserting `/szamla/` before `docs` also returned 404. The advertised response
location ending `szamladbkdel/xmlszamladbkdelvalasz.xsd` returned 404, as did its
`/szamla/docs/` variant and the `dijbekerodel/` response candidate. The response
review therefore uses the available EN/HU inline deletion XSDs; no successful
standalone deletion-response download is claimed.

Fresh HTML, extracted schemas, SHA-256 manifest, generated request matrix and
scratch checks are in `/tmp/opencode/mutations-77d53c5/`; acquisition/validation
script: `/tmp/opencode/mutations-77d53c5.py`. Inline extraction preserves code-block
text after HTML decoding; its hashes need not match a differently formatted
repository fixture. The downloaded request hashes match the repository's recorded
**response-byte** hashes. No schema was repaired or relaxed for validation.

## Field and capability coverage

Paths below are relative to `crates/szamlazz-agent/` unless otherwise specified.
“Covered” means implemented and inspected/tested, not vendor-executed for every
combination. All request XSDs use qualified elements and fixed sequences.

### Common request boundary

`src/xml.rs:628–637` emits either `szamlaagentkulcs` or `felhasznalo` followed by
`jelszo`, in the declared order. Both credential forms were included in each
generated schema case. Simultaneously sending both authentication schemes is not
a missing business capability. `AgentRequest::to_wire` (`src/wire.rs:405` onward)
runs request validation, writes the XML, checks XML characters and builds the
multipart file. Storno and credit explicitly send the shared
`ops::RESPONSE_VERSION = "2"` (`src/ops.rs:32`); deletion has no version element.
Version-1 response support is unnecessary for these writers.

### Storno — `src/ops/storno.rs`

| Wire field/capability | Rust representation and emission | Assessment |
|---|---|---|
| Action/root/namespace | `action-szamla_agent_st`; `xmlszamlast`; `http://www.szamlazz.hu/xmlszamlast` (162–177) | Exact match |
| `beallitasok/eszamla` | `e_invoice: bool`, default false, always emitted (59–73, 144, 180) | Required XSD boolean; paper default is explicit, not inferred from original |
| `szamlaLetoltes` | `download_pdf: bool`, default false (74–76, 145, 181) | Required boolean; optional PDF read in response |
| `szamlaLetoltesPld` | `download_copies: Option<u8>` (77–81, 182–184) | Supported; narrower than XSD `int`, but deprecated and server-ignored per current inline docs; no realistic lost capability established |
| `aggregator`, `guardian` | `Option<String>`, `Option<bool>` (82–85, 185–188) | Both supported; absent and explicit false distinguished for guardian |
| `valaszVerzio` | Shared constant 2 (189) | Supported pinned format; after guardian, before external id |
| `szamlaKulsoAzon` | `external_id: Option<String>` (86–98, 190) | Emitted; docs accurately describe observed assignment to the newly issued storno; Q2 |
| `fejlec/szamlaszam` | Required `InvoiceNumber` (56–58, 193) | Both locales and XSD require invoice number. No documented independent order-number selector; external-id-only reversal is not established |
| `keltDatum` | `issue_date: Option<Date>` (99–107, 194) | Omitted by default; current docs distinguish observed code 352 from generic XSD allowance |
| `teljesitesDatum` | `fulfillment_date: Option<Date>` (108–121, 195) | Optional; can explicitly repeat original fulfillment date |
| `megjegyzes` | `comment: Option<String>` (122–123, 196) | Free-text reversal reason, XML-escaped |
| `tipus` | Fixed `SS` (197) | Matches the storno operation/sample; not an arbitrary caller-settable kind |
| `szamlaSablon` | `Option<InvoiceTemplate>` (124–125, 198–200) | All six published tokens in `src/types.rs:989–1020`, plus open `Other`; no unsupported preview/simple-items flags added |
| `elado/emailReplyto`, `emailTargy`, `emailSzoveg` | `Option<SellerEmail>` with three optional strings (126–128, 202–208) | Correct sequence; empty block emitted when absent, allowed by XSD |
| `vevo/email`, `adoszam`, `adoszamEU` | Three optional strings (129–135, 209–213) | All covered; schema frames tax numbers as supplementation of a missing original value (H2) |

Dates are checked at the sending boundary (`166–170`): positive years 1–9999,
ISO civil-date serialization. No local “must be today” check invents a clock or
turns an account observation into a universal rule. No local check compares
fulfillment date/appearance with the original; that requires a query and belongs
to a higher-level caller. The documentation explains how to derive them.

Current schema has **no** storno `sendEmail`, attachment collection, invoice
language, order-number field or independent original-external-id field. Do not
copy create-invoice email/attachment capabilities into a missing-storno-field
finding merely because the separate invoice-generation rules describe them.

### Credit registration and explicit clearing — `src/ops/credit_entry.rs`

| Wire field/capability | Rust representation and emission | Assessment |
|---|---|---|
| Action/root/namespace | `action-szamla_agent_kifiz`; `xmlszamlakifiz`; matching namespace (274–295); clear shares action (237–244) | Exact match |
| `beallitasok/szamlaszam` | Required `invoice_number` (158–160, 298) | By-number target; neither order nor external-id selector is in current schema |
| `adoszam` | `issuer_tax_number: Option<String>` (161–164, 299); also on clear (211–212) | Matches HU “ha megadod a kiállító adószámát … a megfelelő számlához rendeli”; no recipient/supplier substitution |
| `additiv` | Required boolean, default false; explicitly emitted (165–169, 186, 300) | False replaces; true retains old entries and appends. Mandatory XML field, so false is a crate constructor default, not an omitted server default |
| `aggregator` | Optional string (170–171, 301), also clear (213–214) | Covered |
| `valaszVerzio` | Explicit 2 (302) | Correct sequence |
| `kifizetes` cardinality | `CreditEntries`, at most five (49–143) | XSD `minOccurs="0" maxOccurs="5"`; `push`, `TryFrom<Vec<_>>`, serde enforce bound |
| `datum` | `CreditEntry.date: Date` (22–23, 306), every entry checked (278–283) | Required date, including entries after the first; positive-year validation |
| `jogcim` | `CreditEntry.title: PaymentMethod` (24–28, 307) | HU explicitly says “jogcím / fizetési mód”; known tokens plus `Other(String)` preserve arbitrary title tokens |
| `osszeg` | `Decimal` (29–31, 308) | Required XSD double; emits finite decimal text without binary-float rounding. No undocumented positive-only bound imposed |
| `leiras` | Optional `description` (32–33, 309) | Covered, omitted when absent |
| Empty replacing registration | `validate()` rejects unfinished `RegisterCreditEntry::new` (284–286) | Intentional construction guard, not loss of zero-entry protocol support |
| Explicit clearing | `ClearCreditEntries` (193–250), no entries/additive fields, delegates exact writer/parser | Emits false and zero entries; optional issuer/aggregator retained; closed serde input prevents registration-shaped JSON silently becoming clear |
| Empty additive registration | Allowed (539–555) | Schema-valid no-entry shape; source comment calls it harmless/no-op, but no separate executed vendor probe establishes that behavior |

No bank-account field exists on this **request**; the queried credit-entry record
having more fields does not imply a registration omission. Neither maximum
retained history nor maximum invoice lifetime credit count is five: the limit is
**entries per request**. Registration/clearing sends no currency; amounts concern
the selected invoice. Account-specific incoming-invoice selection and IPN effects
of clearing have not been established by the recorded clear probes.

### Proforma deletion — `src/ops/proforma.rs`

| Wire field/capability | Rust representation and emission | Assessment |
|---|---|---|
| Action/root/namespace | `action-szamla_agent_dijbekero_torlese`; `xmlszamladbkdel`; matching namespace (60–68) | Exact match |
| `beallitasok` credentials | Shared writer (69–71) | Complete settings block; no aggregator, guardian or response-version field in schema |
| `fejlec/szamlaszam` | `ProformaSelector::InvoiceNumber` (22–24, 73–75) | One proforma by number |
| `fejlec/rendelesszam` | `ProformaSelector::OrderNumber` (25–29, 76) | All matching proformas, documented on enum and request type |
| Both/neither selector | Excluded by enum shape | Deliberately stricter than two individually optional XSD fields; matches the documented alternative requests |
| Paid-state policy | No local check (1–6, 35–39) | Intentional, backed by D3; caller decides whether paid proformas must be retained |
| Success payload | `()` from dedicated `xmlszamladbkdelvalasz` verdict (62, 82–87) | Complete public response capability: vendor exposes no deleted-number list/count |

The HU page states: **“Ha azonos rendelésszámmal több díjbekérő is van a számlázási
fiókban, akkor a törlés az összes díjbekérőre vonatkozik.”** Translation: if several
proformas share the order number, deletion applies to all of them. The current
Rust warning that a latest-match query does not narrow deletion is correct.
There is no external-id deletion selector, force flag or payment-status guard
in the published request schema.

Identifier strings remain unvalidated wire strings (`src/types.rs:22–38`): even
an empty `InvoiceNumber` can be constructed. This is not an XSD mismatch because
the fields are unrestricted `string`, but the enum guarantees a chosen field,
not a usable vendor identifier. No worker's 40-byte bound is imposed on this
low-level client.

## Response, balance and error coverage

### Identity and payload

| Field / behavior | Current implementation | Assessment |
|---|---|---|
| Required `sikeres` | `src/xml.rs:473–500`, required lexical boolean | `true/false/1/0` with XML whitespace accepted; missing/empty/malformed verdict is uncertain, even beside a body code |
| `hibakod`, `hibauzenet` | `Verdict::api_error` (502–516); shared header check | Body-only 463 and deletion 335 retained; missing code becomes `Absent`, not fabricated |
| `szamlaszam` / `szlahu_szamlaszam` | `envelope::Body::invoice_number` (121–133, 326–329) | Body first; blank body falls back to decoded header; whitespace-only identity absent; Q1/Q2 |
| Storno numbered result | `parse_issued` (275–279), `CreatedInvoice` (23–59) | No numberless success promoted to an issued reversal; `reverses` explicitly heuristic (61–87) |
| Credit numbered result | `InvoiceBalance` (252–272), parse (316–343); clear delegates (247–249) | Same optional-identity question for both operations; requested number is never fabricated as response evidence |
| Net/gross/outstanding | `Option<Decimal>` from XML then raw numeric header (`envelope.rs:158–168, 344–370`) | All current monetary fields covered; body takes precedence; header decimal comma accepted; exponent notation accepted when exactly representable |
| Missing/zero balance | Optional values remain `None`; zero is `Some(0)` | Does not invent fully paid state from omission; no local recomputation pretending to be vendor balance |
| Payment method | `envelope::header_payment_method` (318–324), both result types | Encoded header decoded once, open `PaymentMethod`; no XML payment-method element exists in these response XSDs |
| Buyer account URL | Body then decoded `szlahu_vevoifiokurl` (135–143) | Preserved as opaque text; XML URL is not percent-decoded |
| PDF | Storno optional `Pdf`; decoded base64 (`envelope.rs:145–148, 246`) | Storno supports returned PDF. Credit-specific inline schema has no PDF, so `InvoiceBalance` not exposing one is appropriate |
| Internal id | Storno `szlahu_id` to optional nonnegative i64 (331–341) | Extra observed metadata; invalid/missing auxiliary id does not erase success. Credit has no documented id field requirement |
| Deletion identity/count | No fields beyond verdict | Correctly accepts bare success without headers; cannot infer exact deleted set |

Response schemas describe successes and failures together. All six fresh EN/HU
inline schemas and the common downloadable response schema accepted a bare
`sikeres=true` envelope in this review. This is grammar evidence, not proof that
all seven forms occur as successful vendor responses.

Plain-success malformed numeric/PDF content yields a parse failure and
`OutcomeClass::Unknown`; the parser does not silently approximate out-of-range
money. Finite `Decimal` is narrower than all XSD double values (NaN/infinities,
very large exponents). This is intentional financial-value handling, covered by
numeric tests, not a demonstrated real-invoice incompatibility.

### Error classification and recovery

`src/error.rs:320–424` distinguishes read retry hints from outcome classification:

| Codes/cause | Class and mutation interpretation | Evidence |
|---|---|---|
| 3, 135, 136, 164 | Credential/access refusal; `Rejected` for this exchange; not proof about an earlier lost send | Current official meanings; exact execution order not live-proven, as the code now states |
| 53, 54, 57 | Missing file, e-invoice not enabled, malformed XML: `Rejected` | Current official error table |
| 14, 221, 352 | Storno-of-storno, corrective prevents storno, issue date not today: `Rejected`, not retryable | Recorded B5/B7/B3 evidence and typed unit tests |
| 463 | Credit on reversed invoice: `Rejected`, body-only supported | D8 observation; clear parser's synthetic 463 control is not proof of a clear-on-reversed execution |
| 335 | `ProformaNotFound`, class `Rejected` | Official deletion response and D1/D2; not generic successful deletion replay |
| 7 | `NotFound` class but operation-dependent missing data/reference | Must not turn every mutation 7 into proof that no document exists |
| 71/152 | `DuplicateOrderNumber` | Shared classification retained; does not authorize adopting some unrelated document |
| 1, 55 | `Unknown`, `is_retryable=true` | Maintenance/signing failure; retry hint does not authorize another mutation |
| 56 | Storno + usable number: notification-warning success; without number: `Unknown` error | Shared envelope behavior; first-party PHP evidence is recorded in `src/recovery.md:44–49`, not a live test-account result |
| 56 on credit/clear | Remains an API error with `Unknown` class even with a number | Correct operation distinction: invoice identity alone does not establish a credit mutation; synthetic test `response_headers.rs:202–224` |
| Unknown/absent code; malformed response; HTTP failure; `szlahu_down` | `Unknown` | No refusal invented from an uninterpretable result |

Numbered 56 is handled specially only by issuing parsers. The envelope checks a
body refusal before attempting optional metadata; malformed diagnostics cannot
erase a readable code, and malformed/duplicate identity cannot be replaced by
an apparently usable header to manufacture success (`envelope.rs:179–248,
285–315`; `response_headers.rs:465–581`). Header/status precedence is deliberate:
nonempty down header, then error header, then HTTP status, then body. A body-only
code on HTTP 500 is conservatively a status failure, not a settled refusal.

The official limit is **“the same request … at most five times”**, including the
initial send. `src/recovery.md:36–42` states it accurately. These operation
implementations contain no retry loop. `src/recovery.md:13–19` correctly requires
mutation-specific reconciliation: additive repetition can double entries;
replacement/clearing can overwrite intervening state; repeated order deletion can
reach newly created matches. A read retry hint is not permission to repeat a write.

## Firm findings

**None established in the scoped runtime implementation.** In particular there
is no missing aggregator/guardian/template field, no missing explicit clearing
capability, no missing credit balance or payment-method field, no swallowed
body-only 463, and no singular-target claim for order-based proforma deletion.
The earlier review documents were not used as authority for this verdict.

## Vendor questions and intentional differences

### Q1 — P3 clarification: successful credit acknowledgement requires an unspecified nonblank echo

**Code:** `credit_entry.rs:319–322`, `ClearCreditEntries::parse` at 247–249,
`envelope.rs:121–133, 326–329`.

**Current official contract:** the [EN response](https://docs.szamlazz.hu/agent/credit_entry/response)
says additional headers **“may also arrive”** and optional elements **“may not
always be included”**. It declares
`<element name="szamlaszam" type="string" maxOccurs="1" minOccurs="0">`.
The HU page says **“további adatok is érkezhetnek”** and **“nem mindig jelennek meg”**.
Successful examples include the number, but neither locale states a nonblank
success-only guarantee.

**Exact offline reproduction:** parse HTTP 200, no headers, with:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
</xmlszamlavalasz>
```

Both registration and clearing return
`ResponseError::Parse(ParseError::Missing("szamlaszam"))`, class `Unknown`.
The fresh credit response XSD validates those bytes. Empty/whitespace-only body
and decoded header numbers have the same result. A nonblank header recovers
identity and absent monetary fields stay `None`. Confirmed with the scratch
public-API test, not a vendor call.

**Impact if the vendor confirms this success shape:** a completed credit mutation
would surface as uncertainty rather than `InvoiceBalance`. A consumer that wrongly
retries could duplicate additive entries or clear/replace newer entries. That is
conditional interoperability impact (potential P2), not demonstrated execution
incidence or a reason to fabricate the requested number as reported identity.

**Disposition:** retain the existing deliberate contract pending the answer, as
recorded in `docs/research/2026-09-10-credit-entry-success-question.md:22–29` and
the unsent `2026-09-11-agent-vendor-clarification.md:18–50, 118–123`.
The two successful clearing probes **do not** settle a universal echo guarantee.

### Q2 — P3 clarification: storno selector prose and optional success identity

The [EN request](https://docs.szamlazz.hu/agent/reversing_invoice/request) says the
invoice number **“is required”** and an external identifier may reference the
original **“if it was set when the invoice was created”**; the HU version agrees.
The inline sample instead describes later querying by that key. The recorded B6
and XPRB-P4 executions establish assignment to the created storno when the request
also carries the original number; reusing the original id makes the storno its
newest holder. `storno.rs:86–98` preserves precisely that interpretation.

Ask the vendor to distinguish original lookup from storno assignment and define
both-field precedence. This is **not** a missing external-id-only selector:
`szamlaszam` remains required by both current request schemas. Do not “fix” the
writer to use an original's external id as its storno id.

The [storno response schema](https://docs.szamlazz.hu/agent/reversing_invoice/response)
also makes `szamlaszam` optional. `parse_issued` rejects bare success, reproduced
offline. This is conservative because `sikeres=true` alone cannot identify the
reversal and the known proforma/delivery-note path can be a no-op. Ask for a
success-specific identity guarantee if expanding the existing vendor message;
do not classify a numberless storno as a preview or as “nothing reversed”.

### Q3 — P3 vendor documentation defects and remaining rules

1. **Deletion schema URLs are stale**, as recorded above. Fresh working request
   download and inline response schemas are available. Impact: an integrator
   following sample `schemaLocation` cannot fetch/validate; this is not a Rust
   request defect (Rust need not emit schema-location hints).
2. **Current storno/credit success examples are not executable XML as printed:**
   their buyer URL contains raw `&`; the storno PDF also contains `....`.
   `tests/upstream.rs:445–509` keeps the source bytes, explicitly asserts their
   refusal and tests clearly labelled repairs. These passing tests are not live
   response captures. The copied positive storno totals do not refute the
   negative-total account observations or justify rejecting positive metadata.
3. **Template labels remain a vendor question:** current Agent table labels
   `SzlaNoEnv` envelope-friendly and `SzlaAlap` traditional; the existing backlog
   records conflict with the linked knowledge-base labels. This review fetched
   the current Agent token table and verified Rust token coverage, but did not
   independently rerun a rendering comparison or re-fetch the knowledge-base
   page. Preserve tokens; no new label-mismatch bug is established here.
4. **Tax-number selection, clearing IPN and concurrency:** the optional issuer
   field is represented. The fresh IPN page says an issuer-side paid-amount change
   with configured URL creates an IPN, only the latest active invoice message is
   sent, and delivery is retried every three minutes up to ten attempts. It does
   not establish an already-empty clear's notification behavior or concurrent
   credit-write ordering. No IPN observed by the clearing probes is claimed.

### Recorded intentional behavior

`docs/szamlazz-hu-behaviour.md` was read as an execution-evidence record, especially
78–117, 119–165, and the scope/limitations at 3–47 and 194 onward:

| Behavior | Evidence boundary | Current treatment |
|---|---|---|
| Repeat storno returns existing storno | B4, one historical test account | Documented observation; response does not distinguish new vs existing |
| Proforma/delivery-note storno is a same-number no-op | B5 | `reverses` rejects same-number echo; successful wire result alone is insufficient |
| Fulfillment date defaults to original; wrong explicit dates accepted | P48 | Optional low-level field retained; caller advised to use original date |
| Storno appearance follows request even on mismatch | P73 four-case observations | Caller controls `e_invoice`; no invented server-match validation |
| Credit history removed by storno | B8 | Documented consequence, no automatic restoration |
| Fully paid proforma deletable | D3 | No paid-state gate invented by client |
| Replace/additive and five entries | D7 | Correct false/true semantics; sixth rejected locally, not a live vendor refusal |
| Explicit clearing | CLEAR-populated/CLEAR-empty, separate operator-confirmed test account, 2026-09-11 | Implemented explicit intent and shared balance parser |

The dated clearing record (`docs/research/2026-09-11-credit-clearing-live.md:14–32,
64–74`) reports `CTEST-2026-13` with a queried 100 HUF entry cleared and separately
empty `CTEST-2026-15` remaining empty. Both returned expected parsed numbers and
outstanding 3136 HUF, then were cleaned up with verified storno identities.
Raw headers/bodies were **not** archived, so which channel supplied each number
is unknown. Continuity with September 3/6/7's account was not established.
Current probe source at `tests/probes.rs:69–144` was inspected but not executed.

The September 11 receipt record was read only to keep its evidence separate.
Its receipt prefix/MNB/email observations establish nothing about invoice storno,
credit clearing, or invoice notification code 56. Synthetic zero/positive-gross
storno controls likewise do not establish live acceptance of zero/negative originals.

## Optional hardening and test gaps

### H1 — P3: persist credit/clear identity edge cases in the regression suite

The parser behavior is correct under its current deliberate contract, but
`credit_entry.rs:445–455`'s test named
`missing_invoice_number_everywhere_is_an_error` actually supplies **non-XML**
`not xml`, so it does not exercise the missing identity branch. The storno suite
has a bare-success test; clearing has one positive local HTTP test and malformed
verdict controls. A small shared registration/clear table for bare success,
blank body/header, header fallback, optional balances and differing echoed number
would pin the intentional policy directly. The scratch check executed those
cases here; its absence from the repository is a coverage opportunity, not a
present runtime defect. The current parser reports the vendor number as received,
even if different from the requested one; equality/correlation is left to callers.

### H2 — P3: narrow two local storno documentation claims

- `storno.rs:131–135` lists buyer tax numbers without the schema's condition:
  **“If the buyer's tax number is missing from the original invoice, it can be
  provided in this block.”** Document this as supplementation, not a general way
  to change existing buyer tax identity. No test proves changing an existing
  number; the current code makes no explicit promise that it does.
- `storno.rs:30–36` says resending after a transport failure is safe, within an
  explicitly observed test-account section. Shared `src/recovery.md:14` now says
  reconcile uncertain sends despite the observed repeat behavior. Linking that
  recovery advice locally would reduce the risk of readers treating the account
  observation as a universal retry guarantee. The observed idempotence itself
  is not being labelled a bug.

Other coverage limits: every declared path was exercised, not every Cartesian
combination or vendor account setting. Storno nested email leaves are tested in
full/empty blocks, not all eight subsets; each template token is covered through
the shared invoice token matrix, while the mutation matrix uses representative
templates. No live test of aggregator/guardian, original tax-number supplementation,
negative credit amounts, incoming-invoice clearing, or multi-match deletion
atomicity is inferred from XSD validity.

## Executed offline checks

All Cargo workspace commands below used the baseline lockfile and `--offline`.
Only the explicitly selected schema-export ignored test ran; no `live` or
`probes` test ran, and no `.env` was read. The local HTTP check uses wiremock with
dummy credentials. No source or fixture edits were made.

```sh
cargo test -p szamlazz-agent --locked --offline --lib ops::
cargo test -p szamlazz-agent --locked --offline --test clear_credit_entries --test request_dates --test response_headers --test response_booleans --test response_completion --test response_namespaces --test numeric_fidelity --test error_classification --test upstream
SZAMLAZZ_SCHEMA_OUTPUT=/tmp/opencode/mutations-77d53c5/requests.json cargo test -p szamlazz-agent --locked --offline --test schema_requests -- --ignored --exact emit_request_matrix
python3 /tmp/opencode/mutations-77d53c5.py
cargo test -p szamlazz-agent --locked --offline --features client-reqwest --test client clears_credit_entries_only_through_explicit_request -- --exact
cargo run --offline --manifest-path /tmp/opencode/mutations-77d53c5/Cargo.toml
```

| Check | Result |
|---|---|
| Operation unit tests (filter also includes other operations) | **136 passed**, 49 filtered |
| Selected integration suites, in command order | **2 + 3 + 14 + 3 + 4 + 11 + 6 + 3 + 11 = 57 passed** |
| Schema matrix exporter | **1 passed**; exporter itself is not a validator |
| Fresh storno EN/HU/download schemas | **30 requests × 3 = 90 valid**, **27/27** declared paths per source |
| Fresh credit EN/HU/download schemas | **14 requests × 3 = 42 valid**, **15/15** declared paths per source |
| Fresh deletion EN/HU/download schemas | **4 requests × 3 = 12 valid**, **8/8** declared paths per source |
| Six inline response schemas + shared download | **7 bare-success validations passed** |
| Explicit clear checked transport, local mock | **1 passed**, 8 filtered |
| Scratch public-API controls | **Passed**: 18 numberless/blank parser checks; header fallback, body precedence, missing balance, exponent amount, bare deletion acknowledgement, five/six-entry push and serde bounds |

`xmllint` was not on PATH and Python `lxml` was unavailable. Rather than claiming
the repository's required full-workspace schema checker ran, the scratch validator
called installed **libxml2 2.9.14** (`xmlSchemaParse`, `xmlSchemaValidateDoc`) through
Python `ctypes`, with network-disabled document parsing. All 16 fresh schemas
compiled; **151** document/schema checks returned valid. Acquisition uses public
GETs; validation itself is offline. This does not replace the workspace CI check
and did not execute its negative controls.

The first local HTTP test selection without `client-reqwest` ran **zero tests**
because that file is feature-gated; the feature-enabled command above then ran
the intended test successfully. The auxiliary scratch Cargo project uses current
Agent source by path but resolved its own offline dependency lock; its result
supplements, rather than replaces, the locked workspace runs. Scratch files are
temporary evidence, not added tests or shipped changes.

## Conclusion

The scoped current mutation implementation conforms to the published request
capabilities and recorded vendor behavior. Preserve the explicit clear operation,
operation-specific recovery distinctions and broad order-deletion warning.
The actionable next contract work is vendor clarification of successful credit
identity (including clearing), with response/selector documentation corrections
kept separate from implementation bugs. No code fix is proposed as part of this report.
