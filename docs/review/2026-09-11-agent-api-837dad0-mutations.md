# Számla Agent mutation audit — 837dad0

**Reviewed revision:** `837dad024300e2a202c2b6351fcba73df82a7744`, verified as HEAD before and after the audit.  
**Official-source retrieval:** 2026-09-11.  
**Result:** **no confirmed implementation defect in the requested scope**. All documented request fields, their ordering, namespaces and multipart selectors are represented correctly. One reproduced **credit-response compatibility question** remains: a schema-valid successful acknowledgement without an echoed invoice number is rejected. Its occurrence, or permission specifically on successful version-2 registrations, is not established; it is not counted as an operational P2 bug.

## 1. Scope and method

This is a full-source audit at the pinned revision, not a diff review:

- `crates/szamlazz-agent/src/ops/storno.rs`: **request behavior**, public settings/defaults, targeting and serialization.
- `crates/szamlazz-agent/src/ops/credit_entry.rs`: complete public models, collection bounds, validation, writer, response parser and tests.
- `crates/szamlazz-agent/src/ops/proforma.rs`: complete selector/model, writer, response parser, semantics and tests.
- Supporting paths traced as needed: `xml.rs`, `wire.rs`, `number.rs`, `ops/envelope.rs`'s credit payload/helpers, shared `PaymentMethod`/`InvoiceTemplate`/`SellerEmail`, and `client.rs`'s transport handoff. These supporting reads do not constitute a separate complete transport or issuing-response audit.

Source references below use paths relative to `crates/szamlazz-agent/`, unless explicitly workspace-qualified. Line ranges refer to the pinned revision. Official citations identify pages and their named sections or XML/XSD blocks; the rendered pages have no stable source-line numbering.

The three requested category pages were fetched first, followed by **every current operation descendant**: request, response, XML/example/XSD, and credit's IPN page. The sitemap corroborated that enumeration. There are no separate current settings descendants under these three operations: settings are in their XML blocks. Relevant shared template, notification and simplified-image settings were also fetched. Still-reachable legacy `/xsd` pages were checked after independent inspection, when historical leads identified them.

The independent comparison preceded reading historical `docs/review/*agent-api*` reports. Those reports subsequently supplied leads, not current evidence. No agents were delegated. No account credentials were read or used; no Számla Agent operation or vendor account request was made. Network accesses were public documentation/XSD GETs. Only this report was added in the repository; scratch reproduction source was written with `apply_patch` under `/tmp/opencode/`.

## 2. Fresh official-source register

Every URL in this table was fetched during this audit. Current documentation pages displayed **`v202608271632`**; legacy `/xsd` pages displayed **`v202606031507`**. Neither footer proves a statement's publication date. Examples are documentation specimens, not account captures.

| ID | Fetched URL(s) | Coverage |
|---|---|---|
| N | [Sitemap](https://docs.szamlazz.hu/sitemap.xml) | Current operation descendants and settings navigation |
| S0 | [Storno category](https://docs.szamlazz.hu/agent/category/reversing-invoice) | Requested entry point |
| S1 | [Storno request EN](https://docs.szamlazz.hu/agent/reversing_invoice/request), [HU](https://docs.szamlazz.hu/hu/agent/reversing_invoice/request) | POST, file field, number requirement, external-id wording |
| S2 | [Storno XML/example/XSD EN](https://docs.szamlazz.hu/agent/reversing_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml) | Complete request sequences, types and cardinalities |
| S3 | [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response) | Fetched completely, including response examples/XSD; used for request version/download behavior only |
| S4 | [Storno legacy XSD page](https://docs.szamlazz.hu/agent/reversing_invoice/xsd), [download](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd) | Request schema cross-check |
| C0 | [Credit category](https://docs.szamlazz.hu/agent/category/registering-credit-entry) | Requested entry point |
| C1 | [Credit request](https://docs.szamlazz.hu/agent/credit_entry/request) | Multipart route |
| C2 | [Credit XML/example/XSD EN](https://docs.szamlazz.hu/agent/credit_entry/xml), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/xml) | Every setting/entry, replace/additive, 0–5 entries |
| C3 | [Credit response EN](https://docs.szamlazz.hu/agent/credit_entry/response), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/response) | Both versions, headers, complete structured success/error examples and XSD |
| C4 | [Credit legacy XSD page](https://docs.szamlazz.hu/agent/credit_entry/xsd), [download](https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd) | Request schema cross-check |
| C5 | [IPN](https://docs.szamlazz.hu/agent/credit_entry/other) | Account URL setting, trigger, parameters, delivery/retry behavior; distinct from the synchronous reply |
| D0 | [Deletion category](https://docs.szamlazz.hu/agent/category/deleting-a-pro-forma-invoice) | Requested entry point |
| D1 | [Deletion request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request) | Multipart route |
| D2 | [Deletion XML/examples/XSD EN](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml) | Both selectors, complete schema, HU all-matches note |
| D3 | [Deletion response EN](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response), [HU](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/response) | Complete success/error examples, response XSD, critical text/HTML |
| D4 | [Deletion legacy XSD page](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xsd), [working request download](https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd) | Request schema cross-check; download location revalidated from a historical lead |
| X | [Shared invoice response XSD download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Credit helper's shared envelope; distinguished from credit's PDF-free inline schema |
| B1 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Agent key or username/password alternatives |
| B2 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | Endpoint, action table, case sensitivity, XML file transport |
| B3 | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | Error semantics, version-1 text, five-send limit, test-environment limit |
| B4 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie) | Supporting native transport ownership check |
| T | [Template settings](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template), [linked template knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok) | Six tokens, omission/default, conflicting human labels |
| E | [Notification settings](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | Shared email text, BBCode, distinction from create-only controls |
| I | [Simplified-image settings](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Storno inherits original state; no missing storno `simpleItems` switch |
| P | [PHP deletion](https://docs.szamlazz.hu/php/dijbekero-torles), [PHP credit](https://docs.szamlazz.hu/php/jovairas) | All-matches/rollback corroboration and wrapper defaults; no PHP code executed |

Failed downloads, recorded rather than treated as evidence of schema contents:

- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd` — **404** (the example's location, upgraded from HTTP).
- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd` — **404** (likewise).
- `https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdelvalasz.xsd` — **404** (adjacent candidate, not a published working link).

All three **request** downloads succeeded and agree with the current inline field sequences/cardinalities. HU credit uses a named root complex type rather than EN's anonymous one; the accepted structure is equivalent. Deletion's **response** schema was reviewed from the complete inline blocks. No schema was merged, patched or substituted into the fixture corpus.

## 3. Findings and evidence adjudication

### Confirmed defects

**None established in this scope.** In particular, no missing request field, incorrectly ordered optional field, wrong namespace/action, wrong credit-entry maximum, or missing documented deletion response field was found.

### Q1 — Numberless successful credit acknowledgement is rejected; success-path contract unresolved

**Priority:** P3 contract clarification; **potential P2 compatibility impact if emitted**. Not a confirmed operational defect.  
**Code:** `src/ops/credit_entry.rs:250–256`; mandatory public result identity at `:195–197`; body/header lookup at `src/ops/envelope.rs:120–133`.  
**Confidence:** high in local reproduction and schema compatibility; no live occurrence established.

**Quoted requirement/source:** C3 EN/HU says additional header data **“may also arrive”**, and **“Elements marked `minOccurs="0"` may not always be included.”** Its operation-specific response XSD declares:

```xml
<element name="sikeres" type="boolean" maxOccurs="1" minOccurs="1"/>
<element name="szamlaszam" type="string" maxOccurs="1" minOccurs="0"/>
```

**Observed implementation:** after validating the complete envelope and accepting its true verdict, the parser uses `body.invoice_number(response).ok_or(ParseError::Missing("szamlaszam"))?`. Optional echoed identity is therefore mandatory for constructing `InvoiceBalance`.

**Concrete reproduction:** pass HTTP 200, no `szlahu_*` headers, and this body to `RegisterCreditEntry::parse`:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
  <kintlevoseg>0</kintlevoseg>
</xmlszamlavalasz>
```

```rust
use szamlazz_agent::ops::credit_entry::RegisterCreditEntry;
use szamlazz_agent::wire::{AgentRequest, RawResponse};

let raw = RawResponse::new::<&str, &str>([], br#"
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres><kintlevoseg>0</kintlevoseg>
</xmlszamlavalasz>"#.to_vec()).with_status(200);
let result = RegisterCreditEntry::new("I-1").parse(&raw);
// Err(ResponseError::Parse(ParseError::Missing("szamlaszam")))
```

The executed scratch program populated a valid one-entry request and verified `validate()` for both replace and additive modes. Both the body above and a verdict-only body returned `Err(Parse(Missing("szamlaszam")))`. Adding only `szlahu_szamlaszam: I-1` made all four cases succeed. It used the public parser linked to the actual workspace-built library; no transport or credential object was used.

**Impact if this response is emitted:** the caller loses a successful registration acknowledgement and any returned balance, and must reconcile. Incorrectly repeating an additive request can duplicate entries; repeating replacement can overwrite intervening changes. The parser itself sends nothing, and the returned parse error is conservatively classified as an unknown outcome. No automatic duplicate registration is demonstrated.

**Evidence adjudication:** the body is allowed by the published schema, but that schema and the optionality prose cover **both success and error**. C3's success example has a number and its error example does not. Workspace behavior notes `:145` record numbered successful credit replies; none records numberless success. Thus neither a universal success echo guarantee nor success-specific permission to omit it is conclusively established. Version 1's bare `xmlagentresponse=DONE` does not settle version 2. The Rust result shape is also not a vendor guarantee. Historical reports disagree on severity; this audit retains the unresolved interpretation rather than promoting schema optionality into a claim of observed vendor failure.

**Bounded next action:** obtain confirmation whether every successful version-2 registration echoes a nonblank number in either channel. If not, permit an acknowledgement with optional **reported** identity, or distinguish requested identity explicitly; do not silently manufacture a vendor-reported number. Workspace `docs/research/2026-09-10-agent-vendor-questions.md:37–54` already asks this question and remains an **unsent draft**, not an answer. This review did not send it.

## 4. Exhaustive request comparison

Notation: `?` means `minOccurs=0`; every named singleton has `maxOccurs=1`. Arrows denote XML child order. A required string element is not necessarily constrained to nonempty text by its XSD.

### Shared routing and scalar behavior

All three requests use the exact root's namespace `http://www.szamlazz.hu/{root}`. Namespace identifiers remain HTTP; the endpoint is HTTPS. `src/xml.rs:139–160` emits XML 1.0/UTF-8 and the default namespace. `:568–620` escapes text, writes booleans as true/false, Decimal as plain numeric text, Date as civil-date text, and credentials in schema order. `None` omits a scalar; `Some("")` emits an empty element. No claim is made that omission and empty have identical server semantics.

`src/wire.rs:7–14,66–99,402–408` supplies the endpoint constant, multipart file disposition/filename, `text/xml` part and boundary framing; the action determines the file-field name. `src/client.rs:374–383` actually POSTs those bytes/content type. Validation occurs before multipart construction, including rejection of XML 1.0-forbidden characters. Native default cookie persistence exists (`client.rs:300–307`); a custom transport remains caller-owned. The example `xsi:schemaLocation` is a validation hint, not an omitted operation field.

B1 explicitly permits **either** agent key **or** username/password. The writer emits `szamlaagentkulcs`, or `felhasznalo → jelszo`; it does not emit both merely because deletion's examples show both. Credential case/content is not normalized.

B3's **at most five sends of the same request** is a caller-level limit, distinct from the credit request's **at most five entries**. These operation implementations contain no resend loop or cross-call counter. The documented test-environment ceiling is 500 invoices per ten minutes, not a mutation-model default or local rate limiter. No broader transport retry-budget compliance is claimed.

### Storno request — S1/S2/S4

| Surface / exact sequence | Mapping, defaults and assessment |
|---|---|
| `action-szamla_agent_st`; root `xmlszamlast` | Exact at `storno.rs:162–170` |
| `beallitasok → fejlec → elado? → vevo?` | `:171–206`; optional seller/buyer containers are always emitted, empty by default, schema-valid |
| Credentials → `eszamla → szamlaLetoltes → szamlaLetoltesPld? → aggregator? → guardian? → valaszVerzio? → szamlaKulsoAzon?` | `:171–184`; every field present in correct order. Required bools default false/false. Copies `Option<u8>`, aggregator string, guardian optional bool, version fixed to shared `ops::RESPONSE_VERSION = "2"`, external id string |
| `szamlaszam → keltDatum? → teljesitesDatum? → megjegyzes? → tipus? → szamlaSablon?` | `:185–194`; required original number, two optional dates, free text, fixed `SS`, optional open template token |
| Seller `emailReplyto? → emailTargy? → emailSzoveg?` | `:195–201`; all three independently optional through `SellerEmail` |
| Buyer `email? → adoszam? → adoszamEU?` | `:202–206`; every field supported. S2 specifically describes supplying a buyer tax number missing from the original, not arbitrary editing of an existing tax number |
| Template tokens | `types.rs:983–1019`: `SzlaMost`, `SzlaAlap`, `SzlaNoEnv`, `Szla8cm`, `SzlaTomb`, `SzlaFuvarlevelesAlap`, plus `Other(String)`; exact tokens sent |

Constructor `storno.rs:138–159` omits every optional field and requests neither electronic appearance nor PDF. These are **library defaults**, not automatic inheritance. S3 supports version 2's structured XML and optional base64 PDF; the writer deliberately does not select version 1 text/raw PDF even though the example omits `valaszVerzio`. No reply decoding conclusion is drawn here beyond that request choice.

S2 says copies are deprecated: **“our system no longer processes it.”** Narrowing XSD `int` to `u8` for this ignored setting is not a demonstrated lost business capability. Dates/appearance/external-id behavior is adjudicated against live evidence below, rather than historical sample dates. Optional template omission delegates selection; `InvoiceTemplate::Default` emits `SzlaAlap` and is explicitly not equivalent to omission.

No external-id-only storno, caller-defined `tipus`, `sendEmail`, attachments, language or `simpleItems` flag is declared by the storno request XSD. I explicitly says storno **“inherits the state of the original document”** for simplified image. Create's notification page E does not establish a missing storno email-suppression/attachment capability. The shared subject/body fields and BBCode text pass through unchanged apart from XML escaping.

### Credit-entry registration — C1/C2/C4

| Surface / exact sequence | Mapping, defaults and assessment |
|---|---|
| `action-szamla_agent_kifiz`; root `xmlszamlakifiz` | Exact at `credit_entry.rs:213–228` |
| `beallitasok → kifizetes{0..5}` | `:230–245`; correct order/cardinality |
| Credentials → `szamlaszam → adoszam? → additiv → aggregator? → valaszVerzio?` | `:230–237`; number required, issuer tax number optional, explicit bool, optional aggregator, version 2 |
| Entry `datum → jogcim → osszeg → leiras?` | `:238–244`; Date, open `PaymentMethod`, Decimal, optional description |
| Bound | Private `CreditEntries(Vec<_>)`, `push`, `TryFrom<Vec<_>>` and custom serde enforce ≤5 (`:48–132`); slice access cannot grow the collection |
| Replace/additive | Constructor false; C2: **“If true, former credit entries are retained; otherwise they are replaced.”** Every request explicitly sends the choice |
| Zero entries | Empty replace refused by `validate` (`:217–223`); empty additive accepted locally; deliberate narrowing described below |

No amount sign/scale restriction or automatic rounding is invented. Decimal covers finite representable amounts rather than the entire XSD `double` domain. `PaymentMethod::Other` retains free text, so the `jogcim` string is not restricted to known methods (`types.rs:588–687`). Required date/title/amount are provided by each entry constructor, not defaulted to PHP's today/transfer/0.0. Optional fields default absent (`credit_entry.rs:175–188`). PHP P's `additive=true` is its wrapper default, not a wire requirement overriding this explicit false.

C2 HU: **“ha megadod a kiállító adószámát, a rendszer a bejövő kifizetést a megfelelő számlához rendeli”** supports `issuer_tax_number`. The EN example's “incoming receipt” translation does not define a receipt operation. Matching incoming invoices by issuer tax number was not live-tested here.

### Proforma deletion — D1/D2/D4

| Surface / exact sequence | Mapping and assessment |
|---|---|
| `action-szamla_agent_dijbekero_torlese`; root `xmlszamladbkdel` | Exact at `proforma.rs:60–68` |
| `beallitasok → fejlec` | Required blocks, correct order (`:69–77`) |
| Settings: credentials only | Complete; no response-version/PDF setting exists or is invented |
| Header `szamlaszam? → rendelesszam?` | `ProformaSelector` emits exactly one (`:14–30,72–77`); exact lowercase `rendelesszam`, not query operation's `rendelesSzam` |
| Number / order scope | Number targets one proforma; order targets **all** matching proformas (`:25–27,35–43`) |

The raw XSD structurally permits both/neither selectors; the enum deliberately selects one documented alternative. It cannot prevent an empty selected string, which the XSD's plain `string` also permits; no local semantic validation or vendor acceptance of empty targeting is claimed. Neither precedence for both identifiers nor external-id deletion is established by the sources.

D2 HU explicitly says **“Ha azonos rendelésszámmal több díjbekérő is van a számlázási fiókban, akkor a törlés az összes díjbekérőre vonatkozik.”** Translation: if several proformas share an order number, deletion applies to all of them. P corroborates multiple deletion and states rollback if a member fails. The implementation submits a single operation, not a local loop or a latest-query-selected subset; it does not promise an audited rollback or a deleted-number list.

## 5. Complete credit and deletion response comparison

C3 version 1 is `xmlagentresponse=DONE` on success and `[ERR]…` text on failure; version 2 is structured `xmlszamlavalasz`. Credit always asks for 2, so rejecting version-1 text is not a version-2 bug. D3 always uses `xmlszamladbkdelvalasz`, with critical text/HTML as an error alternative and no selectable version.

| Documented field/channel | Actual handling and assessment |
|---|---|
| Root/namespace | Credit `xmlszamlavalasz` / `http://www.szamlazz.hu/xmlszamlavalasz` (`credit_entry.rs:250–251`, `envelope.rs:19–21`); deletion `xmlszamladbkdelvalasz` / matching namespace (`proforma.rs:82–87`) |
| Required `sikeres` | Shared unique scalar verdict, true/false/1/0 (`xml.rs:460–481,750–782`). Missing/invalid errors; empty is legacy false rather than success, outside XSD boolean lexical forms |
| Optional `hibakod`, `hibauzenet` | False yields `ApiError`; absent/blank code is `ErrorCode::Absent`; unknown code preserved. Optional malformed/duplicate diagnostic does not erase readable verdict/code (`xml.rs:460–504`) |
| Credit `szamlaszam?` / encoded `szlahu_szamlaszam` | Nonblank trimmed body first, decoded header fallback (`envelope.rs:120–133,326–329`); required by result, Q1 |
| Credit `szamlanetto?` / raw `szlahu_nettovegosszeg` | Optional exact Decimal; body then header (`credit_entry.rs:257–262`) |
| Credit `szamlabrutto?` / raw `szlahu_bruttovegosszeg` | Same (`:263–268`) |
| Credit `kintlevoseg?` / raw `szlahu_kintlevoseg` | Same (`:269–274`); outstanding header is observed despite omission from C3's table |
| Credit `vevoifiokurl?` / `szlahu_vevoifiokurl` | Body first, then once-decoded header (`:276`, `envelope.rs:135–143`); XML gets entity decoding, not URL decoding |
| `szlahu_fizetesmod` | Once-decoded open `PaymentMethod` (`credit_entry.rs:275`, `envelope.rs:318–324`); no documented XML counterpart |
| Error headers | `szlahu_error_code` raw; `szlahu_error` decoded once; body-only errors also recognized (`wire.rs:262–310`, `xml.rs:510–535`) |
| Deletion success | `()` after true verdict; no count/number list exists to expose |
| Deletion failure | False/335 is `ProformaNotFound`, not idempotent success; absent/unknown codes retained. `hibakod` is XSD `int` here, deliberately handled by the shared open error-code reader |
| Critical deletion text/HTML | Bounded unexpected-body diagnostic or HTTP-status error, never successful deletion; tests `proforma.rs:155–180` |

C3's exact sequence is `sikeres → hibakod? → hibauzenet? → szamlaszam? → szamlanetto? → szamlabrutto? → kintlevoseg? → vevoifiokurl?`. D3 has only its first three fields. All are accounted for. The response reader intentionally tolerates reordered fields and well-formed extensions rather than validating XSD ordering. Complete UTF-8/XML, root, namespace and lexical validation precedes protocol projection; foreign subtrees cannot supply verdict or metadata (`xml.rs:183–359`).

Blank credit body amounts are absent and permit header fallback; malformed nonblank body amounts do not fall back. A present invalid/blank numeric header errors; absent both is `None`. XML money accepts finite dot/exponent forms; header money additionally accepts decimal comma and HTTP padding, not grouping or percent encoding (`envelope.rs:151–168,344–371`, `number.rs:63–141`). Out-of-domain precision/underflow/overflow is refused, not silently rounded. These are explicit finite-money policies, not claims of supporting every `double` value including infinity/NaN.

Response decision order is **library policy**, not a vendor-specified resolution of contradictory channels: nonblank `szlahu_down`, nonblank error-code header, known non-2xx HTTP status, then complete XML/body verdict/payload. Thus body-only 463 at HTTP 200 is an API error; at HTTP 500 without a header code it is an HTTP-status error. Neither credit nor deletion promotes code 56 to success merely because issuing operations have a numbered-notification exception. The fetched sources establish no contrary conflict-priority rule.

Credit uses the shared `Body`, which also declares optional `pdf`; it neither decodes nor returns PDF. Its own inline schema has no PDF, VAT total, currency or document-id field. A malformed nested/repeated unexpected PDF can still make shared payload deserialization fail: an extension-robustness boundary, not a missing documented credit field. Similarly, strict rejection of malformed nonblank optional totals is not rejection of an XSD-valid numeric value. The observed auxiliary `szlahu_id` is not exposed by `InvoiceBalance`; this is a projection boundary, not an omitted C3 requirement.

C5 IPN is an asynchronous account-configured POST about payment status, not the synchronous credit reply. Its fields, optional payment date activation, three-minute/ten-attempt delivery, HTTP 200 requirement and latest-record behavior do not belong in `InvoiceBalance` or require IPN transport in `RegisterCreditEntry`. IPN implementation itself is outside scope.

## 6. Accepted live deviations and explicit library boundaries

Read the complete workspace `docs/szamlazz-hu-behaviour.md` and `fixtures/SOURCES.md`. The behavior evidence is bounded to **one TEST account**, on its stated September 3/6/7 dates; original historical request/response logs are expressly outside the repository (`behavior:3–28`). Its observations are accepted here as instructed, not independently re-probed. Older “design consequence” cells are not new vendor guarantees.

| Behavior / difference | Precise evidence and disposition |
|---|---|
| Storno external id attaches to created SS, not original lookup when number supplied; repeat discards new id | B6, B4x, XPRB-P4, P48-P6; behavior `:69–70,95`. Preserve `storno.rs:86–98,183` despite S1 EN/HU wording |
| Non-today storno issue date rejected 352, including paper | B3, behavior `:90`. Preserve omission recommendation at `storno.rs:99–107`; old example date is not an acceptance guarantee |
| Omitted fulfillment inherits original; equal and mismatching explicit dates accepted | P48-P1…P5, behavior `:92–94`. Preserve optional date and qualified guidance at `storno.rs:108–121`; no inferred server guard |
| Storno appearance is the request's flag, even if mismatched | P73-EE/EP/PE/PP, behavior `:18–24,97–98`. Preserve caller-selected bool and advice to derive original appearance (`storno.rs:59–73`) |
| Repeat storno echoes existing SS; storno of D/SL is a same-number no-op | B4/B5, behavior `:86–87`. Preserve request documentation; this audit does not re-review issuing-result interpretation |
| Paid proforma can be deleted; success has no headers; repeat/missing/consumed returns 335 | D1…D4, behavior `:105,109–110`. Preserve caller-owned paid-retention policy and dedicated verdict parser |
| Replace/additive and five entries; returned order differs | D7, behavior `:133–134`. Preserve explicit false default and bound; no entry-order/deduplication promise |
| Credit on reversed invoice gives body-only 463 | D8, behavior `:135,141`. Preserve body verdict parsing; freshly executed controls retain code 463 even with unusable optional diagnostic |
| Comma monetary header | P60-E1/E3, behavior `:160`; shared reader supports it. This is not permission for comma in XML numbers |
| Empty replace refused locally despite schema permitting zero entries | `credit_entry.rs:144–151,217–223`, README `:395`, behavior `:211–215`. Intentional capability boundary; **zero-entry clearing was not probed**. Rustdoc's “would clear” is inference, not observed effect; empty additive no-op is likewise not new live evidence |

Current live source was read, **not run**: `tests/live.rs:44–134` covers HUF invoice lifecycle, replace/additive balances, storno date/appearance/external-id and repeat; `:138–169` covers number-selected proforma deletion. `tests/probes.rs:13–63` covers the two mismatched appearance cases. `tests/live_support/mod.rs:155–186` shows that reversal explicitly queries/copies fulfillment date and deletion uses number selection. The behavior note's references to the former `eszamla_semantics` test name are stale: current source has the lifecycle and separate probes just described. Existence of those tests is not an execution record for this audit.

Fixture provenance distinguishes July official corpus, September response acquisitions, synthetic fixtures and project-generated golden XML (`fixtures/SOURCES.md:23–39,149–170`). Existing outline comparison intentionally loses empty containers and surrounding text; its pass is corroboration, not exact XML equivalence or live acceptance. The audit independently checked container presence and every field above.

## 7. Source ambiguities, defects and historical leads

These are **not additional confirmed implementation findings**:

1. **External-id targeting:** S1 EN/HU says the original may be referenced by external id but also requires its invoice number; S2 says the identifier permits later querying without identifying which document. Live evidence establishes assignment to SS with number present. No external-id-only selector or reinterpretation is justified.
2. **Malformed success examples:** current C3 contains raw `&` inside `vevoifiokurl`. S3 additionally contains `....` in its abbreviated base64 PDF and positive example totals. Rejecting those literal samples is correct XML/base64 handling, not a parser defect. `tests/upstream.rs:445–510` preserves original defects and explicitly escapes the URL/substitutes synthetic PDF for controls. These transformations do not reconstruct real replies.
3. **Metadata parity:** C3 says **“With an XML response, the same data is also in the XML body”**, yet declares no payment-method XML element. Header-only payment-method support matches the concrete schema. Its payment-method/URL header rows do not precisely specify encoding; decode-once/literal-plus treatment is a tested library policy, not a freshly captured guarantee.
4. **Template names:** T's Agent table maps `SzlaAlap` to traditional and `SzlaNoEnv` to envelope-friendly; the linked knowledge base reverses those labels (and uses inconsistent tag/token casing). Current `InvoiceTemplate` matches the Agent tokens. No rendered document was examined; do not swap mappings on the strength of one conflicting page. The existing unsent vendor-question draft §2 remains a question.
5. **Deletion scope/rollback:** EN D2 omits HU's all-matches sentence. P corroborates batch scope and says rollback occurs on member failure; no live atomicity test establishes that behavior here. It cannot settle a lost reply or justify a retry against future matches.
6. **Specialized settings:** aggregator and guardian are declared and correctly ordered. Their contracted operational meaning/defaults are not explained by these sources, and no entitlement or all-combinations acceptance is established.
7. **Schema-domain narrowing:** finite Decimal/civil Date, ignored copies as u8, fixed SS/version 2, exactly one deletion selector, and empty-replace refusal are explicit supported subsets. No demonstrated useful operation is missing solely because the full raw schema permits more values.

Historical mutation reports read only after the independent audit: `2026-09-10-agent-api-mutations.md`, `2026-09-10-agent-api-current-mutations.md`, `2026-09-10-agent-api-f83e5fd-mutations.md`, and `2026-09-11-agent-api-mutations.md`; relevant adjudication sections were searched for disposition. Leads were rechecked as follows:

| Historical lead | Current-source/evidence adjudication |
|---|---|
| Missing/incorrect aggregator, guardian, template order | No current defect: exact present writer sequences and freshly fetched inline/download XSDs agree |
| Paid-only restriction or single-match order deletion | No current defect: `proforma.rs:4–6,25–27,35–43` explicitly documents actual scope/policy; D2 HU and behavior D3 corroborate |
| Only text examples available | Obsolete: current response pages have structured examples, retained with defects in dated corpus; fresh fetch and upstream test agree |
| No working deletion request-XSD download | Revalidated working `dijbekerodel` URL in D4; response candidates remain 404 |
| Malformed diagnostic erases credit refusal (f83e5fd FM1) | Fixed: `xml.rs:460–481` separates facts/diagnostic. Scratch nested/duplicate diagnostic controls retain body-only 463; existing `response_headers` regression also passes |
| Numberless successful credit (CA1/FA1/M-01) | Still reproducible at current lines; adjudicated independently as Q1, not a settled success-specific vendor requirement |
| Comma headers/payment method lost | No current defect: shared numeric and payment-method helpers plus executed cross-operation header tests |
| Issuing-only numbered-56 fallback, malformed identity, preview, storno heuristic | Outside requested **storno request** scope. Current shared regression tests ran, but their pass is not a new complete storno/create-response audit or an imported finding |

## 8. Executed verification

1. `git rev-parse HEAD` matched the requested SHA. Initial working tree was clean. A later `git diff 837dad024300e2a202c2b6351fcba73df82a7744 -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md Cargo.lock` was empty; HEAD still matched. Final status also showed concurrently created invoice/transport review reports; neither was authored or modified by this audit. `git diff --check` passed for tracked changes.
2. `cargo test --offline --locked -p szamlazz-agent --lib ops:: --test response_headers --test response_completion --test response_namespaces --test upstream` — **136 operation unit tests passed**. The `ops::` filter also filtered out all four integration targets; their zero-test runs are **not** claimed as coverage.
3. Corrected integration invocation: `cargo test --offline --locked -p szamlazz-agent --test response_headers --test response_completion --test response_namespaces --test upstream` — **40 passed, zero failed**: headers 14, completion 4, namespaces 11, upstream 11. The corpus is present and relevant corpus tests exercised it. Broader operation tests do not expand this report's audit scope.
4. `cargo build --offline --locked -p szamlazz-agent` — passed.
5. Workspace-linked scratch reproduction:

   ```sh
   rustc --edition=2024 /tmp/opencode/mutations-837dad0-check.rs \
     -L dependency=/home/laborant/szamlazz-rs/target/debug/deps \
     --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib \
     -o /tmp/opencode/mutations-837dad0-check
   /tmp/opencode/mutations-837dad0-check
   ```

   Passed all assertions: four numberless-credit rejections, four number-header success controls, nested/duplicate diagnostic retention for each mode, and deletion true/1 versus false/0 controls. These are synthetic public-parser inputs; no credentials, network or request effects.

## 9. Explicit omissions and limits

- **Storno response implementation**, invoice-create responses/preview, other operations, Restate recovery, IPN receiver and other crates were not audited for completeness. S3 was fetched as requested; fetching is distinct from including its parser in scope.
- No live/probe suite, vendor request, credential use, email delivery, PDF rendering, batch rollback, paid-state mutation, issuer-tax-number matching, empty-entry mutation, external-id-only targeting or account-specific aggregator/guardian test was performed.
- No full XSD validator was executed in this audit. Field/type/order/cardinality comparison was manual against freshly fetched schemas, supported by existing generated-output and corpus tests. Prior reports' XSD validation results are not this audit's results.
- No complete transport, retry budget, custom HTTP client, wasm/browser, fuzz/performance or XML-parser security audit is claimed. Native POST/framing and response helpers were traced only where these operations depend on them.
- All current English descendants of the requested categories were fetched, including hidden inline schema/example tabs. Hungarian pages fetched are individually listed in §2; this is not a full bilingual-site crawl. External account-settings/contact links were not opened. Knowledge-base IPN articles, decorative assets, template preview PDFs and entire PHP/legacy ZIP distributions were not fetched; operation examples/XSDs on the current pages were read in full.
- Original historical live logs are unavailable in the repository. No synthetic test or published example was promoted into live evidence. The remaining Q1 cannot be settled by repeating a parser test or by assuming all accounts emit the same optional fields.

**Bottom line:** retain the current mutation request writers and accepted live deviations. Credit/deletion cover the documented response fields. Clarify the success-specific credit echo guarantee before treating Q1 as a vendor regression or changing reported-identity semantics.
