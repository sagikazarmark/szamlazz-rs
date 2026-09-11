# Independent mutation/envelope review — f83e5fd

**Revision:** `f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9` (HEAD).

**Fresh official-source acquisition and verification:** 2026-09-10.

**Result:** one confirmed **P3 malformed-response robustness defect**; no confirmed P0/P1/P2 normal-wire defect or request-field/order/routing defect. The older numbered-56 optional-payload identity-loss finding is fixed. Numberless successful credit registration remains a reproduced interoperability question, explicitly separated from confirmed findings.

## Scope and method

Reviewed all fields, constructors, validation, serialization, response parsing, semantics and tests in `crates/szamlazz-agent/src/ops/{storno,credit_entry,proforma,envelope}.rs`. Followed the shared verdict/XML machinery in `xml.rs`, headers/multipart in `wire.rs`, numeric conversion in `number.rs`, `Pdf`/payment-method/template types, error classifications and the create/preview dispatch using this envelope. Paths beginning `src/` or `tests/` below are relative to `crates/szamlazz-agent/`; line references identify this HEAD.

Read `docs/szamlazz-hu-behaviour.md` and `fixtures/SOURCES.md`. Read the older untracked `2026-09-10-agent-api-current-mutations.md` only after independently inspecting current code and fresh operation documentation; its claims were leads for new checks, not evidence of current defects. This is a direct protocol audit of the pinned revision, not an empty HEAD-to-HEAD diff review. No subagent tool was available; review and verification were performed directly.

Only this new report was authored in the repository. Scratch programs were authored with `apply_patch` under `/tmp/opencode/f83e5fd-mutations-review/`. No source, test, fixture, configuration or lockfile edits; no live account calls. All external requests were unauthenticated documentation/download GETs. PHP source and downloaded examples were read, never executed.

Final verification reconfirmed HEAD and an empty `git diff HEAD -- crates/szamlazz-agent Cargo.lock fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md`. Unrelated Restate/Adatkapcsolat/context changes and other new reports appeared concurrently; none were authored or modified by this review.

## 1. Confirmed actionable defect

### FM1 — Optional error-message structure still erases independently readable verdict/code and numbered-56 evidence

**Severity:** P3, malformed-response robustness. **Confidence:** high in reproduced implementation behavior. **Vendor occurrence:** unobserved. This is not a claim that an XSD-valid vendor response fails or that the client automatically duplicates writes.

**Exact locations:**

- `src/ops/envelope.rs:292–294`: `parse_envelope` validates the complete XML and namespace, then deserializes the entire `xml::Verdict` with `?`, before the separately recoverable payload/identity path.
- `src/xml.rs:347–355`: that verdict couples `sikeres` and `hibakod` to the optional `hibauzenet: Option<String>`.
- `src/ops/envelope.rs:188–196`: failure here exits before number extraction; the new fallback correctly allows only plain notification text, so it cannot rescue a structurally bad diagnostic within XML.
- Other mutation entry points share the coupling: `src/ops/credit_entry.rs:250–251`, `src/ops/proforma.rs:82–87`, through `src/xml.rs:389–393`.

**Authoritative evidence and its limits:**

1. [Fresh shared response XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) declares the diagnostic optional:

   ```xml
   <element name="sikeres" type="boolean" maxOccurs="1" minOccurs="1"></element>
   <element name="hibakod" type="string" maxOccurs="1" minOccurs="0"></element>
   <element name="hibauzenet" type="string" maxOccurs="1" minOccurs="0"></element>
   <element name="szamlaszam" type="string" maxOccurs="1" minOccurs="0"></element>
   ```

2. [Official HU response handling](https://docs.szamlazz.hu/hu/php/valasz-feldolgozas): **“Előfordulhat, hogy egy számla kiállítása sikeres, azonban a számlaértesítőt valamilyen ok miatt nem sikerül kézbesítenie a Számlázz.hu rendszerének.”** An invoice can be successfully issued despite notification delivery failure. [EN](https://docs.szamlazz.hu/php/valasz-feldolgozas) makes the same distinction.
3. [Fresh first-party PHP 2.12.4 download](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), `szamlaagent/src/szamlaagent/Response/InvoiceResponse.php:17` defines notification failure as 56. Lines 319–322 say **“Ha a számlaértesítő kézbesítése sikertelen volt, de a válasz tartalmaz számlaszámot, akkor a számla kiállítása sikeres.”** Translation: if notification delivery failed but the response contains an invoice number, invoice issuance succeeded.
4. Current `src/ops/envelope.rs:173–177` expressly promises lenient optional metadata on numbered 56, so malformed optional data must not hide issuance. The current fix extends that principle to structural totals/PDF failures, but not the diagnostic string.

**Minimal reproduction:** HTTP 200, no headers, passed to `StornoInvoice::new("I-1").parse(&raw)`:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres>
  <hibakod>56</hibakod>
  <hibauzenet><bad/></hibauzenet>
  <szamlaszam>I-2</szamlaszam>
</xmlszamlavalasz>
```

**Actual:** `Err(Parse(Xml(XmlError(MixedContent("bad")))))`, classified `OutcomeClass::Unknown`, with no `I-2` in the returned error. Two scalar `hibauzenet` elements instead produce `duplicate field 'hibauzenet'` (backticks in the actual diagnostic). Adding both `szlahu_error_code: 56` and `szlahu_szamlaszam: I-2` still fails.

**Controls:**

- Omit the diagnostic and instead place `<bad/>` inside `szamlabrutto`: HEAD returns numbered `CreatedInvoice { notification_delivery_failed: true, gross_total: None, … }`, correctly closing the old CM1.
- Duplicate `pdf` likewise preserves body-only identity now.
- A valid non-56 refusal plus bad optional totals/PDF is still a refusal, even with provisional header 56; the earlier refusal-erasure fix remains intact.
- Body-only credit refusal `sikeres=false`, `hibakod=463`, nested `hibauzenet` also returns a generic parse error rather than the independently readable `PaymentOnReversedInvoice` answer. This is the same coupling, not a second finding.

**Why actionable despite malformed input:** the XML is well-formed and correctly namespaced; the unique verdict/code/number are readable. Only a human-readable auxiliary diagnostic violates its scalar schema. Losing that diagnostic need not erase these facts. This is a narrow inconsistency in the existing evidence-retention design, not a recommendation to accept malformed verdicts, numbers, complete-document syntax or namespace identity.

**Impact:** unnecessary reconciliation and loss of the returned document handle or known refusal code. A caller incorrectly treating every error as permission to retry could repeat a write, but the crate itself classifies these parse failures conservatively and does not resend in these parsers.

**Recommended correction:** establish unique scalar verdict/code independently of optional diagnostic decoding, retain usable identity independently of remaining metadata, and preserve a missing/unusable diagnostic separately. Keep missing/duplicate/nested verdict/code/identity conservative, complete XML validation intact, and non-56 refusal precedence. Do not restore the old broad “header 56 rescues any malformed XML” fallback.

## 2. Reproduced boundaries, vendor ambiguities and intentional policies

### FA1 — Numberless successful credit registration is schema-valid but rejected

**Not counted as a confirmed normal-wire defect.** Potential P2 interoperability impact if emitted; P3 contract clarification in the absence of success-path evidence.

`src/ops/credit_entry.rs:253–256` requires a number after accepting the success verdict; `InvoiceBalance.invoice_number` is mandatory at lines 195–197. HTTP 200, no number header, and:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
</xmlszamlavalasz>
```

returns `Err(Parse(Missing("szamlaszam")))`. Adding a number header succeeds. Full libxml2 validation against the freshly downloaded shared XSD accepts this body; the operation-specific inline XSD also declares only `sikeres` mandatory.

[EN credit response](https://docs.szamlazz.hu/agent/credit_entry/response): **“Elements marked `minOccurs="0"` may not always be included”**, and **“Additional data may also arrive in the HTTP response header.”** [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/response): **“A `minOccurs="0"` jelölésűek nem mindig jelennek meg.”** Neither says at least one channel must echo a number on every successful registration.

Counter-evidence/uncertainty: this is one schema for both success and failure, so optionality may accommodate failures. Every freshly fetched successful example and recorded credit observation includes a number. No source found explicitly demonstrates a successful version-2 credit registration without one. The constructor's request already names the target, but substituting it silently would manufacture *reported* identity. The older report's ambiguity is therefore still open, not settled merely by accepting or rejecting its interpretation.

**Resolution:** clarify the success-path echo guarantee, or expose successful acknowledgement with optional *reported* identity and keep requested identity separate. Do not reuse the new-document number requirement uncritically for a mutation of an already selected invoice. An additive repeat can duplicate credit entries; a replacing repeat can overwrite intervening entries. This failure is not permission to resend.

### FP1 — Plain-success optional metadata remains strict by deliberate policy

`src/ops/envelope.rs:208–212,224–249,259–268,308–310` only tolerates malformed optional values for numbered notification failure. Ordinary `sikeres=true` plus a valid number and `szamlabrutto=bad`, nested totals, duplicate PDF or invalid base64 returns `ResponseError::Parse`. The amount/PDF tests at `envelope.rs:535–549,634–641` deliberately assert that policy. Credit totals behave similarly (`credit_entry.rs:257–274`). These were independently reproduced.

The schemas allow omission but require the correct type when an element is present. Strict rejection is consequently not normal-wire noncompliance. A richer result could preserve known write evidence while reporting unusable metadata; no such redesign is counted as a defect here. FM1 is narrower: numbered-56 leniency is already the explicit contract.

Related coupling: credit registration deserializes the whole shared `Body`, including `pdf` (`credit_entry.rs:251`, `envelope.rs:116–117`), though it never returns/decodes PDF and the credit-specific inline reply schema has no PDF. A nested or repeated PDF therefore fails a successful credit reply. A scalar `pdf` is ignored. This is a robustness boundary for malformed/unexpected extension content, not a documented credit-response omission.

### FP2 — Header/body/status conflicts have a local precedence policy

Current order (`wire.rs:291–310`): nonblank decoded `szlahu_down`; otherwise nonblank raw error-code header; otherwise known non-2xx HTTP status; otherwise structured body. Issuing operations provisionally admit 56, then give a readable non-56 body refusal precedence (`envelope.rs:180–203`). Credit/deletion do not promote 56.

- A numbered header 56 and plain notification text may succeed at HTTP 500.
- A body-only numbered 56 at HTTP 500 is a status error before XML is read.
- Success-number headers alone do not override status or a non-56 header error.
- A wrong-root, truncated, malformed XML or invalid-verdict body under header 56 is now refused; plain text/empty-body fallback alone remains (`envelope.rs:190–196,254–256`).

The official pages do not specify precedence for contradictory channels. Their generic **“If error codes are present in the header, invoice number and amounts are omitted”** coexists with the first-party numbered-56 exception. No conflicting live exchange was available. Do not label this explicit precedence policy a vendor violation or change it on the basis of synthetic contradictions alone.

### Other source qualifications

- **Storno external id:** both fresh EN/HU request pages describe referencing the original through `szamlaKulsoAzon`, but require `szamlaszam`. The example describes later querying “the invoice,” without resolving which one. Recorded B6/XPRB/P48 behavior establishes attachment to the created storno when the number is supplied. Current `storno.rs:86–98,183` correctly preserves that bounded observation. No external-id-only selector is established.
- **Paid proforma:** deletion is not restricted to unpaid proformas in the current API or implementation. D3 observed deletion of a fully paid proforma. The caller's retention policy is accurately documented at `proforma.rs:4–6,35–39`.
- **All matching proformas:** HU deletion XML says **“Ha azonos rendelésszámmal több díjbekérő is van a számlázási fiókban, akkor a törlés az összes díjbekérőre vonatkozik.”** All matches are deleted. The EN page omits this sentence; [PHP deletion](https://docs.szamlazz.hu/php/dijbekero-torles) corroborates multiple deletion and claims rollback on member failure. Current enum docs are correct. Rollback was not tested and says nothing conclusive about a lost reply.
- **Empty replacement:** request XSD permits zero `kifizetes`; validated transport intentionally refuses empty replacement (`credit_entry.rs:217–223`). D7 proves replacement/addition with entries, not clearing with zero entries. `docs/szamlazz-hu-behaviour.md:211–215` explicitly records that uncertainty. Empty additive is accepted locally, not newly proven harmless on the server.
- **Finite exact money/civil dates:** Decimal and civil Date are narrower than the whole XSD double/date domain. No silent precision loss, NaN/infinity amount support, timezone-bearing request date, arbitrary storno `tipus`, or response-version-1 API is promised. These are intentional domains, not unsupported business cases established by this audit.
- **Header encoding:** number/error text are explicitly URL-encoded; totals/code explicitly are not. Payment-method/customer-URL table rows do not precisely define encoding. One-pass textual decoding is reasonable and tested; double decoding is not justified. No XML payment-method element exists despite the broad “same data … XML body” sentence.
- **Examples are not captures:** current create/storno/credit success examples contain raw `&` in a URL, and create/storno PDF text contains `....`. These are invalid complete XML/base64 examples. The corpus preserves originals, while its tests explicitly repair URL syntax/substitute synthetic PDF bytes. Neither the positive storno example total nor a repaired PDF proves a live reversal shape.

## 3. Exhaustive request field/order/routing coverage

Notation: `?` = XSD `minOccurs=0`; all non-repeated elements have `maxOccurs=1`. No missing documented field or incorrect emitted sequence was found.

Shared transport: UTF-8 XML 1.0 and correct default namespace (`xml.rs:19–40`), escaped text (`442–460`), ordered credential alternatives (`484–493`), `POST` target `https://www.szamlazz.hu/szamla/` and multipart file part (`wire.rs:14,66–99,395–408`). `to_wire` validates before constructing multipart. A schema-location hint is not a missing business field. Official authentication explicitly allows either agent key or username/password; examples carrying both do not require both. Credential case/content is preserved.

### Számla sztornó / Reversing invoice

| Ordered wire surface | Mapping and review result |
|---|---|
| Action `action-szamla_agent_st`; root `xmlszamlast`; namespace `http://www.szamlazz.hu/xmlszamlast` | Exact, `storno.rs:162–170`. |
| Root blocks `beallitasok`, `fejlec`, `elado?`, `vevo?` | Exact, `171–206`; empty optional seller/buyer blocks are XSD-valid. Omission-versus-empty email behavior is not inferred. |
| Settings `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?` | Shared credentials at `172`. |
| `eszamla`, `szamlaLetoltes` | Explicit bools at `173–174`, defaults false/false at `144–145`. |
| `szamlaLetoltesPld?` | `Option<u8>`, `175–177`; narrower than XSD int, but the current page says **“our system no longer processes it.”** No useful lost capability established. |
| `aggregator?`, `guardian?`, `valaszVerzio?`, `szamlaKulsoAzon?` | String, bool, shared constant 2, string, in order at `178–183`. |
| Header `szamlaszam`, `keltDatum?`, `teljesitesDatum?`, `megjegyzes?`, `tipus?`, `szamlaSablon?` | Number, dates, free text, fixed `SS`, open template token, `185–194`. All six documented template tokens remain representable. |
| Seller `emailReplyto?`, `emailTargy?`, `emailSzoveg?` | Independent optional `SellerEmail` fields, `195–201`. |
| Buyer `email?`, `adoszam?`, `adoszamEU?` | All in order, `202–206`. Schema annotation permits supplying a tax number missing from the original; not arbitrary editing of its existing tax number. |

### Befizetés rögzítése / Registering credit entry

| Ordered wire surface | Mapping and review result |
|---|---|
| Action `action-szamla_agent_kifiz`; root `xmlszamlakifiz`; matching namespace | Exact, `credit_entry.rs:213–228`. |
| Root `beallitasok`, followed by `kifizetes` × 0–5 | Correct sequence `230–245`; maximum enforced by private collection, push, vector conversion and deserialization (`55,69–74,116–132`). |
| Credentials; `szamlaszam`, `adoszam?`, `additiv`, `aggregator?`, `valaszVerzio?` | Correct order/types at `231–236`; version 2. `adoszam` is issuer tax number. |
| Entry `datum`, `jogcim`, `osszeg`, `leiras?` | Date, open PaymentMethod token, finite Decimal, optional description at `239–244`; no invented sign restriction or rounding. |
| Replace/additive semantics | False replaces, true retains previous entries; fresh EN says **“If true, former credit entries are retained; otherwise they are replaced.”** Constructor defaults to replacement (`180–187`); empty replacement policy above. |

HU's issuer comment is **“ha megadod a kiállító adószámát, a rendszer a bejövő kifizetést a megfelelő számlához rendeli”**. The old downloadable example additionally mentions incoming-invoice credit registration (“B-s jovairt”). The EN example's “incoming receipt” does not define another receipt operation or justify renaming the issuer field.

### Díjbekérő törlése / Deleting a pro forma invoice

| Ordered wire surface | Mapping and review result |
|---|---|
| Action `action-szamla_agent_dijbekero_torlese`; root `xmlszamladbkdel`; matching namespace | Exact, `proforma.rs:60–68`. |
| Required `beallitasok`, `fejlec` | Correct order at `69–77`. |
| Settings: credential alternatives only | No nonexistent response-version or extra settings emitted. |
| Header `szamlaszam?`, `rendelesszam?` | Selector emits exactly one at `72–77`; follows current separate examples. Old ZIP example carries both and XSD allows both/neither, but priority/combined semantics are undocumented. Enum restriction is a deliberate unambiguous subset. |
| Number versus order scope | One proforma versus all matching proformas, `18–29,35–41`; no count/list or paid-state guard falsely promised. |

**Executable XSD corroboration:** 14 freshly generated variants validated successfully against unmodified newly fetched XSDs: minimal/full storno, credit with 0 additive/1 replacing/5 replacing entries, deletion by number/order, each with agent-key and username/password credentials. Full variants exercised every optional request field, escaping, negative fractional amount, open payment-method token and deprecated copy count 255. This proves serialization/schema compatibility, not account-level permission or server acceptance.

## 4. Exhaustive response coverage

| Surface | Current implementation / conclusion |
|---|---|
| Complete document/root/namespace | `xml.rs:63–168,170–194`; full structure plus lexical validation. Root must be `xmlszamlavalasz` for storno/credit and `xmlszamladbkdelvalasz` for deletion, in its matching namespace. Foreign subtrees cannot supply recognized protocol fields (`196–256`). Namespace aliases work; undeclared prefixes fail. |
| `sikeres` | Required `Verdict`, `xml.rs:347–355`; true/false/1/0 supported. Empty is legacy false (`630–634`), not success; invalid/missing is refused. No full response XSD/order validation is claimed; reordered valid fields and unknown extensions are tolerated. |
| `hibakod?`, `hibauzenet?` | False verdict supplies typed/open code and diagnostic (`357–378`); blank/absent code remains `Absent`, not a fabricated refusal. Code is only used as a body error when `sikeres=false`; contradictory successful-body error fields have no specified vendor semantics. FM1 covers diagnostic coupling. |
| Number | Body first, decoded header fallback, trimmed/nonblank (`envelope.rs:120–133,329–332`). Unique body-only identity recovered under numbered 56 after optional payload structure failure (`298–315`). Number required for issued storno (`278–282`); FA1 for credit. No comparison to storno request number: legitimate same-number no-op remains observable. |
| `szamlanetto?`, `szamlabrutto?`, `kintlevoseg?` | Optional Decimal, corresponding net/gross/outstanding headers as fallback (`158–168,229–246`; `credit_entry.rs:257–274`). Blank body is absent. Valid body wins even over invalid header; bad nonblank body does not fall back to good header. Sparse totals permitted; no recomputation or sign gate. |
| Numeric grammar | XML finite dot/exponent forms, exact representability (`number.rs:63–141`). Headers accept ungrouped comma/dot/exponent and HTTP SP/HTAB padding (`envelope.rs:347–374`); raw access preserves `+`, never percent-decodes money. Out-of-domain/invalid values are refused, except numbered-56 leniency. |
| `vevoifiokurl?` | Body then once-decoded header (`135–143`). Body receives XML entity decoding, not percent decoding. Scalar edge trimming remains a local reading policy. |
| `pdf?` | Optional base64-to-Pdf (`145–148,249`; `types.rs:104–116`); wrapped whitespace supported, no claim of PDF rendering/signature validation. Missing on numbered issuance remains `None`, even if requested; malformed under ordinary success is strict. Credit has no PDF output. |
| `szlahu_fizetesmod` | Open PaymentMethod on both CreatedInvoice and InvoiceBalance (`envelope.rs:49–53,248,321–327`; `credit_entry.rs:275`). No fabricated XML payment-method field. |
| `szlahu_id` | CreatedInvoice's optional nonnegative i64, lenient on bad auxiliary id (`334–345`); reported document id, not seller/account identity. Credit balance does not expose this undocumented auxiliary header. |
| Notification 56 | Numbered exception and optional metadata leniency (`170–251`); body/header supported; numberless 56 is an uncertain API error, never a preview. Invalid verdict/XML cannot be promoted. FM1 remains. |
| Create/preview consumer | `invoice.rs:923–935`: numbered reply wins even if preview requested; unnumbered reply requires preview request and PDF. Storno has no preview success. No missing/new false-issuance branch found. |
| Deletion payload | `proforma.rs:82–87`: all three documented fields covered by verdict; success `()`, false/335 `ProformaNotFound`, not successful repeat. No missing count/number list exists in the schema. |
| Critical deletion text/HTML | Documented alternative is preserved as bounded unexpected-body error (or status error); never promoted to successful deletion. |

Error classification checked against the fresh general catalogue, deletion 335 example and bounded live observations: credentials 3/135/136/164 are typed; 1/55/56 and unknown/absent codes remain uncertain; 71/152 remain duplicate-order classification; 14/221/352/463 retain their observed meanings. Unknown code tokens are preserved. A type name or `is_retryable` does not authorize resending a write; these operation parsers have no automatic send loop. The official same-request limit is five, not unlimited retry.

## 5. Live-supported differences retained

The behavior note is evidence about one TEST account on its recorded September 3/6/7 dates, not every account. Original exchange logs are not available in this repository. No claim below was freshly re-probed live.

- B4: repeat storno echoes existing storno number/id/negative totals. B5: proforma/delivery-note storno echoes the requested number unchanged with positive totals. These stay distinguishable in the returned document (`storno.rs:24–50`).
- B1/B8: reversal marker is observed through query; storno negates quantity/totals and removes original credit-entry history. `CreatedInvoice::reverses` is only a reply heuristic (`envelope.rs:61–87`): a changed number with absent/positive gross is inconclusive, not proof of no reversal. Zero comparison is intentional policy, not a zero-original live result.
- B6/XPRB/P48: storno external id attaches to the created SS; repeat storno drops a newly supplied external id. Keep number targeting.
- B3/P48: non-today storno `keltDatum` rejected with 352; omitted fulfillment defaults to original, while an explicit wrong fulfillment can be accepted silently. Low-level caller-selected dates remain supported and appropriately qualified.
- P73: request `eszamla` determines the storno's appearance even when mismatched with original; caller-selected flag remains supported. Worker-derived matching policy is outside this crate review.
- D1/D3: deletion succeeds without headers and can delete paid proformas; repeated/consumed/nonexistent deletion yields 335. No paid-only or unpaid-only wire assumption added.
- D7/D8: replace/additive semantics, five-entry request, body-only credit rejection 463; preserve body error parsing. P60: comma monetary headers are real observed forms, not malformed XML numbers.
- Code 56 was not successfully triggered by the recorded account probes; its special rule is first-party PHP evidence. Negative-original/positive-storno and zero-original cases remain unverified.

## 6. Older findings checked against HEAD

| Older lead | Independently checked disposition |
|---|---|
| CM1 in `current-mutations`: nested totals/duplicate PDF erase unique body-only numbered-56 identity | **Fixed**, `envelope.rs:298–315`. Existing regression `tests/response_headers.rs:474–501` and scratch controls pass. FM1 is a distinct diagnostic-field boundary. |
| Older M1: optional metadata failure hides a valid non-56 refusal under header 56 | **Still fixed**, verdict considered before payload (`197–212`); scratch and `response_headers.rs:432–450` retain code 3/message. |
| CH2: malformed XML/invalid verdict under numbered header 56 can be promoted | **Fixed/tightened**, `190–196,254–256`; malformed XML and bogus boolean refused; plain notification text still supported. |
| Credit echo treated as required | Implementation unchanged; reproduced schema-valid rejection and retained success-path ambiguity as FA1. |
| Plain-success metadata strictness | Unchanged intentional policy, FP1, independently reproduced. |
| Comma amount fallback and payment-method loss | Remain fixed, `envelope.rs:321–327,364–374`; existing cross-operation header tests pass. |
| False storno heuristic implies no reversal | Guidance remains corrected (`envelope.rs:71–82`, `storno.rs:24–28`). |
| Unpaid-only proforma promise / single-match order deletion | Current docs correctly state paid-policy responsibility and all-match scope. |
| Missing current structured examples in historical corpus | Dated examples/provenance and tests exist; fresh pages agree on fields and invalid sample URL/PDF syntax. |
| No working deletion XSD download found | **Research limitation resolved for request:** the freshly downloaded legacy example points to working `/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd`. Reply download remains unavailable at attempted paths. |

Older transport/other-operation findings were not imported into this count. This review does not establish interrupted-body client handling, endpoint behavior, worker recovery or other crates' correctness.

## 7. Fresh primary-source register

All listed current pages were fetched in this review. Site footer was `v202608271632`; that is not the last-modified date of every statement. EN/HU categories were followed to each request/response/XML page, rather than relying on search snippets or repository fixtures.

| Group | Fresh URLs |
|---|---|
| Storno EN | [Category](https://docs.szamlazz.hu/agent/category/reversing-invoice), [request](https://docs.szamlazz.hu/agent/reversing_invoice/request), [response](https://docs.szamlazz.hu/agent/reversing_invoice/response), [XML/examples/inline XSD](https://docs.szamlazz.hu/agent/reversing_invoice/xml) |
| Storno HU | [Category](https://docs.szamlazz.hu/hu/agent/category/reversing-invoice), [request](https://docs.szamlazz.hu/hu/agent/reversing_invoice/request), [response](https://docs.szamlazz.hu/hu/agent/reversing_invoice/response), [XML/examples/inline XSD](https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml) |
| Credit EN | [Category](https://docs.szamlazz.hu/agent/category/registering-credit-entry), [request](https://docs.szamlazz.hu/agent/credit_entry/request), [response](https://docs.szamlazz.hu/agent/credit_entry/response), [XML/examples/inline XSD](https://docs.szamlazz.hu/agent/credit_entry/xml), [IPN](https://docs.szamlazz.hu/agent/credit_entry/other) |
| Credit HU | [Category](https://docs.szamlazz.hu/hu/agent/category/registering-credit-entry), [request](https://docs.szamlazz.hu/hu/agent/credit_entry/request), [response](https://docs.szamlazz.hu/hu/agent/credit_entry/response), [XML/examples/inline XSD](https://docs.szamlazz.hu/hu/agent/credit_entry/xml), [IPN](https://docs.szamlazz.hu/hu/agent/credit_entry/other) |
| Deletion EN | [Category](https://docs.szamlazz.hu/agent/category/deleting-a-pro-forma-invoice), [request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), [response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response), [XML/examples/inline XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml) |
| Deletion HU | [Category](https://docs.szamlazz.hu/hu/agent/category/deleting-a-pro-forma-invoice), [request](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/request), [response](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/response), [XML/examples/inline XSD](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml) |
| Shared | [Root](https://docs.szamlazz.hu/), [Agent index](https://docs.szamlazz.hu/agent/), [sending/routing](https://docs.szamlazz.hu/agent/basics/sending-requests), [EN authentication](https://docs.szamlazz.hu/agent/basics/authentication), [HU authentication](https://docs.szamlazz.hu/hu/agent/basics/authentication), [EN errors](https://docs.szamlazz.hu/agent/basics/error-handling), [HU errors](https://docs.szamlazz.hu/hu/agent/basics/error-handling), [EN create response](https://docs.szamlazz.hu/agent/generating_invoice/response), [HU create response](https://docs.szamlazz.hu/hu/agent/generating_invoice/response) |
| First-party corroboration | [PHP index/download link](https://docs.szamlazz.hu/php/), [EN response handling](https://docs.szamlazz.hu/php/valasz-feldolgozas), [HU response handling](https://docs.szamlazz.hu/hu/php/valasz-feldolgozas), [deletion](https://docs.szamlazz.hu/php/dijbekero-torles) |

### Downloaded XSDs and examples

| Fresh download | SHA-256 |
|---|---|
| [Storno request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd) | `6f9d5beb6efd205f0509e7413e15fb96660ee89c9f169d27c4972d3b24127a3a` |
| [Credit request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd) | `9637a242df2f55f87ecfff1b07dd74ef0b09bdaff47e39bb4d9890d4748a73de` |
| [Deletion request XSD](https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd) | `076b4d98c3cf599a5b5ab30e5e9ff3e522d9c0642d806fb502e8a560ace08641` |
| [Shared response XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |
| [PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| [Legacy Agent ZIP](https://docs.szamlazz.hu/assets/files/SzamlaAgent-eab03f119308ff908bbf46c7cde5d1bb.zip) | `b70bc43e7dfd7960b6234e8df6916d7a3aa5e10b841d20ae7d26b84e992e420a` |

The current Agent index explicitly labels the latter archive last updated **2019-03-13**. Read its `storno/xmlszamlast.{xml,xsd}`, `kifiz/xmlszamlakifiz.{xml,xsd}`, `dijbekerodel/xmlszamladbkdel.{xml,xsd}`. The three packaged request XSD hashes equal the corresponding fresh downloads above; historical example values/version 1 are not current server-behavior proof. The old deletion example supplied the successful current request-XSD URL.

404s, recorded rather than silently replaced:

- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd`
- `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd`
- Both corresponding `/szamla/docs/xsds/szamladbkdel/` variants.
- `https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdelvalasz.xsd`
- Guessed adjacent `…/agentst/xmlszamlast.xml` and `…/agentkifiz/xmlszamlakifiz.xml`; examples instead read from fresh pages and the linked archive.

Deletion **response** schema was therefore reviewed from current EN/HU inline blocks, not a fabricated download. Current inline request schemas agree with fresh downloadable request fields/order/cardinality; HU credit's named root complex type versus EN's anonymous one is structurally equivalent. No schema was merged or patched for validation.

## 8. Executed verification and limitations

### Existing targeted tests

```sh
cargo test --offline --locked -p szamlazz-agent --lib --test response_headers --test response_completion --test response_namespaces --test numeric_fidelity --test upstream --test error_classification
```

**227 passed, zero failed/ignored:** 185 unit, 12 headers, 4 completion, 6 namespaces, 6 numeric fidelity, 11 upstream, 3 classification. Relevant upstream corpus directories are present; the test result is not based on absent-corpus skipping. No ignored live test was selected. Passing other operations' unit tests does not enlarge the protocol-review scope.

### Independent public-API probes

```sh
cargo build --offline --locked -p szamlazz-agent
rustc --edition=2024 --test /tmp/opencode/f83e5fd-mutations-review/probe.rs -L dependency=/home/laborant/szamlazz-rs/target/debug/deps --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -o /tmp/opencode/f83e5fd-mutations-review/probe
/tmp/opencode/f83e5fd-mutations-review/probe --nocapture
```

**6 assertion groups passed** in the final run: malformed metadata versus evidence; earlier refusal/XML-fallback closure; malformed diagnostic hiding code 463; numberless credit/header rescue and unused PDF coupling; malformed/duplicate/foreign identity; deletion booleans/335/trailing-document refusal. Assertions intentionally confirm current problematic behavior as well as controls. A preceding five-group run passed before adding the code-463 control. Scratch links the actual workspace-built library, avoiding an independently resolved Cargo dependency graph.

### Full XSD validation and fresh archive inspection

```sh
rustc --edition=2024 /tmp/opencode/f83e5fd-mutations-review/emit.rs -L dependency=/home/laborant/szamlazz-rs/target/debug/deps --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -o /tmp/opencode/f83e5fd-mutations-review/emit
python3 /tmp/opencode/f83e5fd-mutations-review/validate.py
python3 /tmp/opencode/f83e5fd-mutations-review/downloads.py
```

14 request variants passed full schema validation; numberless success passed shared-response schema validation. The installed `libxml2.so.2` was called through Python ctypes; `lxml`, `xmllint` and `xmlstarlet` were absent, and no dependencies were installed. Schemas were fetched into memory from official HTTPS URLs immediately before validation. `emit.rs` also checks actual multipart content type and operation file-field naming. `downloads.py` reads only selected XML/XSD/PHP archive entries and prints hashes; no vendor executable is run.

### Limitations

- No live acceptance/emission/delivery claims: numberless credit, code-56 wire shape, malformed metadata, conflicting channels, empty additive/replace effects, email override inheritance, zero/negative-original reversal and rollback remain unprobed.
- Schema-valid request output is not proof of vendor business-rule acceptance. The 14 variants exercise every field but not every Cartesian combination or account setting.
- Full shared response schema validation was performed for the numberless success; malformed robustness probes intentionally violate field shape. Deletion reply schema comparison was manual from fresh inline source.
- No wasm, live transport interruption, performance/fuzz campaign, worker recovery or other-crate audit. Full source and targeted checks are substantial coverage, not a proof against all malformed XML.
- Older reports and synthetic controls were never substituted for fresh primary sources or live captures. No production fix or proposed-fix tests are claimed.

## Conclusion

The three mutation writers are complete and correctly routed/ordered against current EN/HU documentation and freshly downloaded XSDs. Preserve the live-supported storno and deletion differences. HEAD fixes the older optional-payload identity loss and broad malformed-XML header-56 fallback. The remaining bounded actionable improvement is **FM1: keep an unusable optional diagnostic from erasing readable verdict/code/number evidence**. Resolve the numberless credit-success contract separately from that malformed-response hardening.
