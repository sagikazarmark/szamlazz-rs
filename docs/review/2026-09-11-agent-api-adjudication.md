# Számla Agent review adjudication — 2026-09-11

**Reviewed HEAD:** `2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81`.

## Disposition

**Accept four findings: two P2 implementation defects and two P3 corrections. Downgrade the numberless credit acknowledgement to an unresolved contract question and receipt NAV setup to a documentation opportunity. No P0/P1 finding is established.**

| Report ID | Adjudicated disposition | Evidence boundary |
|---|---|---|
| Transport **T-01** | **Accept P2:** the documented key/key legacy authentication form exposes the agent key through `Debug`. | Independently reproduced through credentials, builder and client. Operationally applicable credential form; no actual secret/log incident observed. |
| Transport **T-02** / mutations ambiguity 4 | **Accept P2, narrowly:** numbered-56 fallback bypasses the crate's duplicate/nested body-identity checks. | Independently reproduced with completed synthetic replies. A response-integrity/contract defect, not established vendor emission or wrong-document adoption in production. |
| Mutations **M-01** | **Downgrade:** high-priority vendor clarification and potential compatibility improvement; no confirmed operational P2. | Rejection of schema-valid numberless success is reproducible. Success-specific permission to omit the number remains unresolved. |
| Taxpayer **TQ-01** / queries §“XML structure and namespaces” | **Accept P3:** shared namespace-conformance gap, not taxpayer data injection. | Independently reproduced under NAV 2.0 and 3.0; W3C requirements are explicit. No operational occurrence or changed business fact established. |
| Receipts **R-NAV-1** | **Downgrade to P3 documentation opportunity**, outside the confirmed-defect count. | Current account-setup guidance is real and worth linking. Missing onboarding prose does not demonstrate a receipt protocol defect or failed reporting. |
| Transport **T-03** | **Accept P3:** qualify unsupported protocol guarantees in error rustdoc. | Direct code/document comparison; no runtime misclassification demonstrated. |

Here **P2** means a concrete, bounded implementation correction with material impact when its conditions hold. **P3** means a low-impact conformance or documentation correction. Priority is separate from occurrence: synthetic evidence can prove a local contract failure without proving the service emits that input. Conversely, XSD acceptance alone does not prove every accepted instance is a supported operational response.

## Scope and method

Read all six current reports: [invoices](2026-09-11-agent-api-invoices.md), [mutations](2026-09-11-agent-api-mutations.md), [queries](2026-09-11-agent-api-queries.md), [receipts](2026-09-11-agent-api-receipts.md), [taxpayer](2026-09-11-agent-api-taxpayer.md), and [transport](2026-09-11-agent-api-transport.md). Their conclusions were claims to adjudicate, not independent proof.

Independently traced the current credential/client diagnostics, issuance envelope and its callers, credit parser, shared XML/namespace reader, taxpayer paths, receipt public contract, query entry points, numeric helpers, preview/simple-items writer, README and recovery guidance. This is a focused adjudication of those reports, not a second exhaustive enumeration of every schema field. Repository line references below are at the pinned HEAD and relative to `crates/szamlazz-agent/` unless otherwise stated.

HEAD was verified before and after examination. The reviewed crate, workspace manifests/lockfile and behavior note matched HEAD. Unrelated work was already present elsewhere in the worktree. No delegation, authenticated requests, live vendor calls, source/test/fixture edits or full-suite rerun were performed. This report is the only repository file written by this adjudication; the new offline probe is under `/tmp/opencode/`.

### Primary sources independently fetched for this adjudication

Fetched on **2026-09-11**. The Agent documentation displayed `v202608271632`; that is a site build, not an observation date for examples or server behavior.

| Ref | Source | What it establishes |
|---|---|---|
| A | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Explicit support for the same agent key in `felhasznalo` and `jelszo`; key secrecy and account-wide permissions. |
| C | Credit response [EN](https://docs.szamlazz.hu/agent/credit_entry/response), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/response) | Optional additional headers; shared success/error XSD with optional number; numbered XML success examples and numberless error examples. |
| E | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | Meanings of 3/135/136/164 and other codes; no universal HTTP-status or exact processing-order guarantee. |
| P | [First-party PHP response handling](https://docs.szamlazz.hu/php/valasz-feldolgozas) | Issuance may succeed despite notification failure. |
| X | [Downloaded issuance-envelope XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | `szamlaszam` is a singleton string; verdict and optional payload share one type. |
| W | [Namespaces in XML 1.0, Third Edition](https://www.w3.org/TR/2009/REC-xml-names-20091208/) | §3 forbids `xmlns` as an element prefix; §7 requires colon-free PI targets; §8 requires reporting namespace-well-formedness violations. |
| R1 | [Agent receipt-reporting page](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting) | Still says automation is being developed and no action is needed yet. |
| R2 | [Hungarian receipt-reporting knowledge base](https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg) | States automatic forwarding from September 10, retrospectively for receipts after September 1, conditional on NAV connection and technical-user permission; expressly discusses Számla Agent receipt issuance. |
| R3 | [NAV connection guide, step 13](https://www.szamlazz.hu/nav-online-szamlazas-regisztracios-segedlet/#lepesek) | Lists “Hozzáférés a nyugtaadat-szolgáltatási interfészhez” and distinguishes computer-generated receipts from the ePénztárgép application. |

Also independently read the first-party `InvoiceResponse.php:305–323,427–431` in the transport review's previously downloaded [PHP 2.12.4 archive](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip). Its number-plus-56 exception is real. That archive was **not downloaded again by this adjudication** and was not executed; fresh P corroborates the notification distinction, not malformed-body precedence.

## Accepted findings

### T-01 — P2: a supported credential alias defeats secret redaction

**Code:** `src/credentials.rs:88–96`; propagation at `src/client.rs:149–158,339–345`.

`Credentials::user_password(key, key)` stores the agent key in both fields. Its `Debug` redacts only `password` and formats `username` verbatim. Client and builder diagnostics recursively format those credentials. This is not merely a caller putting an arbitrary secret into an unrelated string: source A explicitly documents this authentication form, and the crate's credential writer emits it unchanged (`src/xml.rs:603–613`).

The independent offline probe confirmed the placeholder key appears in all three diagnostic strings; `Credentials::agent_key` remains redacted. Building the client did not send a request. Existing `debug_is_redacted` tests intentionally retain an ordinary username and therefore miss the alias (`src/credentials.rs:105–114`).

**Impact and occurrence:** a caller using this supported legacy form and logging a diagnostic can disclose a full agent credential. No real key, deployed use of the alias, log exposure or compromise was observed. P2 is justified by the demonstrated disclosure path; P1 is not established by the evidence.

**Correction:** ensure neither legacy field can reveal a secret through credential/client/builder `Debug`; redacting both is the simplest complete rule. Retain the wire-compatible authentication form. A focused key/key regression is appropriate.

### T-02 — P2: optional-metadata recovery also swallows body-identity failure

**Code:** `src/ops/envelope.rs:179–251,288–318`; public consumers include `StornoInvoice::parse` and invoice creation. `src/ops/query_pdf.rs:83–93` uses the same helper but additionally requires a PDF.

The control flow is decisive:

1. The complete XML/root/namespace and unique verdict/code are read first.
2. If payload deserialization fails under 56, `parse_envelope` rereads a scalar `Identity`, explicitly intended to reject duplicate/nested `szamlaszam`.
3. If that identity read also fails, `parse_reply` discards **every** payload error under notification failure (`:208–211`).
4. `Body::default().invoice_number(response)` then selects the header number, turning the response into a typed issued document.

Independent synthetic response, HTTP 200:

```text
szlahu_error_code: 56
szlahu_szamlaszam: I-4
```

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres><hibakod>56</hibakod>
  <szamlaszam>I-2</szamlaszam><szamlaszam>I-3</szamlaszam>
</xmlszamlavalasz>
```

`StornoInvoice::new("I-1").parse(...)` returns the number **I-4**, warning true. The same happens with identical duplicate numbers and with `<szamlaszam><bad/></szamlaszam>`. It also happens **without the error-code header**, when body 56 and the number header are present. Without a number header, malformed identity stays unknown; ordinary successful XML with the same malformed identity is refused even with a number header. A readable non-56 body refusal still wins.

**Resolution of the disagreement:** the mutations reviewer is correct that no ambiguous *body* number is selected, and no vendor conflict-precedence rule establishes that the header is wrong. Source X is not, by itself, a mandate for full XSD validation. However, the crate independently promises rejection of duplicate singleton/scalar-child structure (`README.md:253–264`), limits leniency to optional metadata (`:234–237` and envelope `:173–177`), and explicitly checks scalar identity (`envelope.rs:300–306`). A failed identity check disappearing in the outer fallback violates that local boundary. The narrow README sentence “malformed or duplicate body identity is never selected” does not resolve the broader promise or make malformed identity equivalent to absence.

**Accept as a local response-integrity defect, not a proven vendor-compliance failure.** The result carries financial-document identity as confirmed issuance, which warrants P2 rather than treating this solely as an XML nicety. The particular header can still be correct; the probe does not prove wrong adoption, duplicate issuance or a false reversal. `CreatedInvoice::reverses` remains a separately documented heuristic, and storno wire success alone is not verified reversal.

**Correction:** distinguish missing body identity from malformed/duplicate identity. Keep header fallback for genuine absence, valid unique body identity despite malformed optional totals/PDF, and the established numbered-56 notification exception. Refuse malformed identity as uncertain rather than concealing its failed check. Current tests at `tests/response_headers.rs:507–535` omit the number-header-present negative cases. A general rule for conflicting *valid scalar* body/header numbers is a separate policy question; this adjudication does not infer one from the vendor.

**Occurrence:** all malformed combinations here are fabricated offline. The account note says 56 could not be triggered (`docs/szamlazz-hu-behaviour.md:153,171`). Neither malformed identity nor even the precise numbered-56 wire combination was newly observed from szamlazz.hu.

### TQ-01 — P3: namespace-invalid extension markup passes shared validation

**Code:** `src/xml.rs:39–70,417–427`; taxpayer calls it at `src/ops/taxpayer.rs:404–424`.

The shared reader rejects unknown prefixes but does not reject element prefix `xmlns`, which the resolver already knows. `valid_pi_target` deliberately uses XML `Name`, including colon, rather than namespace-conforming `NCName`. Prefixes having no namespace meaning on a PI does **not** exempt PI targets from W §7.

Fresh W is unambiguous:

- §3: “Element names MUST NOT have the prefix `xmlns`.”
- §7: “No entity names, processing instruction targets, or notation names contain any colons.”
- §8 requires reporting namespace-well-formedness violations.

The probe inserted `<xmlns:extension/>`, its paired-tag equivalent, and `<?p:target data?>` into sparse otherwise-readable taxpayer replies. All returned `valid=true` under both NAV layouts. Legal `<?é data?>` and `<xml:extension/>` controls also passed.

**Impact and occurrence:** the extensions are ignored, and neither injects a verdict nor changes taxpayer data. No live occurrence, authentication bypass or business-identity error is established. The queries report's “minor conformance boundary” and taxpayer report's P3 are compatible on impact; this adjudication records it as a real low-priority shared-validator defect because the namespace rule and advertised validation boundary are explicit.

**Correction:** reject exactly the forbidden element prefix and require NCName PI targets while retaining the reserved `xml` target check. Preserve legal non-ASCII names, ordinary `xml:*` elements and legal bound prefixes merely beginning with `xml`; W specifically prohibits making the latter fatal solely for their spelling.

### T-03 — P3: stronger protocol guarantees than the cited evidence supports

**Code:** `src/error.rs:3–5,333–340`.

Accept both corrections:

1. “Never via HTTP status codes” turns documented in-band domain errors into a universal guarantee about HTTP failures. The actual `HttpStatus` path and README already handle non-2xx conservatively. Say domain errors are reported in-band; do not assert HTTP failures are impossible.
2. “Before it looks at the request (its documentation)” attributes an exact processing order to the vendor that A/E do not provide. “The same request succeeds once the account is fixed” also overlooks later document validation. Describe 3/135/136/164 as authentication/access refusals and identify the exchange-level `Rejected` interpretation without claiming unconditional subsequent success.

**Occurrence:** this is directly observable documentation, not evidence that those codes are actually post-write errors or that the runtime classification is wrong. No code-classification change follows from missing support for the stronger prose. Preserve `recovery.md:4–9`: a refusal of this exchange cannot settle an earlier lost send.

The adjacent assertion that the endpoint “never redirects” (`src/client.rs:293–299`, already noted in the transport report) merits the same provenance cleanup. Keep the no-redirect policy; do not imply all redirects convert POST to GET. This is part of the same documentation cleanup, not an additional ranked finding.

## Downgraded ambiguities and opportunities

### M-01 — numberless successful credit acknowledgement: unresolved compatibility question

**Confirmed local fact:** `src/ops/credit_entry.rs:250–256` requires a number in body or header to build `InvoiceBalance`. The probe supplied `<sikeres>true</sikeres><kintlevoseg>0</kintlevoseg>` and no headers; the result was `Parse(Missing("szamlaszam"))`. Adding only a number header permits success. This probe exercises parsing, not an executed credit registration.

**Why not confirm the report's medium-severity vendor-contract defect:** C's XSD allows the number to be absent, and its generic optionality warning supports a compatibility concern. But the **same type covers failures**, whose examples intentionally omit the number. A successful-always-numbered implementation is compatible with that schema and with “not always present” across all responses. Optional headers do not establish that the number can be absent from **both** channels on success. Both current XML success examples carry it. Their malformed literal URL ampersands are illustrative-source defects, not evidence for a numberless success.

Conversely, the examples do not prove an always-numbered guarantee either. Version 1's `xmlagentresponse=DONE` shows that acknowledging this mutation need not conceptually allocate a new identity, but is not a version-2 response rule. Thus neither “numberless success is definitely supported” nor “numberless success cannot happen” follows from current sources.

This exact issue already exists in [vendor questions §3](../research/2026-09-10-agent-vendor-questions.md#3-successful-credit-entry-registration-without-an-echoed-number-highest-priority), explicitly an **unsent draft**, with no numberless successful registration observed. Fresh C does not answer it. Retain it as the highest-priority vendor clarification; do not silently promote the draft into an answered question or claim a vendor ticket was sent.

**Operational impact if confirmed:** a legitimate acknowledgement would be lost; a caller that wrongly retries could append twice or replace intervening entries. The crate classifies the parse failure as uncertain and has no application retry loop; current recovery guidance tells callers to reconcile first. Duplicate mutation is therefore conditional, not a reproduced automatic consequence.

**Next decision:** obtain a success-specific rule or complete captured numberless success. If supported, expose an optional echo or separately identify the request's target. Never populate vendor-reported `invoice_number` from the request without making its provenance explicit. Documenting the current number requirement would be useful now. The schema mismatch is established; its operational classification remains open.

### R-NAV-1 — account onboarding note, not a P2 crate defect

The omission is real: `README.md:155–217` and `src/ops/receipt.rs:91–113` cover issuance, recovery and email without linking receipt-reporting setup. Fresh R2/R3 establish a concrete account permission worth mentioning; this is not a hypothetical requirement. Fresh R1 still has older “working on automation / nothing to do” wording. The sources disagree on rollout state; the more specific HU guidance and step 13 support linking the current setup instructions, not claiming observed delivery.

**Downgrade:** no missing request field, mishandled response, false NAV-acceptance flag or promise of complete account onboarding is demonstrated. `Receipt` models the issued document (`receipt.rs:536–598`), not a NAV submission acknowledgement. Account configuration and vendor forwarding happen outside this crate. The seriousness of the operator's reporting obligation does not by itself make an omitted README link a medium-severity implementation defect.

**Useful P3 opportunity:** add a short dated link to current vendor setup guidance, name the separate technical-user permission, and distinguish receipt issuance from NAV reporting. Do not copy the stale “nothing to do” assertion or invent a request flag/status field. No reporting failure or account misconfiguration was observed. The September 10 rollout remains a first-party statement, not a newly verified account fact; this review does not independently adjudicate legal compliance.

## Disposition of the rest of the six reports

| Slice | Conclusion retained after focused code checks |
|---|---|
| Invoice requests | No additional confirmed functional finding. Current writer explicitly follows download/PHP preview-before-simple order (`invoice.rs:819–825`); reversing it based on the conflicting inline schema alone is unjustified. The exact-coefficient arithmetic implementation is present (`number.rs:6–59`), so earlier lossy-arithmetic conclusions must not be recycled. Detailed fresh schema counts and prior reproduction results remain attributed to the invoice report, not rerun here. |
| Mutations | Preserve numbered 56, preview distinction, wire-success storno/no-op semantics and all-match deletion. Accept the local T-02 boundary defect; retain true-verdict-plus-code-56 warning classification as a separate synthetic policy ambiguity. M-01 is not a second confirmed P2. |
| Queries | Retain no additional operational query defect. XML/PDF entry points preserve their separate contracts; exact finite numeric/date domains and malformed artifact handling are documented boundaries. HU PDF schema drift and placeholder examples remain vendor-documentation issues. Shared TQ-01 is counted once, not again as a query injection finding. |
| Receipts | Retain no confirmed receipt-specific wire defect. Stale downloadable schemas, call-ID scope, foreign-currency behavior, partial email semantics and optional artifact recovery do not become code defects merely by disagreeing with examples or lacking live evidence. R-NAV-1 is the onboarding opportunity above. |
| Taxpayer | Retain no taxpayer-specific missing business-field/verdict defect. Path/namespace extraction is explicit; omitted diagnostics and sparse business content are capability policies. Shared TQ-01 is accepted at P3. Generic NAV error forwarding and OK-without-validity remain clarification questions. |
| Transport | Accept T-01/T-02/T-03 at their stated bounded severities. No new route, cookie-isolation or platform defect follows from these findings. Ordinary header/body precedence, repeated headers and 56-on-PDF-query remain library policies with limited operational evidence. |

Historical account evidence was read from `docs/szamlazz-hu-behaviour.md`, not reproduced. In particular, numbered successful credit replies (`:145`), body-only failures (`:141`), and inability to trigger 56 (`:153`) do not establish universal response rules. Its one-test-account scope and unavailable original logs limit all operational extrapolation.

## Independent verification record

New probe source: `/tmp/opencode/agent-adjudication-2ba5fb86.rs`, authored with `apply_patch`. It uses public interfaces with fabricated XML and placeholder credentials; it never calls `Client::send`.

Executed successfully:

```sh
git rev-parse HEAD
git diff --exit-code HEAD -- crates/szamlazz-agent Cargo.toml Cargo.lock docs/szamlazz-hu-behaviour.md
cargo build -p szamlazz-agent --locked --offline --features client-reqwest
rustc --edition=2024 /tmp/opencode/agent-adjudication-2ba5fb86.rs \
  --extern szamlazz_agent=target/debug/libszamlazz_agent.rlib \
  -L dependency=target/debug/deps -o /tmp/opencode/agent-adjudication-2ba5fb86
/tmp/opencode/agent-adjudication-2ba5fb86
```

| Independent check | Result |
|---|---|
| T-01 key/key through credential, builder, client diagnostics | Key present in all three; preferred agent-key diagnostic redacted. |
| T-02 duplicate-different, duplicate-same, nested identity; body 56 plus number header, with/without header 56 | All six return header I-4, notification warning true. |
| T-02 malformed identity without number header | Unknown outcome; no issued result. |
| T-02 same malformed identity under ordinary success with number header | Parse failure. |
| T-02 absent body identity, including malformed optional total | Header fallback succeeds. |
| T-02 readable body code 3 despite header 56 and malformed identity | Code 3 retained. |
| M-01 true credit verdict, no number / number header control | Missing-number parse failure / successful balance. |
| TQ-01 forbidden `xmlns` element in empty/paired forms and colon PI, both NAV layouts | All six malformed-namespace inputs accepted; validity unchanged. |
| TQ-01 legal non-ASCII PI and `xml:*` controls, both NAV layouts | All four accepted. |

The [taxpayer report §10](2026-09-11-agent-api-taxpayer.md#10-verification-and-reproducibility) reports the full all-features crate suite passing: **265 unit/integration tests + 8 doctests; four live tests ignored**. That is an attributed prior result, not a suite rerun by this adjudication. Existing green tests do not cover all reproduced gaps. No additional repository test suite was run.

Final checks confirmed the pinned HEAD and empty in-scope source/test/fixture/manifest diff; the report passed `git diff --no-index --check /dev/null docs/review/2026-09-11-agent-api-adjudication.md`. Existing unrelated work and the six input reports were preserved.

**Recommended order:** fix the supported-credential diagnostic leak and the identity-error fallback; correct the small namespace checks and protocol prose. Keep M-01 as a success-specific vendor question and add receipt setup links as an onboarding improvement. None of these conclusions licenses a live probe or an automatic retry of an uncertain mutation.
