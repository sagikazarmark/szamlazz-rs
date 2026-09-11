# Számla Agent mutation fidelity review — eec57fc

**Reviewed HEAD:** `eec57fcf3036d93cd68c9cfc017338cd3020e7dd`.

**Official documentation retrieved:** 2026-09-11.

**Conclusion:** no confirmed implementation defect in invoice storno, credit-entry registration/explicit clearing, or proforma deletion. One **P3 vendor-contract clarification** remains: successful credit acknowledgements without a reported invoice number are rejected. The schema permits that shape, but neither success-specific permission nor a live occurrence has been established. The September 11 clearing executions materially strengthen the evidence for clearing itself; they do not settle that universal response guarantee.

## 1. Scope, method and evidence boundary

This is a fresh full-source review of the scoped operations, **including the storno response**, rather than a diff review. Read the complete `src/ops/{storno,credit_entry,proforma,envelope}.rs`, then traced their shared XML, numeric, header, credential, type and client paths. Reviewed relevant tests and recorded observations. Current implementation and live official pages were compared before consulting earlier mutation reports for leads and disposition.

Source paths below are relative to `crates/szamlazz-agent/`, except explicitly workspace-qualified `docs/`, `fixtures/` and `scripts/` paths. Line numbers refer to the reviewed revision. Official citations use URLs and quoted statements or schema declarations because rendered pages have no stable source-line numbering.

- Started with all three requested category URLs, then fetched all their current operation descendants: request, response, XML/example/XSD, and credit's IPN page. Each current descendant was read in **English and Hungarian**, including both code-block tabs.
- Followed relevant shared authentication, request, error, session, template, notification and simplified-image rules; cross-checked legacy XSD pages, direct XSD downloads and first-party PHP documentation.
- No credentials or local secret files were read. No Számla Agent account request, live/probe execution, email or support submission was made. Network use was public documentation/XSD GETs only.
- No source, tests, fixtures or configuration were edited. Only this report was authored, using `apply_patch`.
- **No cargo suite or scratch executable was run:** the parent runs the cargo suite. Reproductions below are source-traced public-parser inputs, explicitly not new execution evidence. Existing test definitions are inspected coverage, not passing-test claims.
- HEAD matched the requested revision at initial and later checks. The tree was initially clean; unrelated concurrent changes subsequently appeared under `crates/restate-szamlazz/`. They were not reviewed or changed here. No change under the reviewed Agent crate was present at the later check.

### Evidence levels

1. Fresh official pages/XSDs establish published syntax and statements, not actual account behavior.
2. `docs/szamlazz-hu-behaviour.md` records historical account observations, with explicit provenance limits at lines 3–37. The September 3/6/7 raw logs are not in this repository.
3. `docs/research/2026-09-11-credit-clearing-live.md:12–50,64–75` records two actual executions and captured-output transcriptions. Raw HTTP bodies/headers were not archived. Continuity with the historical account was not established.
4. Synthetic fixtures, tests, golden files and probe definitions establish what code expects or checks. They do not independently establish vendor execution, universal guarantees, or rollback.

## 2. Fresh official-source register

All URLs in this section were fetched in this review. Current documentation pages showed `v202608271632`; legacy `/xsd` pages showed `v202606031507`. Those are site-build labels, not dates or guarantees for individual claims.

| ID | Sources | Coverage |
|---|---|---|
| S0 | [Storno category](https://docs.szamlazz.hu/agent/category/reversing-invoice) | Requested starting point and current descendants |
| S1 | [Storno request EN](https://docs.szamlazz.hu/agent/reversing_invoice/request), [HU](https://docs.szamlazz.hu/hu/agent/reversing_invoice/request) | POST, multipart field, required original number, contradictory external-id wording |
| S2 | [Storno XML/XSD EN](https://docs.szamlazz.hu/agent/reversing_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml) | Every request element, type, order, cardinality and example |
| S3 | [Storno response EN](https://docs.szamlazz.hu/agent/reversing_invoice/response), [HU](https://docs.szamlazz.hu/hu/agent/reversing_invoice/response) | Both versions, all headers, structured examples and complete response XSD |
| S4 | [Legacy storno XSD](https://docs.szamlazz.hu/agent/reversing_invoice/xsd), [download](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd) | Independent request-schema comparison |
| C0 | [Credit category](https://docs.szamlazz.hu/agent/category/registering-credit-entry) | Requested starting point and descendants |
| C1 | [Credit request EN](https://docs.szamlazz.hu/agent/credit_entry/request), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/request) | Multipart dispatch |
| C2 | [Credit XML/XSD EN](https://docs.szamlazz.hu/agent/credit_entry/xml), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/xml) | Complete settings/entry sequences, replacement, zero-to-five entries |
| C3 | [Credit response EN](https://docs.szamlazz.hu/agent/credit_entry/response), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/response) | Both versions, headers, examples and complete operation-specific XSD |
| C4 | [Legacy credit XSD](https://docs.szamlazz.hu/agent/credit_entry/xsd), [download](https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd) | Independent request-schema comparison |
| C5 | [Credit IPN EN](https://docs.szamlazz.hu/agent/credit_entry/other), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/other) | Asynchronous payment-status notification, distinct from synchronous acknowledgement |
| D0 | [Deletion category](https://docs.szamlazz.hu/agent/category/deleting-a-pro-forma-invoice) | Requested starting point and descendants |
| D1 | [Deletion request EN](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), [HU](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/request) | Multipart dispatch |
| D2 | [Deletion XML/XSD EN](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml) | Both selectors, complete request schema, HU all-matches rule |
| D3 | [Deletion response EN](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response), [HU](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/response) | Success, 335, critical text/HTML, complete response XSD |
| D4 | [Legacy deletion XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xsd), [working request download](https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd) | Request-schema comparison; working download revalidated from a historical lead |
| X | [Shared invoice-response XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Storno envelope; includes PDF unlike credit's inline response schema |
| B1 | [Authentication EN](https://docs.szamlazz.hu/agent/basics/authentication), [HU](https://docs.szamlazz.hu/hu/agent/basics/authentication) | Either credential form; case sensitivity; legacy key in both login fields |
| B2 | [Sending requests EN](https://docs.szamlazz.hu/agent/basics/sending-requests), [HU](https://docs.szamlazz.hu/hu/agent/basics/sending-requests) | Endpoint/action table, XML file, case-sensitive tags, validation |
| B3 | [Error handling EN](https://docs.szamlazz.hu/agent/basics/error-handling), [HU](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Published codes, five-send limit, version-1 text, no automatic retry-until-success |
| B4 | [Session cookies EN](https://docs.szamlazz.hu/agent/basics/session-cookie), [HU](https://docs.szamlazz.hu/hu/agent/basics/session-cookie) | Reuse, 90-minute inactivity and refresh advice |
| T | [Templates EN](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template), [linked knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok) | Six tokens, omission, conflicting human labels/casing |
| E | [Notification EN](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/email-notification) | Shared email text/BBCode; create-specific controls |
| I | [Simplified image EN](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/travel-agency) | Storno inheritance and template override |
| PS | [PHP storno EN](https://docs.szamlazz.hu/php/sztorno-szamla-generalas), [HU](https://docs.szamlazz.hu/hu/php/sztorno-szamla-generalas) | Header/email inputs; wrapper types distinguished from XML types |
| PC | [PHP credit EN](https://docs.szamlazz.hu/php/jovairas), [HU](https://docs.szamlazz.hu/hu/php/jovairas) | Wrapper defaults and `leiras` as credit-entry remark |
| PD | [PHP deletion EN](https://docs.szamlazz.hu/php/dijbekero-torles), [HU](https://docs.szamlazz.hu/hu/php/dijbekero-torles) | Multiple deletion and stated rollback |
| PR | [PHP response EN](https://docs.szamlazz.hu/php/valasz-feldolgozas), [HU](https://docs.szamlazz.hu/hu/php/valasz-feldolgozas) | Issuance can succeed despite notification failure |

Failed GETs, not sources of schema contents:

- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd` and `.../xmlszamladbkdelvalasz.xsd`: **404**. These are the examples' schema locations, upgraded from HTTP.
- `https://www.szamlazz.hu/szamla/docs/xsds/agentdbkdel/xmlszamladbkdel.xsd` and `.../xmlszamladbkdelvalasz.xsd`: **404**. These were candidate locations, not published working links.

All three working **request** downloads agree with current EN/HU inline element sequences, types and cardinalities. HU credit names its root complex type while EN uses an anonymous type; the accepted shape is equivalent. Deletion's **response** schema was read in full from both inline pages. No downloaded schema was silently repaired or merged.

## 3. Findings

### Confirmed defects

**None established.** No missing documented request field, wrong ordering, namespace or multipart selector, incorrect five-entry bound, missing clearing capability, lost documented response field, or incorrect handling of the recorded mutation rejections was found.

### Q1 — P3 clarification: a successful credit acknowledgement needs an undocumented nonblank echo

**Potential severity if the vendor emits it:** P2 compatibility failure.

**Confidence:** high in the source-traced behavior and schema allowance; no live numberless success and no success-specific vendor answer. This is not counted as a confirmed operational bug.

**Exact implementation:** `src/ops/credit_entry.rs:316–322` accepts the verdict through `xml::valasz`, then requires `body.invoice_number(response).ok_or(ParseError::Missing("szamlaszam"))?`. `InvoiceBalance::invoice_number` is mandatory at `:256–258`. `ClearCreditEntries::parse` delegates at `:247–249`. Body/header selection is `src/ops/envelope.rs:120–133,326–329`.

**Official quote:** [C3 EN](https://docs.szamlazz.hu/agent/credit_entry/response) says additional header data **“may also arrive”** and **“Elements marked `minOccurs="0"` may not always be included.”** Its XSD declares:

```xml
<element name="sikeres" type="boolean" maxOccurs="1" minOccurs="1"/>
<element name="szamlaszam" type="string" maxOccurs="1" minOccurs="0"/>
```

[C3 HU](https://docs.szamlazz.hu/hu/agent/credit_entry/response) likewise says optional elements **“nem mindig jelennek meg.”** The XSD describes success and failure together; it does not express a success-dependent requirement.

**Minimal reproduction for the parent (not executed here):**

```rust
use szamlazz_agent::ops::credit_entry::ClearCreditEntries;
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{ParseError, ResponseError};

let response = RawResponse::new::<&str, &str>([], br#"
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres><kintlevoseg>1270</kintlevoseg>
</xmlszamlavalasz>"#.to_vec()).with_status(200);
assert!(matches!(
    ClearCreditEntries::new("I-1").parse(&response),
    Err(ResponseError::Parse(ParseError::Missing("szamlaszam")))
));
```

The same path applies to populated replacing/additive registration. Omitting `kintlevoseg` changes nothing. Adding a nonblank `szlahu_szamlaszam` header makes identity available; the parser never substitutes the requested number. A returned number different from the request is retained as reported, rather than silently rewritten.

**Impact:** a true acknowledgement and any returned balance become an uncertain parse failure. A caller incorrectly repeating additive registration could duplicate entries; repeating replacement/clear could overwrite intervening state. The parser sends nothing and the parse error remains `OutcomeClass::Unknown` (`src/error.rs:752–769`), so this is not a demonstrated duplicate-write defect.

**Why this remains unresolved after the new live evidence:** `docs/research/2026-09-11-credit-clearing-live.md:19–26,69–74` records both clearing states succeeding with the expected number and full outstanding gross. Lines 64–67 explicitly say raw response channels were not captured. This proves those two operations, not every account/version's nonblank echo. Current official success examples are numbered; failures are not. Neither the schema alone nor the Rust result type settles the conditional guarantee.

**Next action:** obtain the success-specific guarantee requested in `docs/research/2026-09-11-agent-vendor-clarification.md:18–50`. Its status at lines 3–7 is **not sent; no answer received**. If numberless success is supported, model successful acknowledgement with optional **reported** identity and preserve requested identity separately. Do not manufacture a vendor echo. Apply the resolution to both registration and clearing. Until then the existing documented decision to retain the result contract (`docs/research/2026-09-10-credit-entry-success-question.md:22–29`) is defensible.

## 4. Exhaustive request comparison

Notation: `?` means `minOccurs=0`; all singleton fields have `maxOccurs=1`. Arrows are XML order. No XSD default attributes establish the constructors' defaults. A required `string` element does not itself require nonempty content.

### Shared routing and values

[B2](https://docs.szamlazz.hu/agent/basics/sending-requests): the operation is selected **“using the name of the form field containing the XML file.”** `src/wire.rs:7–14,66–99,398–412` supplies the HTTPS endpoint, file-part disposition/filename, `text/xml`, multipart framing and validation. `src/client.rs:374–405` POSTs those bytes and passes completed status/headers/body to the operation parser.

`src/xml.rs:157–179,586–638` emits XML 1.0/UTF-8 with a default namespace, escaped text, true/false booleans, plain Decimal text and civil dates. HTTP namespace identifiers must not be changed to HTTPS merely because the transport is HTTPS. `None` omits an optional scalar; `Some("")` emits empty content. `xsi:schemaLocation` is a validation hint, not a missing business field.

[B1](https://docs.szamlazz.hu/agent/basics/authentication) permits **“either an Agent key … or a username and password.”** The writer emits either `szamlaagentkulcs` or `felhasznalo → jelszo` in the settings block, preserving case/content. Deletion's example showing all three does not require all three. The legacy same-key-in-both-fields form is representable through `Credentials::user_password` (`src/credentials.rs:53–81`).

Checked requests reject XML 1.0-forbidden characters (`src/wire.rs:405–440`) and nonpositive outbound date years (`src/xml.rs:19–35`). `write_xml` is explicitly unchecked (`wire.rs:367–373`); `to_wire`/`Client::send` is the checked boundary. Storno validates both optional dates (`storno.rs:166–170`); registration validates every entry date before its empty-replace guard (`credit_entry.rs:278–289`). Years 1–9999 are a deliberate supported subset, not a new vendor date rule.

### Storno — S1/S2/S4

| Exact XML surface | Current mapping and assessment |
|---|---|
| `action-szamla_agent_st`; `xmlszamlast`; `http://www.szamlazz.hu/xmlszamlast` | Exact at `storno.rs:162–176` |
| `beallitasok → fejlec → elado? → vevo?` | Exact at `:178–213`; optional seller/buyer containers are emitted even when empty, which is schema-valid |
| Credentials → `eszamla → szamlaLetoltes` | Required bools, `e_invoice`/`download_pdf`, both default false (`:59–76,144–145,179–181`) |
| `szamlaLetoltesPld?` | `download_copies: Option<u8>`, default absent, emitted at `:182–184`; XSD int, but S2 explicitly says the server no longer processes this deprecated field |
| `aggregator? → guardian?` | Optional String/bool, absent by default, exact order (`:82–85,185–188`) |
| `valaszVerzio? → szamlaKulsoAzon?` | Shared constant `2` then optional external id (`:189–190`; `src/ops.rs:28–32`) |
| Header `szamlaszam` | Required original `InvoiceNumber`, emitted at `:193` |
| `keltDatum? → teljesitesDatum?` | Optional `Date`, absent by default, exact order (`:99–121,194–195`) |
| `megjegyzes? → tipus?` | Optional comment followed by fixed `SS` (`:196–197`), matching the operation/example |
| `szamlaSablon?` | Optional open template token after `tipus` (`:198–200`) |
| Seller `emailReplyto? → emailTargy? → emailSzoveg?` | All independently optional through `SellerEmail`, exact order (`:202–208`; `types.rs:1022–1032`) |
| Buyer `email? → adoszam? → adoszamEU?` | All independently optional, exact order (`:209–213`) |

All optional fields default absent (`storno.rs:138–159`). The complete six-token template mapping is present at `types.rs:983–1019`: `SzlaMost`, `SzlaAlap`, `SzlaNoEnv`, `Szla8cm`, `SzlaTomb`, `SzlaFuvarlevelesAlap`, plus `Other(String)`. `Default` explicitly selects `SzlaAlap`; it does not mean omission.

S2's buyer annotation says **“If the buyer's tax number is missing from the original invoice, it can be provided in this block.”** Exposing both tax-number strings is correct; the sources do not establish arbitrary replacement of an already-present number. No missing storno `sendEmail`, language, item, external-id-only selector or arbitrary `tipus` capability is established. The create notification page does not prove create-only attachment/suppression settings apply to storno. [I](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) explicitly says reversal **“inherits the state of the original document”** for simplified image, so no storno `simpleItems` switch is missing.

### Registration and clearing — C1/C2/C4

| Exact XML surface | Current mapping and assessment |
|---|---|
| `action-szamla_agent_kifiz`; `xmlszamlakifiz`; matching HTTP namespace | `credit_entry.rs:274–295`; clear shares action at `:237–244` |
| `beallitasok → kifizetes{0..5}` | Correct at `:296–311`; settings required, entries optional and bounded |
| Credentials → `szamlaszam` | Required invoice number (`:297–298`) |
| `adoszam? → additiv` | Optional issuer tax number, then required bool (`:299–300`) |
| `aggregator? → valaszVerzio?` | Optional aggregator followed by shared `2` (`:301–302`) |
| Entry `datum → jogcim → osszeg → leiras?` | `CreditEntry { date, title, amount, description }`, exact order (`:21–45,304–310`) |
| At most five entries | Private collection, `push`, `TryFrom<Vec<_>>`, custom serde all enforce the bound (`:49–142`); dereference exposes a slice, not vector growth |
| Replacing registration | Constructor defaults `additive=false`; at least one entry required by local guard (`:177–190,278–289`) |
| Additive registration | Explicit true retains prior entries; zero entries accepted locally; no automatic splitting into multiple requests |
| Explicit empty replacement | `ClearCreditEntries` has number, optional issuer tax number/aggregator, no entries/additive field (`:193–249`); emits false and zero entries through the same writer |

C2: **“If true, former credit entries are retained; otherwise they are replaced.”** Its `<element name="kifizetes" ... maxOccurs="5" minOccurs="0">` permits clearing's wire shape. The ordinary unfinished registration guard is an explicit-intent boundary, not removal of the zero-entry capability. Clear deliberately bypasses that guard only; checked serialization still validates XML characters. `#[serde(deny_unknown_fields)]` on clear prevents a registration-shaped JSON object from silently becoming a clear.

`PaymentMethod::Other(String)` preserves arbitrary `jogcim` text (`types.rs:588–687`); known methods map to exact tokens. Decimal covers finite representable values, with no invented sign or two-decimal request restriction. `leiras` is sent as description and query `megjegyzes` is read as comment (`query_xml.rs:507–533,1052–1085`), consistent with PC's **“Credit entry remark”** wording. Query-only bank account/transaction/exchange-rate fields are not missing registration fields.

HU C2 says **“ha megadod a kiállító adószámát, a rendszer a bejövő kifizetést a megfelelő számlához rendeli”**: the issuer-tax-number name is appropriate. The EN example's “incoming receipt” wording does not turn this into a receipt operation. Issuer matching and aggregator entitlements remain untested. PC's additive=true, today/transfer/0.0 defaults are PHP wrapper defaults, not a wire requirement overriding explicitly written Rust values.

### Proforma deletion — D1/D2/D4

| Exact XML surface | Current mapping and assessment |
|---|---|
| `action-szamla_agent_dijbekero_torlese`; `xmlszamladbkdel`; matching HTTP namespace | Exact at `proforma.rs:60–67` |
| `beallitasok → fejlec` | Both required, correct order (`:69–77`) |
| Settings: credentials only | Complete; no response-version/PDF field exists |
| Header `szamlaszam? → rendelesszam?` | Enum emits exactly one at `:72–77`; lowercase `rendelesszam` is correct, unlike query's `rendelesSzam` |
| Selection scope | Number is one proforma; order means all matches (`:14–30,35–43`) |

The XSD permits both/neither selectors structurally; the enum deliberately selects one documented alternative. Selected strings can still be empty (`types.rs:22–38`, `proforma.rs:29`), as XSD `string` has no nonempty facet. This is not a proven valid empty-target business request, nor evidence of vendor behavior for both selectors. No external-id deletion selector is documented.

HU D2 explicitly says **“Ha azonos rendelésszámmal több díjbekérő is van a számlázási fiókban, akkor a törlés az összes díjbekérőre vonatkozik.”** PD independently says **“Based on the order number, multiple proforma invoices can be deleted”**, with rollback on member failure. The implementation sends one batch operation, not a loop or latest-match deletion. Its `()` success claims neither a count nor a deleted-number list. Paid retention is caller policy (`proforma.rs:4–6,35–39`), consistent with historical D3.

## 5. Complete response and rejection comparison

### Versions, fields and channels

S3 says version 1/omitted gives `xmlagentresponse=DONE;{invoice_number}` or raw PDF according to download, whereas version 2 gives structured XML with optional base64 PDF. C3 version 1 gives `xmlagentresponse=DONE`; version 2 gives structured XML. Both writers always choose 2. Rejecting ordinary version-1 text/raw PDF therefore does not lose a response format these requests select. Deletion has its own fixed XML reply and no version switch.

| Documented field/channel | Current behavior |
|---|---|
| Storno/credit root and namespace | `xmlszamlavalasz`, `http://www.szamlazz.hu/xmlszamlavalasz`; `envelope.rs:18–21`, storno `:218–220`, credit `:316–317` |
| Deletion root and namespace | `xmlszamladbkdelvalasz`, matching namespace; `proforma.rs:82–87` |
| Required `sikeres` | Unique scalar verdict; true/false/1/0 supported; missing, duplicate and malformed nonempty tokens fail (`xml.rs:473–500,768–800`). Empty-value legacy boundary discussed below |
| `hibakod?`, `hibauzenet?` | False returns `ApiError`; absent/blank code is `Absent`; unknown token preserved; malformed optional diagnostic cannot erase readable facts (`xml.rs:478–522`) |
| `szamlaszam?`, encoded `szlahu_szamlaszam` | Trimmed nonblank body first, once-decoded header fallback (`envelope.rs:120–133,326–329`); required for storno and balance results |
| `szamlanetto?`, raw `szlahu_nettovegosszeg` | Optional exact Decimal; body first then header (`envelope.rs:158–168,226–231`; credit `:323–328`) |
| `szamlabrutto?`, raw `szlahu_bruttovegosszeg` | Same (`envelope.rs:232–237`; credit `:329–334`) |
| `kintlevoseg?`, raw `szlahu_kintlevoseg` | Same (`envelope.rs:238–243`; credit `:335–340`); header supported from observations although not listed in S3/C3 header tables |
| `vevoifiokurl?`, `szlahu_vevoifiokurl` | Body first, then decoded header (`envelope.rs:135–143`; credit `:342`); XML receives entity decoding only |
| `szlahu_fizetesmod` | Once-decoded open `PaymentMethod` on both results (`envelope.rs:245,318–324`; credit `:341`) |
| Storno `pdf?` | Base64 decoded to bytes (`envelope.rs:145–148,246`; `types.rs:104–117`); whitespace wrapping accepted; absent is None |
| Observed storno `szlahu_id` | Auxiliary nonnegative i64; absent/blank/malformed/negative becomes None (`envelope.rs:331–342`) |
| Deletion successful verdict | `()`; D3 declares no payload beyond the verdict (`proforma.rs:62,82–87`) |
| Deletion critical text/HTML | Parse/status error with bounded excerpt, never success (`proforma.rs:155–169`; `error.rs:680–703`) |

The exact S3/X sequence is `sikeres → hibakod? → hibauzenet? → szamlaszam? → szamlanetto? → szamlabrutto? → kintlevoseg? → vevoifiokurl? → pdf?`. C3 stops before PDF. D3 has only `sikeres → hibakod? → hibauzenet?` and declares its code as int. Every documented field is accounted for. Response order is accepted leniently; this parser is not an XSD validator. Foreign namespaces cannot supply protocol fields; complete XML/UTF-8/root/namespace checking precedes projection (`xml.rs:185–378`).

Blank body totals are treated as absent, permitting header fallback; nonblank malformed body totals do not fall back. Present malformed/blank numeric headers fail, absent channels yield None. XML finite dot/exponent forms and HTTP decimal-comma forms are distinguished (`envelope.rs:344–371`; `number.rs:63–141`). Numeric headers are not percent-decoded; XML URLs retain literal `+` and percent escapes. Rejecting unrepresentable double values, infinity/NaN, overflow and precision loss is an explicit exact-money domain policy, not a failure to support normal documented monetary values.

Credit uses the shared `Body`, including its optional PDF field, but does not decode or expose a PDF. Credit's own XSD has no PDF, currency, VAT total or document id. A nested/repeated PDF extension could still fail shared payload deserialization; that is a robustness boundary for an undocumented field, not a missing documented response. No blanket arbitrary-extension compatibility is claimed.

### Verdict precedence and errors

`wire.rs:291–310` applies **nonblank down header → nonblank error-code header → known non-2xx status → body**. Credit/deletion use `xml.rs:528–564`; storno judges the one numbered-56 exception in `envelope.rs:179–249`. This is explicit library policy, not a vendor-specified tie-breaker for contradictory channels. A body-only 463 at HTTP 200 is an API error; without a header code at HTTP 500 it is an uncertain HTTP-status error. Success-number headers do not override that status.

| Code/case | Source and implementation assessment |
|---|---|
| 3/135/136/164 | B3 authentication/access refusals; typed, credential-classified and Rejected for this exchange (`error.rs:334–351,386–394`). Exact server processing order is not claimed; prior unresolved writes remain unresolved |
| 53/57 | B3: **“Missing XML file”**, **“XML reading error”**; typed Rejected (`error.rs:62–77,388–390`), distinct from a local `RequestError` before transport |
| 54 | B3 e-invoice permission/certificate refusal; typed Rejected |
| 1/55 | B3 maintenance/signing failure; Unknown, with potentially transient query hint, never automatic write permission (`error.rs:319–332,375–381`) |
| 14/221/352 | Historical B5/B7/B3 storno refusals; typed Rejected; explicit operation tests at `storno.rs:346–364` |
| 463 | Historical D8 credit on reversed invoice, body-only; parsed as `PaymentOnReversedInvoice`, Rejected for the exchange (`credit_entry.rs:491–505`, `error.rs:179–184,412`) |
| 335 | D3 example and historical D1/D2/D4; `ProformaNotFound`, Rejected, not successful replay (`proforma.rs:127–152`, `error.rs:153–154,403`) |
| 7 | Operation-dependent missing data/target; NotFound classification is not proof an earlier mutation failed (`error.rs:43–55,450–454`) |
| Unknown/absent code | Remains Unknown (`error.rs:375–381,457–471`), not a newly invented rejection |
| Parse/status/down/incomplete transfer | Unknown; incomplete transfer retains status/headers/source and never invents a completed empty body (`client.rs:66–91,117–125,385–405`) |

**Numbered 56:** PR confirms that issuance can succeed while notification delivery fails. The precise code-to-meaning corroboration is attributed in `error.rs:71–75` and `recovery.md:47–49` to official PHP 2.12.4 source, not a live observation; this review fetched the PHP documentation, not a fresh package/source extraction. Current storno handling requires a usable reported number, retains `notification_delivery_failed`, and drops unusable optional totals/PDF rather than hiding the issued document. Numberless 56 remains an error. A non-56 body refusal overrides a header 56. Malformed XML or ambiguous/nested identity cannot be promoted through header fallback (`envelope.rs:180–217,285–315`). Credit/clear and deletion do not treat a document number plus 56 as proof of their different mutations.

**Empty verdict boundary:** `xml.rs:481` uses `flexible_bool`; `:775–778` interprets empty/whitespace as false. The published required boolean does not permit empty lexical content. Thus `<sikeres/><hibakod>463</hibakod>` can retain a known refusal, and an empty verdict without a code yields `Absent/Unknown`, rather than a malformed-boolean parse error. This is existing leniency, not faithful XSD validation. It is not a plain-success bypass and no vendor emission or live failure is established. Tests for strict required receipt facts should not be described as proving strictness of this shared verdict; no change is proposed as a confirmed mutation defect here.

### Storno success is not proof of a newly created reversal

`StornoInvoice::parse` returns the shared numbered document (`storno.rs:218–220`), including a same-number success-shaped no-op. It does not falsely synthesize a new number or a fresh-issuance flag. `CreatedInvoice::reverses` (`envelope.rs:61–87`) is explicitly a **reply-only heuristic**: different number plus known nonpositive gross. It does not check type/reference, perform a query or establish creation by this call.

- A repeat can return the pre-existing SS unchanged.
- A D/SL storno can echo the requested number and reverse nothing.
- A changed number with absent/positive gross is inconclusive, **not** proof of no reversal.
- Zero in the predicate is deliberate local policy; neither zero-total originals nor negative-original/positive-storno acceptance was verified live.
- A success without a number is refused by `parse_issued` (`envelope.rs:268–279`), preserving uncertainty. It is not an invoice-preview outcome, and the original's number cannot substitute for the reversal's.

These qualifications are present in `storno.rs:22–50` and the helper's rustdoc. The current code does not retain the older overclaim that every false heuristic means not-stornoable.

## 6. Recorded live behavior and justified deviations

The following adjudications use **recorded executions**, not the presence of probe source. `docs/szamlazz-hu-behaviour.md` line references are current; historical design-consequence wording is not independently treated as a vendor guarantee.

| Behavior | Evidence and disposition |
|---|---|
| Storno external id attaches to SS; original retains its id; repeat discards a new id | Behavior `:72–79,104` (B6/B4x/XPRB-P4/P48-P6). Preserve `storno.rs:86–98` despite S1's original-lookup wording. Reusing the original's id can make the SS its newest holder |
| Repeat storno is an existing-SS echo | Behavior `:95` (B4), same number/id/negative gross. Preserve qualified observation and recovery guidance; no universal account guarantee or new execution here |
| D/SL storno is a same-number no-op | Behavior `:96` (B5); shared result and heuristic retain this distinction |
| Storno 14/221 | Behavior `:97–98`; typed refusals are justified although absent from the current general error table |
| Non-today storno issue date → 352, including paper | Behavior `:99`; omitting `issue_date` is appropriate. The historical date in S2/PS's sample is not present-day acceptance evidence |
| Fulfillment omission copies original; equal and differing explicit dates accepted | Behavior `:101–105` (P48). Keep optional input and explicit-original-date guidance (`storno.rs:108–121`). The low-level Agent type is not the worker's stricter request contract; ADR 0007 is internal decision provenance, not a fresh NAV-source verification |
| Appearance is request-selected, including both mismatches | Behavior `:18–26,106–107` (P73). Constructor false is not inherited appearance; current rustdoc tells callers to derive it from the original |
| Storno removes original's recorded credit entries | Behavior `:89` (B8). Current operation docs disclose this; no claim that prior credits net the SS outstanding |
| Paid proforma can be deleted; successful delete has no headers; missing/repeat/consumed → 335 | Behavior `:114,118–119` (D1–D4). Current paid-policy/all-matches docs and verdict-only result are appropriate |
| Replace `[100]` then `[200]`; additive 50 retains prior 200; five entries accepted with different query ordering | Behavior `:142,144` (D7). No sorting/order promise or deduplication is inferred |
| Reversed invoice credit → body-only 463 | Behavior `:145,151` (D8). Clear shares this parser; applying 463 to a clear or SS target is a synthetic/semantic expectation, not a new recorded vendor execution |
| Empty replacement on populated and already-empty invoices | Behavior `:29–33,143`; clearing record `:14–26,34–50`. CTEST-2026-13 had a queried 100 HUF entry before clear; CTEST-2026-15 was empty. Both acknowledged their number, outstanding 3136 HUF, then queried `[]`. Cleanup SS numbers were 14/16 |
| Clearing cleanup evidence | Clearing record `:28–32`: returned SS was queried and checked for distinct number/type/original reference; original reversal marker was not queried again after cleanup. Do not strengthen that into a full post-cleanup original-state verification |
| Comma monetary header | Behavior `:170` (P60); header normalization supports it without allowing comma in XML amounts |
| Code 56 not triggered | Behavior `:163,181,190–191`. Current handling is source-derived/synthetic, not account-proven |

No raw HTTP channel guarantee, concurrent-write guarantee, empty-additive execution, sixth-entry vendor refusal code, batch rollback execution, incoming-invoice issuer matching, aggregator/guardian entitlement, or all-account behavior follows from these records. The historical account and September 11 account are not assumed identical.

## 7. Remaining vendor contradictions and deliberate boundaries

1. **Storno external id:** [S1 EN/HU](https://docs.szamlazz.hu/agent/reversing_invoice/request) says the required number can optionally be accompanied by an external identifier **“if it was set when the invoice was created.”** S2 describes subsequent identification without clearly naming the original versus SS. Recorded B6/XPRB behavior establishes assignment to SS when number is present. Do not change to external-id-only targeting or reuse the original's id on that prose alone.
2. **Credit acknowledgement optionality:** Q1 remains open. The new clearing evidence removes “clearing has never been executed” as a valid claim; it does not remove the success-echo question.
3. **Malformed published success examples:** freshly fetched S3/C3 still have raw `&` in `vevoifiokurl`; S3 also abbreviates base64 with `....`. Rejecting these literal examples is correct. `tests/upstream.rs:445–510` explicitly escapes URLs and substitutes synthetic PDF only for separate controls. Those repairs do not reconstruct actual vendor responses. Positive example storno totals do not establish any original's reversal.
4. **XML/header parity:** S3/C3 say **“With an XML response, the same data is also in the XML body”**, but their schemas declare no payment-method element. Header-only payment-method interpretation follows the concrete schema. Payment-method/URL encoding is not fully specified by each row; decode-once is current tested library policy.
5. **Template labels:** Agent T labels `SzlaAlap` traditional and `SzlaNoEnv` envelope-friendly; its linked knowledge base labels the opposite and shows incorrectly cased XML/token examples. Current code preserves exact Agent/XSD tokens and explicitly distinguishes a named Default from omission. No rendered PDF was inspected; swapping mappings based on one contradictory page is unjustified.
6. **Deletion scope/atomicity:** EN D2 omits HU's all-matches sentence; PD corroborates multiple deletion and says **“If deleting any of the proforma invoices fails, we perform a rollback.”** That is a vendor statement, not an observed transaction/late-execution guarantee. Repeating order deletion can include new matches regardless of a previous latest-match query.
7. **Schema locations:** the example deletion download URLs remain broken, but the `dijbekerodel` request download works. Inline response schema remains usable documentation. A 404 is not evidence that the operation lacks a schema.
8. **Supported subsets:** fixed SS, fixed version 2, finite Decimal, positive-year civil dates, ignored copies as u8 and exactly one deletion selector are deliberate model choices. Arbitrary XSD string/int/double allowance alone does not demonstrate a lost useful business capability. Optional empty strings remain distinguishable from omission on the wire.
9. **Specialized settings and emails:** aggregator/guardian are fully emitted, but operational contracts are not defined by the scoped pages. Email strings/BBCode are passed through; the create notification page does not settle storno resend/email-suppression semantics. Test-account notification routing differs from production, per E.
10. **Recovery:** B3 permits **at most five total sends of the same request**, then operator intervention; this is distinct from five entries in one credit request. These operations contain no retry loop. `src/recovery.md:4–19,36–55` correctly distinguishes this exchange from earlier sends, warns against repeated additive/replace/clear writes and new matches on order deletion, and does not treat elapsed time or an empty query as settlement. Supplied HTTP clients retain their retry policy (`client.rs:193–207`); no comprehensive transport-budget or Restate recovery audit is claimed.
11. **IPN:** C5 describes asynchronous account-configured form-urlencoded POSTs, current paid-amount changes, delayed/retried delivery and latest-record behavior. Its fields are not synchronous `InvoiceBalance` fields and provide no completion guarantee for clearing. HU explicitly includes proformas. The receiver implementation is outside this review.

## 8. Tests and fixtures inspected

These are **inspected definitions, not tests run by this reviewer**. The parent owns suite execution and results.

| File / range | Relevant coverage |
|---|---|
| `src/ops/storno.rs:223–403` | Golden writer; minimal required/empty containers; aggregator/guardian/template order; normal/observed-shaped response, document id, negative balance/PDF; serde; D/SL no-op heuristic; 14/221/352; missing number; header precedence |
| `src/ops/credit_entry.rs:347–557` | Golden writer; aggregator/version order; optional balances/method/URL; JSON; rejects v1 text; bounded diagnostics; body-only 463; collection bound/idioms; unfinished replace versus empty additive |
| `src/ops/proforma.rs:91–181` | Both selectors; golden request; true/335/absent-code replies; critical text/HTML; wrong namespace |
| `src/ops/envelope.rs:373–725` | Success/header fallback; numbered/unnumbered 56; malformed optional metadata; real refusals; absent code; missing identity; non-2xx/down; body precedence; reply heuristic including synthetic zero |
| `src/xml.rs:909–1113` | Writer escaping/order; envelope roots/namespaces; verdict table; code/message absence; unknown code; verdict before malformed payload |
| `tests/clear_credit_entries.rs:6–55` | Explicit clearing serde/options, XML-character checking, rejects additive/entries JSON fields, preserves synthetic 463 and malformed-response uncertainty |
| `tests/client.rs:65–106` | Ordinary empty registration sends nothing; explicit clear sends exact multipart/XML once to a mock and reads numbered full-outstanding acknowledgement |
| `tests/schema_requests.rs:30–115,420–506` | Checked wire extraction, both credential forms; minimal/all/single-option storno, explicit-false guardian/empty email, additive/replace 1/5 entries, empty additive, clear with/without options, both deletion selectors |
| `scripts/check-agent-schemas.py:88–174` | Offline xmllint orchestration, provenance hashes, separate inline/download expectations, element-path coverage, negative controls. Merely running its ignored exporter is not XSD validation |
| `tests/request_dates.rs:134–173` | Optional storno date validation and every credit-entry date in both modes; shared table tests unsupported/boundary date inputs |
| `tests/response_headers.rs:9–105,148–283,299–337,406–602` | Optional diagnostics preserve facts; shared payment method; status/header/body order; 56 operation distinction; numeric/text decoding; comma/header fallback; optional metadata and ambiguous identity under 56 |
| `tests/response_completion.rs:12–87,150–192` | Completed XML, legal declarations/prologs/tails and malformed/truncated documents, including storno and deletion |
| `tests/response_namespaces.rs:105–165,260–279,332–357` | Foreign verdict/metadata rejection; valid aliases; no fabricated scalar by dropping a foreign child; duplicate singleton checks; legal attribute quoting |
| `tests/numeric_fidelity.rs:135–185` | Envelope totals refuse lossy/out-of-domain input; related exact finite-number controls |
| `tests/upstream.rs:416–529` | Current acquired storno/credit response examples; original source defects retained and transformations explicitly synthetic |
| `tests/golden/xmlszamlast.xml`, `xmlszamlakifiz.xml`, `xmlszamladbkdel.xml` | Read complete serialized expectations; project-generated, not live evidence |
| `tests/live.rs:44–169` | Definitions for lifecycle replace/additive, storno/original/reference/date/appearance/id/repeat and number deletion; not run here |
| `tests/probes.rs:15–145`; `tests/live_support/mod.rs:155–294` | Definitions for appearance mismatch and both clearing states; reversal/date selection, unresolved-write and cleanup behavior; only the dated clearing record establishes the two new executions |

Fixture provenance was checked in `fixtures/SOURCES.md:15–39,41–119,142–170`: official corpus is workspace-only and acquired, packaged fixtures are synthetic, golden XML is project-generated. Historical “only text example” wording records July; current structured response examples exist. Tests reading an absent official corpus may skip; no suite pass or exercised-corpus claim is made here.

Useful coverage boundaries: no current test definition proves empty-additive vendor behavior, numberless-success emission, batch rollback, PDF visual template labels, or the general success-echo guarantee. The complete-document/namespace tests directly exercise representative shared paths; they are not exhaustive combinations of every operation with every malformed XML input.

## 9. Historical leads rechecked against HEAD

Earlier `2026-09-11-agent-api-837dad0-mutations.md` and `2026-09-11-agent-api-61c334f-mutations.md` were consulted after direct code/document comparison. Their findings were not imported as current evidence.

| Lead | Current disposition |
|---|---|
| Missing aggregator/guardian/template/order | Not present: current writers and fresh inline/download schemas agree |
| Missing clear operation / zero entries never tested live | Obsolete: explicit clear exists and both dated September 11 executions are recorded |
| Nonpositive outbound dates generate invalid XSD date text | Checked boundary now refuses them (`xml.rs:19–35`, operation validators); not repeated as a finding |
| Credit optional diagnostic erases a refusal | Fixed: facts and diagnostic are decoded separately (`xml.rs:478–500`); relevant regression inspected |
| Numbered 56 loses usable body identity or trusts ambiguous identity via headers | Current fallback preserves unique body identity and refuses duplicate/nested identity (`envelope.rs:285–315`); regressions inspected |
| Missing storno payment method / comma amount handling | Present in shared helpers and result fields; cross-operation tests inspected |
| Paid-only proforma restriction / order deletion means one match | Current docs explicitly describe caller paid policy and all-matches scope |
| False `reverses` proves no reversal; zero is live-proven | Current rustdoc explicitly rejects both overclaims |
| Numberless credit success | Still Q1, with current lines and the new clearing evidence, not promoted into a demonstrated vendor regression |
| Probe definitions establish execution | Rejected as an evidence method; current behavior note and clearing record expressly separate definitions, executions and raw-channel limits |

## 10. Final assessment

The scoped request writers cover the current documented mutation surfaces, with correct action names, namespaces, fields, ordering, optionality and selectors. Storno, registration/clear and deletion responses preserve the documented information and recorded rejection outcomes. The source is consistent with the repository's shared-envelope, open-token, explicit-intent and evidence-qualified recovery conventions; no separate hard standards violation is established.

Retain the implementation and the justified observed deviations. Follow up **Q1** with the vendor before changing reported-identity semantics. Parent cargo results should be recorded separately; this report claims static review and fresh public-document retrieval only. Production-account behavior, zero/negative-original storno acceptance, email delivery, batch atomicity, concurrent mutations, all transport retry policies, and the full Restate worker are outside the established evidence.
