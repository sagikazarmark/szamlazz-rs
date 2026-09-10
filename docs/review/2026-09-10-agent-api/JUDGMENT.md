# Independent judgment: Számla Agent conformance review

**Date:** 2026-09-10. **Decision:** retain **11 distinct actionable items: 2 P2 and 9 P3**, comprising **6 documentation/example defects, 4 bounded implementation defects, and 1 conformance-hardening item**. No P0/P1 established. Reject IR-02 on the normal public request path; exclude TP-01 from defect counts as a projection/capability choice. Merge IO-02 and QX-01 into one namespace-hardening item.

These counts are not eleven failures on ordinary vendor traffic. The strongest normal-workflow failure is the README reconciliation example. The remaining implementation counterexamples concern raw numeric VAT construction, legal numeric padding, extreme Decimal scale and deliberately collision-heavy multipart content. None establishes a newly observed vendor incident.

## Baseline and method

- Read all five `raw/01…05*.md` reports in full and traced their proposed findings through current source. The reports are claims to adjudicate, not independent confirmation of one another.
- Initial and inspected HEAD: **`f54dac78cd1f7f981cd70d2d29ee3376be5b9bd3`**. `git diff --name-status bc71841 f54dac78` contains only four `restate-e2e-harness` files. The one intervening commit is `f54dac7 fix(restate-e2e-harness)!: honor observation budgets and name idle evidence`. **No agent source, manifest or lockfile changed between the two review baselines.** There was no working-tree diff in `crates/szamlazz-agent`, `Cargo.toml` or `Cargo.lock` at inspection. The coordinator README's older baseline therefore does not invalidate these agent findings.
- Independently fetched relevant vendor pages/schemas, W3C datatype rules, RFC 2046 and reqwest 0.13.4 documentation. Read the already downloaded original vendor VAT PDF and selected first-party PHP ZIP members; those are primary-source artifacts, not account executions. Source conflicts not essential to retained findings are adjudicated from the reports' inventories and repository provenance, with selective independent checks specified below.
- Ran an independent, external consumer probe against the current path dependency:

  ```text
  cargo run --offline --quiet --manifest-path /tmp/opencode/agent-judge-20260910/Cargo.toml
  ```

  It uses built-in requests, `to_wire`, public response parsers and synthetic bodies. It confirms the character-boundary rejection, both VAT paths, README marker controls, namespace siblings, Decimal underflow and a 102-character boundary. A second run added body/header Decimal siblings. Both completed successfully; their assertions establish current counterexamples, not fixes.
- No production edits, repository test additions, account calls, delegation or full-suite run. Other untracked review/research work was left untouched. This report is the only repository file authored by the judge. The coordinator owns suite results; specialist test counts are not added together as unique judge-run coverage.

Paths below beginning `src/` or `README.md` are relative to `crates/szamlazz-agent/`.

## Every proposed finding: disposition

| Proposal | Decision | Final priority / class | Reason and impact boundary |
|---|---|---|---|
| **IR-01** raw numeric `VatRate::Other` | **Retain** | P3 implementation | Calculator returns zero VAT for the literal taxable token `27.00`; an ordinary invoice containing that item passes `to_wire`. Explicit Rust construction only; normal unpadded `From`/serde selects `Percent`. |
| **IR-02** forbidden XML controls escape request boundary | **Reject as stated** | Not counted | `validate()` succeeds and `write_xml()` emits the character, but `to_wire()` rejects it, and `Client::send()` always calls `to_wire()` before HTTP. The reviewer stopped before the actual request boundary. |
| **IR-03** EUT/EUKT/HO/EUE descriptions | **Retain** | P3 documentation | Short descriptions lose goods/third-country/non-reverse-charge distinctions confirmed in current vendor guidance. Correct tokens, misleading selection guidance. |
| **IR-04** `K.AFA` text-driven subtype | **Retain** | P3 documentation | Vendor PDF explicitly describes exact-wording selection and used-goods fallback. This is a consequential processing note, not a missing XML field or a requirement to build a tax engine. |
| **IO-01** order-based proforma deletion scope | **Retain** | **P2 documentation** | Current Hungarian source explicitly says **all** matching proformas. Public selector and recovery guidance conceal that destructive scope through singular descriptions. Batch execution was not observed live. |
| **IO-02** foreign envelope children | **Merge with QX-01** | P3 hardening, one item | Local-name extraction can adopt a foreign verdict, number or amount. Unexpected response shape; not failure to parse normal vendor XML. |
| **IO-03** issuer-tax-number receipt matching | **Retain** | P3 documentation | Current Rust repeats the English example's mistranslation. Hungarian prose and English inline schema describe incoming payment-to-invoice matching, not invoice-to-receipt matching. |
| **R-01** padded response VAT | **Retain; extend scope to invoice helpers** | P3 implementation | Legal XML numeric padding becomes `Other`; typed grouping is wrong and later calculation can become zero VAT. Original raw token and reported amounts remain intact. |
| **QX-01** foreign invoice children | **Merge with IO-02** | P3 hardening, one item | Same root-only namespace validation plus namespace-insensitive serde. Undeclared prefixes are an additional case of the same boundary weakness. |
| **QX-02** silent Decimal underflow/rounding | **Retain, narrowly** | P3 implementation / numeric integrity | Nonzero `1e-29` in plain notation becomes zero; exponent notation errors. Do not turn this into a demand to implement the entire `xs:double` range. Include shared envelope/header consumers in the repair. |
| **TP-01** omitted NAV metadata/notices | **Exclude from defect counts** | Capability decision | True omission, but `TaxpayerInfo` is a business projection. No lost validity/business field, broken parse, or current forwarded notice requiring action was established. Document the projection; add metadata only if a consumer needs it. |
| **P05-01** multipart boundary length | **Retain** | P3 implementation / outbound conformance | A built-in request produces a 102-character boundary through `to_wire`; MIME's maximum is 70. Strict receiver rejection is possible, not observed. |
| **P05-02** README live-marker predicate | **Retain** | **P2 executable documentation** | Exact example rejects an ordinary live invoice with an omitted marker. Parser and public field guidance are already correct. No automatic duplicate send exists in the example. |
| **P05-03** “never retries automatically” | **Retain, docs-only scope** | P3 documentation / transport ownership | Injected reqwest policies remain active. This is not evidence that the isolated default retries ambiguous application failures or that the crate's recovery logic runs during transport retries. |

**Count reconciliation:** 14 proposals − 1 false positive − 1 capability-only omission − 1 duplicate namespace report = **11**. IR-01 and R-01 remain two causes/checks under one VAT repair workstream; counting that workstream as a single ticket would yield ten tickets, not ten distinct causes.

## Retained findings: evidence and recommended repairs

### P2: fix the recovery example and deletion scope first

**P05-02 — `README.md:136–142`.** The predicate is literally `document.info.reversed == Some(false)`, after order and document-type checks. `src/ops/query_xml.rs:300–316,750–751,784` preserves `None` and explicitly tells callers to use `!= Some(true)` for live. The freshly fetched [invoice response schema][invoice-xsd] makes `sztornozott` optional; the [query response example][query-response] has no marker. `docs/szamlazz-hu-behaviour.md:77` records absence before storno and true afterwards on the test account.

The judge parsed matching-order, `SZ`, paper invoice bodies and evaluated both predicates:

| Marker | README adopts | Corrected marker predicate adopts |
|---|---|---|
| absent | no | yes |
| false | yes | yes |
| true | no | no |

Use `reversed != Some(true)` while retaining identity checks and unresolved handling of absent/wrong/unanswered queries. The user impact is stalled reconciliation after a landed create's lost reply, not a parser failure or proven duplicate issuance. Compilation alone would not catch it. **High confidence.**

**IO-01 — `src/ops/proforma.rs:14–27,33–49,68–72`; `src/recovery.md:16`.** Independently fetched [Hungarian deletion XML documentation][delete-hu] states:

> Ha azonos rendelésszámmal több díjbekérő is van a számlázási fiókban, akkor a törlés az összes díjbekérőre vonatkozik.

That means deletion applies to **all proformas with that order number in the account**. The English omission is not a competing single-target guarantee. The writer correctly sends `rendelesszam`; success is `()` with no count or deleted-number list. An order query returning one document does not narrow deletion to that document. Document the batch scope on `OrderNumber`, `DeleteProforma` and recovery guidance, and distinguish number-based deletion. Repeating the order selector can reach later matching proformas. No automatic query/guard or new response field is justified. The existing live record confirms order deletion and paid-proforma deletion separately, not a multi-match batch. **High confidence in documented scope; no live batch verification.**

### P3: two numeric VAT causes, one common impact

**IR-01 — `src/item.rs:179–212`; `src/types.rs:258–315,1140–1142`.** The calculator branches on `Percent`, while `Other` is rendered verbatim. `Other("27.00")` therefore produces net/VAT/gross **100/0/100** and sends `afakulcs=27.00`. The same input through `VatRate::from("27.00")` produces **100/27/127**. The judge confirmed the contradictory item passes the complete `to_wire` boundary. The vendor's [VAT list][vat-rules] allows numeric percentages; the behavior record's P60 row confirms `27.00`/`27.0` acceptance, not acceptance of the contradictory amounts.

Interpret supported raw numeric tokens consistently in derived arithmetic, or refuse derivation when it cannot establish the rate. Preserve open tokens and explicit caller-computed items. Do not claim normal JSON decoding produces IR-01. **High confidence; uncommon construction path.**

**R-01 — `src/ops/receipt.rs:637–643`; `src/ops/query_xml.rs:451–457,494–499`; `src/types.rs:1079–1085`.** The response helpers all call the same untrimmed conversion. The receipt and invoice schemas define numeric `afakulcs` as a restriction of `double`; [W3C whiteSpace][xml-space] is fixed to collapse for this type. ` 27 ` and XML-tab/newline-padded 27 denote the same percentage as 27. The judge independently reproduced receipt item/subtotal and invoice item/subtotal returning `Other`, and passed the invoice helper results to the calculator to confirm zero VAT. The financial-item helper has the identical code path.

Preserve raw response text. In the numeric response branch, remove only XML boundary whitespace before interpretation; keep special `afatipus` precedence and unknown tokens verbatim. Test each helper family, numeric padding, normal/exponent percentages and special/unknown controls. Fixing this helper alone does not fix callers constructing raw numeric `Other`; fixing only the calculator leaves typed grouping wrong. These are related, not duplicate findings. **High confidence in valid-token misinterpretation; vendor emission of padding unobserved.**

### P3: numeric integrity at the Decimal boundary

**QX-02 — `src/xml.rs:431–442,495–503`; shared totals and queried quantities/rates/credit entries.** Generic `FromStr` calls rust_decimal 1.43.0's rounding parser. The dependency source distinguishes that parser from its exact parser. Independently reproduced:

| Numeric text in queried exchange rate | Result |
|---|---|
| `0.00000000000000000000000000001` | `Some(0)` |
| `1e-29` | parse error |
| `1e-2` | `Some(0.01)` |

The judge also confirmed plain-form nonzero→zero in a required recorded credit-entry amount. **Additional verified scope:** `src/ops/envelope.rs:306–313,365–370` uses the same rounding behavior for body and header money; a public storno parse returns `gross_total=Some(0)` for that tiny positive value on either channel. This is the same cause, not another counted defect. It matters to downstream zero/sign decisions, but no real zero/positive-original storno incident is established.

Adopt an explicit finite-number conversion policy, with consistent refusal of unrepresentable nonzero values rather than silent source-value change. Preserve ordinary exponent support and header-specific comma grammar. Do not mechanically replace every parser with an exact routine without checking exponent behavior and all callers. The repair should cover optional/required amounts, envelopes, headers, and numeric VAT interpretation where the same conversion is used. Keep intentional numbered-56 optional-metadata handling.

This is **not** a demand for NaN/INF, arbitrary-precision money, or every valid XSD numerical value. The exponent form is an actual refusal of a schema-legal finite token, but it lies outside Decimal's domain; the actionable defect is inconsistent silent conversion of the equivalent plain form. Likewise, many extra decimal digits need not denote distinct IEEE-double values. **High confidence in underflow; low expected incidence, no observed vendor case.**

### P3: one namespace-hardening item

**IO-02 + QX-01 — `src/xml.rs:87–112,254–273`; `src/ops/envelope.rs:272–275`; `src/ops/query_xml.rs:558–575`.** The root's expanded name is checked, then serde recognizes descendants by local name. The [invoice][invoice-xsd], [envelope][envelope-xsd] and [receipt][receipt-xsd] schemas require qualified children. Independently reproduced through public parsers:

- Correct-root invoice plus foreign `sztornozott=true` returns `Some(true)`; an undeclared prefix does too.
- Deletion with only foreign `x:sikeres=true` returns success.
- **Receipt-send sibling:** the same foreign-only success verdict is accepted. Receipt payload parsing also goes through the same shared serde boundary (`receipt.rs:681–711`).

Treat this as **conformance hardening for unexpected responses**, not evidence that valid normal vendor XML fails. Accepting sparse content and extensions is useful; adopting a foreign element as the protocol's identity/verdict is a different issue. Validate recognized expanded names and parent paths, keep correct-URI arbitrary prefixes, ignore unrelated extensions and reject undeclared prefixes. A full XSD validator is unnecessary. Taxpayer's layout-aware extraction is not another affected local-name adoption case, though a shared namespace-well-formedness repair should cover its document scan too. **High confidence in mechanism; live occurrence/exploitability unestablished.**

### P3: overlong multipart boundary

**P05-01 — `src/wire.rs:109–120,398–404`; `src/client.rs:300–309`.** A built-in XML query whose external-id text is the base boundary followed by 70 `x` characters returns a **102-character** boundary. `multipart_boundary` repeatedly appends `x` while each candidate occurs in content. No custom trait implementation or malformed XML is required. The same construction can be placed in a free-text invoice field or attachment.

[RFC 2046 §5.1.1][mime] limits boundary parameters to 70 characters; [RFC 7578 §4.1][form-data] uses MIME multipart syntax. Select collision-free candidates with bounded width, checking XML and attachment bytes. Do not truncate a colliding candidate. Test collision avoidance and maximum length together. **High confidence in emitted nonconformance; actual vendor refusal and performance severity unverified.**

### P3: semantic and transport documentation

**IR-03 — `src/types.rs:197–209`.** The freshly fetched [Hungarian VAT table][vat-rules] says EUT/EUKT are intra-/extra-EU **goods supplies**, HO is a **third-country** transaction, and EUE is an other-member-state **non-reverse-charge** transaction. Preserve those distinctions in variant docs. The PDF corroborates them. Keep KBAUK's existing new-means-of-transport meaning; “UK” in the short table is not United Kingdom. **High confidence in wording mismatch, no tax incident established.**

**IR-04 — `src/types.rs:203–204`; related comments in invoice/item docs.** Read the original [vendor VAT PDF][vat-pdf], whose `K.AFA` row says the vendor searches invoice comment, item name and item comment for `utazási irodák`, `használt cikkek`, `műalkotások`, or `gyűjtemény darabok és régiségek`; unmatched/absent wording defaults to the used-goods subtype. Link and quote the vendor rule concisely on `KAfa`. All input locations are already writable. Do not infer case-folding, substring matching, precedence among multiple matches, or independently endorse every tax-law statement in the PDF. **High confidence in documented vendor rule, no NAV/live execution verified.**

**IO-03 — `src/ops/credit_entry.rs:159–162`.** Replace “matches the incoming invoice with the corresponding incoming receipt” with source-qualified incoming credit-entry/payment-to-invoice assignment. The freshly fetched [Hungarian example and schema][credit-hu] say `bejövő kifizetést a megfelelő számlához rendeli`. The reported English inline-schema agreement explains why this is a translation correction despite the English example disagreeing. The operation has no receipt selector and returns an invoice balance. Do not invent incoming-ledger precedence or cross-account behavior. **High confidence in correction; detailed vendor execution unverified.**

**P05-03 — `README.md:347`; `src/recovery.md:4–6`; `src/client.rs:131–166,236–247,300–309`.** A supplied HTTP client is used unchanged and owns transport settings; requests carry cloneable in-memory bodies. [Reqwest 0.13.4][reqwest-retry] documents retries and caller-defined classifiers. Its locked source's `retry.rs:195–202,253–267,301–314` confirms default protocol-NACK policy and replayable bodies. The specialist's loopback three-POST experiment is corroborating evidence, not rerun by the judge.

Say **no application-level retry/recovery loop**, and explicitly name supplied transport retry policy alongside its other responsibilities. Do not classify the supported injection behavior itself as a production implementation bug. The isolated crate enables cookies/rustls, not HTTP/2/3; consumer feature unification can change this. No default application-503 or ambiguous-write repeat was established. Explicitly disabling default reqwest retry is a separate contract choice, not a required fix inferred here. **High confidence in documentation overstatement.**

## Rejected finding and capability-only omission

### IR-02: the public path disproves the claimed escape

```text
Client::send (client.rs:300–301)
  -> AgentRequest::to_wire (wire.rs:398–404)
     -> validate()
     -> write_xml()
     -> validate_xml_10()
        -> RequestError::InvalidXmlCharacter(11)
     -> multipart / HTTP only on success
```

The judge built the reviewer's ordinary invoice with `Buyer\u{000b}Co`: `validate()` succeeds, `write_xml()` contains byte 11, **`to_wire()` rejects**. `validate` is documented as cross-field validation; `write_xml` is a low-level public serialization hook returning bytes. Its direct-call contract could be clarified to point callers to `to_wire` for checked requests, but this does not warrant a second character validator, field-attributed error redesign, or a retained normal-path nonconformance finding. **Rejection confidence: high.**

### TP-01: true omission, not a broken taxpayer query

`src/ops/taxpayer.rs:190–227,340–403,595–625` intentionally projects taxpayer business information and drops header/software, OK messages and notices. The [current forwarding example][taxpayer-response] contains header/software; [NTCA common 1.0][nav-common] defines optional messages/notifications. Their presence in a schema does not require every SDK result to expose them. No current notice forwarding or changed business verdict was proved. The old five-business-field gap is closed.

Document that `TaxpayerInfo` is a projection. A caller using Sans-I/O can retain `RawResponse`; **the bundled `Client::send` does not return that raw exchange**, so do not imply it is accessible beside every typed result. If a real diagnostic consumer requires metadata, design that capability explicitly. **High confidence in omission and exclusion from normal conformance defects.**

## Disposition of conflicts, optional omissions and secondary concerns

Report-local conflict IDs are prefixed below to avoid confusing unrelated `SC-01` entries. None is included in the 11-item count.

| Report/topic | Judgment |
|---|---|
| **01 SC-01: preview + simpleItems order** | **Unresolved primary-source conflict, not a proven repairable writer defect.** Current writer follows download/PHP (template → preview → simpleItems), while current inline EN/HU reverses the last two. Judge checked writer, separate retained source excerpts/provenance, and actual PHP lines 398/400/404. This is not live-backed. Preserve group/erasure support; obtain vendor clarification or a separately authorized combined-option probe. Never auto-fallback by dropping preview or changing order after an uncertain call. |
| **01 SC-02: layout labels** | **Unresolved source conflict.** Judge freshly confirmed API table versus linked knowledge-base inversion of SzlaAlap/SzlaNoEnv. Keep tokens; do not invert mappings without rendering evidence. Naming debt is not an extra serialization bug. |
| **01 other limits** | Explicit `fizetve=false` versus omission, additional flag/reference combinations, gross-first constructor and rate-free special kinds are unproved behavior differences or convenience/expressivity limits. Current explicit-item/automatic-MNB paths cover documented useful cases. Carrier lengths, account eligibility, contract-only fields and business preflight remain server-owned; no additional finding. |
| **02 SC-01: HU PDF XSD** | EN/download and request prose support current selector/order policy; malformed/conflicting HU inline schema is not authority to remove selectors or insert a fake invoice number. Preserve separate sources. |
| **02 SC-02 / 05 storno external id** | Keep bounded observed attachment-to-new-SS behavior and required original number. Do not implement external-id-only original selection from conflicting prose. |
| **02 SC-03 / 05 malformed success examples** | Bare ampersands and abbreviated base64 are source defects. No parser relaxation or fabricated capture provenance. Positive-gross illustration does not prove a storno execution or justify strengthening `reverses()`. |
| **03 S-01** receipt download omissions | Preserve documented erasure and order-query fields. Stale downloads do not establish missing crate conformance. |
| **03 S-02** `all` versus order prose | Current writer is justified. Different declaration/PHP order within `xs:all` is not an ordering defect. |
| **03 S-03** receipt automatic MNB | First-party-source-supported behavior with no receipt live probe. Keep qualification; do not count it as a live-tested exception. |
| **03 S-04** VAT lists/illustrative receipt inconsistencies | Open tokens and monetary aliases already preserve supported data. No named-TEHK requirement; no acceptance of placeholder PDF needed. |
| **03 S-05 / S-06** receipt toggle/reporting rollout | Preserve as vendor clarification questions. No local toggle assumption or missing reporting flag is established. Do not invent NAV request/status fields or guarantee rollout completion. |
| **03 S-07** PHP send-PDF comment | Actual send schema is verdict-only. `Response=()` is appropriate; query can obtain PDF. |
| **03 O-01 / O-02** no-op send/call-ID-only recovery | Omitted no-send block is not a useful missing send capability; present-empty resend stays. Call-ID-only receipt lookup and automatic recovery are not demonstrated vendor capabilities. |
| **04 source/version/field questions** | Keep genuine NAV 2.0 and 3.0 layouts, sparse/open business fields and advisory infoDate text. Wrong schema links do not make request-only waybill/rate blocks into missing queried fields. Additional simple-address detail acceptance is extra leniency, not evidence NAV sends it. Generic NAV error roots are not established Számla Agent forwarding requirements. |
| **02/05 empty credit-entry replacement** | Intentional restriction, explicitly awaiting verification. Schema expressivity alone does not justify enabling destructive clearing; label any “would clear” wording as implied semantics, not observed execution. |
| **02 response metadata extras / 05 raw exchange, v1 and streaming** | Capability choices, not newly broken documented operation paths. Do not reopen previously excluded PDF id/payment/warning expansions or mandate v1/streaming. |
| **04/03 permissive parsing** | Empty/default booleans, sparse fields, extra labels, Unicode numeric padding, underscore acceptance, broad integer widths, finite date/money domains and unknown business tokens are not each separate normal-API failures. Namespace adoption and silent numeric underflow are the narrowly retained exceptions above. |
| **05 secondary documentation claims** | Credential processing-order attribution, “never HTTP status”/“never redirects,” omitted two-day replay qualification, browser-secret guidance and legacy key-in-username Debug exposure deserve bounded wording/usage notes if those docs are touched. No new runtime defect or exposed key was established. Do not turn this review into unrequested authentication redesign or eliminate wasm/legacy credential support. |
| **02/04 minor guidance opportunities** | Storno buyer identifiers fill missing original data; XML query is documented for internal outgoing documents. Useful clarifications, but current code does not promise a contradictory overwrite/imported-document capability. Not separately prioritized. |

## Evidence boundaries and completion recommendation

**Fix order:** (1) both P2 documentation paths; (2) the two VAT causes; (3) bounded Decimal conversion and multipart candidate generation; (4) semantic/transport wording; (5) one coherent namespace-hardening change with recognized-path controls. This is a prioritization judgment, not a request to implement within this report.

**New scope concerns from judging, without count inflation:** padded VAT affects invoice/financial/subtotal helpers as well as receipts; namespace adoption includes receipt verdicts; Decimal underflow includes invoice-envelope body/header money. Fixing only the specialist-owned file would leave siblings behind. Conversely, the raw XML writer is not the checked request boundary, and omitted NAV metadata is not a failed taxpayer query.

**What remains unproved:** no newly observed failure to parse ordinary valid vendor response XML, live padding/extreme scales/foreign namespaces, vendor rejection of the overlong boundary, combined preview acceptance, template rendering, batch deletion execution, receipt lifecycle/call-ID retention/email/MNB/reporting behavior, or current forwarded NAV notification delivery. The existing account record remains bounded to its original test account/dates; its raw exchanges are not in this repository. Ordinary schema-valid numeric padding is demonstrably misinterpreted, and extreme finite numbers have demonstrably inconsistent conversion, without claiming either spelling was observed from the vendor.

**Historical closure:** current source supports the reports' closure of date suffix support, business-text preservation, named error additions, header decimal grammar, PDF outstanding/URL, create/storno payment method, taxpayer business additions, complete-root traversal and taxpayer recognized-path handling. These closures do not prove universal XML validation. The source/version conflicts and intentional live-backed deviations must remain visible in any final conformance statement; passing golden tests against modified cached schemas cannot settle them.

**Final confidence:** high for the retained code mechanisms and cited wording mismatches; bounded/unverified for their vendor incidence and execution consequences. **Final counts: P0 0; P1 0; P2 2; P3 9.** Of 11 actionable items, only four are implementation defects, one is unexpected-input hardening, and six are documentation/example corrections. The principal rejected claim is IR-02; the principal non-defect omission is TP-01.

[delete-hu]: https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml
[invoice-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[query-response]: https://docs.szamlazz.hu/agent/querying_xml/response
[envelope-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd
[receipt-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[vat-rules]: https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates
[vat-pdf]: https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf
[xml-space]: https://www.w3.org/TR/xmlschema-2/#rf-whiteSpace
[credit-hu]: https://docs.szamlazz.hu/hu/agent/credit_entry/xml
[mime]: https://www.rfc-editor.org/rfc/rfc2046#section-5.1.1
[form-data]: https://www.rfc-editor.org/rfc/rfc7578#section-4.1
[reqwest-retry]: https://docs.rs/reqwest/0.13.4/reqwest/retry/index.html
[taxpayer-response]: https://docs.szamlazz.hu/agent/querying_taxpayer/response
[nav-common]: https://raw.githubusercontent.com/nav-gov-hu/Common/release/common-1.0.x/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd
