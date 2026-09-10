# Számla Agent: current API conformance review

**Date:** 2026-09-10 · **Reviewed code:** `f54dac78cd1f7f981cd70d2d29ee3376be5b9bd3`

## Conclusion

**The crate has strong coverage of the documented Számla Agent operations and fields. We found no missing operation or unimplemented ordinary request field in the reviewed schemas. It is not yet an unqualified conformance pass:** there are two consequential documentation/example defects, four bounded implementation defects, four further documentation corrections, and one response-namespace hardening item.

**11 deduplicated actionable findings: 2 P2, 9 P3; no P0/P1 established.** These are review priorities, not eleven observed production failures. Six concern documentation/examples, four implementation, and one unexpected-response handling. Valid numeric XML whitespace is demonstrably misinterpreted; most other runtime counterexamples require unusual inputs. No authenticated account operation ran during this review.

Several official sources contradict each other. In particular, **the combined preview/`simpleItems` order remains unresolved**, rather than a proven crate defect or a live-tested exception. Recorded test-account behavior was preserved with its original limits.

Five specialist subagents reviewed the operation groups and current primary sources. A sixth independently judged their findings, rejected a false positive, merged overlapping reports and checked affected sibling paths. The coordinator inspected the critical decisions and ran the full offline crate suite.

## Prioritized findings

Code paths below are relative to `crates/szamlazz-agent/`. IDs preserve the specialist reports' evidence links.

### P2 — correct these first

| ID | Finding and impact | Location | Recommended correction |
|---|---|---|---|
| **P05-02** | **The README recovery example cannot recognize the normal live-invoice shape.** It requires `reversed == Some(false)`, but the vendor normally omits the marker before reversal. A successful reconciliation query therefore still returns `Unknown`. The production parser is correct; the example does not automatically resend. | `README.md:136–142`; `src/ops/query_xml.rs:300–316` | Use `reversed != Some(true)`, retaining the order/type checks. Verify absent, false and true cases. |
| **IO-01** | **Order-number proforma deletion deletes all matching proformas**, but the public selector and operation describe a singular target. A caller who queried one latest document can unintentionally request deletion of every proforma under that order. The writer already implements the vendor operation correctly. | `src/ops/proforma.rs:14–49,68–72`; `src/recovery.md:16` | State the batch scope explicitly on the selector, operation and recovery guidance; distinguish number-based deletion. The response supplies no count or deleted-number list. |

**Evidence:** the [invoice schema][invoice-xsd] makes `sztornozott` optional; the [official query example][query-response] omits it; `docs/szamlazz-hu-behaviour.md:77` records absent-before/true-after reversal. The [Hungarian deletion documentation][delete-hu] explicitly says “a törlés az összes díjbekérőre vonatkozik” — deletion applies to all matching proformas. Its English counterpart omits that sentence. **High confidence** in both mismatches; multi-match deletion was not newly executed against an account.

### P3 — bounded implementation defects

| ID | Confirmed behavior | Location | Recommended correction |
|---|---|---|---|
| **IR-01** | `VatRate::Other("27.00")` is emitted as a taxable numeric token, but `try_calculated` produces net/VAT/gross **100/0/100**. The request passes `to_wire`. Ordinary unpadded `From`/serde uses `Percent` correctly; this is a direct Rust-construction edge case. | `src/item.rs:179–212`; `src/types.rs:258–315,1140–1142` | Interpret supported raw numeric tokens consistently during calculation, or refuse a derivation whose rate cannot be established. Keep open wire tokens and explicit caller-computed items. |
| **R-01** | Schema-valid `<afakulcs> 27 </afakulcs>` becomes `Other(" 27 ")` in typed VAT helpers. This affects receipt, invoice, financial-item and subtotal helpers. Grouping is wrong; passing the result into the calculator can produce zero VAT. Original reported amounts and raw rate text remain intact. | `src/ops/receipt.rs:637–643`; `src/ops/query_xml.rs:451–457,494–499`; `src/types.rs:1079–1085` | Preserve raw text, but trim XML boundary whitespace for numeric interpretation. Preserve special-code precedence and unknown tokens. This and IR-01 have related impact but distinct causes. |
| **QX-02** | Plain-form **`1e-29` becomes zero**, while exponent spelling `1e-29` errors. Reproduced in queried exchange rates, credit-entry amounts, and storno body/header totals. Generic Decimal parsing silently rounds out-of-scale input. | `src/xml.rs:431–442,495–503`; `src/ops/envelope.rs:306–313,365–370` | Define a consistent finite-number conversion policy; reject unrepresentable nonzero values rather than silently changing them. Preserve ordinary exponent support, comma-header grammar and numbered-56 handling. |
| **P05-01** | Collision handling can produce a **102-character multipart boundary** from an ordinary built-in request. MIME permits at most 70. The algorithm repeatedly appends `x` until the candidate no longer occurs in content. | `src/wire.rs:109–120,398–404` | Generate bounded-width collision-free candidates; check both the maximum length and collisions across XML/attachments. Truncation alone is not a repair. |

**Evidence and confidence:** all four were reproduced offline through public interfaces and independently checked by the judge. The [vendor VAT list][vat-rules] supports numeric percentages; [XML Schema whitespace rules][xml-space] require collapse for `double`; the vendor [invoice][invoice-xsd] and [receipt][receipt-xsd] schemas use that type. [RFC 2046][mime] supplies the boundary limit. High confidence in implementation behavior; live padded/extreme numeric output and vendor rejection of overlong boundaries remain unobserved. Vendor-side monetary rounding is a separate fact and does not justify local response underflow.

### P3 — response conformance hardening

**IO-02 + QX-01, one finding:** root namespaces are checked, but subsequent serde extraction uses local names. A foreign `sztornozott` can mark an invoice reversed; a foreign `szamlaszam` can supply its identity; a foreign `sikeres=true` can make deletion or receipt-send parsing succeed. Undeclared child prefixes can also be accepted.

- **Locations:** `src/xml.rs:87–112,254–273`; `src/ops/envelope.rs:272–275`; `src/ops/query_xml.rs:558–575`; receipt payloads share the boundary.
- **Sources:** the [invoice][invoice-xsd], [envelope][envelope-xsd] and [receipt][receipt-xsd] schemas require qualified children.
- **Recommendation:** recognize expanded names at the relevant parent paths; accept arbitrary prefixes bound to the correct URI, ignore unrelated extensions and reject undeclared prefixes. Preserve sparse optional content. Full XSD validation is unnecessary.
- **Classification:** unexpected-response hardening, not evidence that ordinary valid vendor responses fail. Mechanism reproduced with high confidence; live incidence and exploitability unestablished. Taxpayer's versioned path extraction already avoids the corresponding foreign-field adoption.

### P3 — remaining documentation corrections

| ID | Correction | Location / source |
|---|---|---|
| **IR-03** | Preserve the VAT distinctions: EUT/EUKT concern **goods**, HO is **third-country**, EUE is **not reverse charged**. Current descriptions are broader than the vendor meanings. | `src/types.rs:197–209`; [VAT table][vat-rules] and [vendor VAT PDF][vat-pdf] |
| **IR-04** | Explain that `K.AFA` NAV subtype selection depends on the vendor's specified Hungarian wording in invoice/item text, with **used goods as the unmatched/absent default**. Existing text fields already express it; no XML field is missing. Do not invent matching/precedence rules beyond the source. | `src/types.rs:203–204`; [vendor VAT PDF][vat-pdf]; exact strings in [judgment](JUDGMENT.md) |
| **IO-03** | `issuer_tax_number` assists incoming credit-entry/payment-to-invoice assignment, **not invoice-to-receipt matching**. The Rust doc repeats an English example's mistranslation. | `src/ops/credit_entry.rs:159–162`; [Hungarian credit-entry example][credit-hu], corroborated by the English inline XSD |
| **P05-03** | Replace the absolute “never retries automatically” with **no application-level retry/recovery loop**, and document that injected HTTP clients retain their retry policies. One logical send produced three identical POSTs under an explicitly configured loopback retry policy. | `README.md:347`; `src/recovery.md:4–6`; `src/client.rs:131–166`; [reqwest retry contract][reqwest-retry] |

These four corrections have high evidence confidence. They establish neither a tax-processing incident nor default retries after an ambiguous write. The isolated crate's default does not enable HTTP/2/3; dependency feature unification and injected configuration can change transport behavior.

## Coverage and capability boundaries

| Reviewed surface | Result | Detailed inventory |
|---|---|---|
| Shared endpoint, multipart dispatch, authentication, cookies, headers, error classes | All **11 dispatch fields** match. All **37 numeric codes** in the fetched general catalogue and operation supplements are named, plus PHP-backed 56 and five recorded live-only codes. Four writers correctly pin response version 2. | [Protocol review](raw/05-protocol.md) |
| Invoice creation: kinds, header, seller, buyer, items, ledgers, waybill, attachments | Every current inline-XSD element is represented, including `simpleItems`, group identifiers, erasure counts and all four carrier sub-blocks. Combined preview order is unresolved below. | [Invoice requests](raw/01-invoice-requests.md) |
| Invoice storno, credit entries, proforma deletion, PDF query and reply envelope | No missing request field established. Structured amounts, PDF balance/URL and create/storno payment method are exposed. | [Invoice operations](raw/02-invoice-operations.md) |
| Receipt create, reverse, query and send | All current documented request fields and response payloads covered, including tender rows, PDF options, call IDs and present-empty email resend. | [Receipts](raw/03-receipts.md) |
| Invoice XML query | Every element in the freshly fetched `szamla.xsd` has a model destination; all current request selectors covered. | [Query/taxpayer review](raw/04-query-taxpayer.md) |
| Taxpayer query | Request complete; current NAV taxpayer business fields covered under versioned 2.0/3.0 layouts. | [Query/taxpayer review](raw/04-query-taxpayer.md) |

**Documented response information deliberately not projected:** taxpayer header/software metadata, success messages and notifications. This is a real completeness limitation (**TP-01**), but no missing business verdict/field or broken query was established. Treat it as a capability decision. Sans-I/O callers can retain `RawResponse`; bundled `Client::send` does not return it beside the typed result.

Other deliberate limits include version-2-only selection, buffered rather than streaming PDFs, selected cross-field checks rather than full XSD/business validation, refusal of empty replacing credit entries, and no automatic recovery engine. Gross-first amounts remain expressible through explicit line items. These are not counted as missing operations or bugs merely because a schema permits a broader representation.

## Official-source conflicts requiring qualification

| Topic | Conflict and current disposition |
|---|---|
| **Preview + `simpleItems`** | Current EN/HU inline schemas require template → simple items → preview. Downloadable XSD and PHP 2.12.4 write template → preview → simple items, which the crate follows. **Not live-backed; deployed acceptance and preservation of preview remain unresolved.** Obtain vendor clarification or an explicitly authorized probe. |
| Invoice layouts | API/PHP and the linked knowledge base invert the traditional/envelope-friendly descriptions for `SzlaAlap`/`SzlaNoEnv`. Preserve tokens pending rendering evidence. |
| PDF query schema | Hungarian inline XSD conflicts with EN/download on selector optionality/order and is itself malformed. Current writer follows EN/download and the documented selector alternatives. |
| Receipt downloads | Create download omits documented `torloKod`; query download omits documented `rendelesSzam`. Keep supported fields rather than treating stale downloads as exhaustive. |
| Receipt automatic MNB | General currency prose requires a rate; receipt-specific first-party PHP explicitly supports MNB with an omitted rate. Supported source-backed choice, **not a live receipt observation**. |
| Receipt order toggle / NAV reporting | Developer and linked knowledge-base pages disagree about toggle scope and current reporting rollout. No missing request flag or local implementation defect follows. |
| Published examples | Some contain unescaped URL ampersands, abbreviated base64, inconsistent amounts or sparse fields. Preserve source provenance and explicit fixture transformations; do not weaken parsers to accept illustrative placeholders. |

Exact URLs, source hashes where acquired, quotations and competing sequences are retained in the specialist reports. The site's displayed build was `v202608271632`; this does not date each rule. Cached/project-modified schemas were not treated as authoritative replacements for current sources.

## Recorded live behavior retained

The review respected `docs/szamlazz-hu-behaviour.md`, including:

- Nonunique external IDs, newest-holder queries, no external-ID echo, and attachment only on actual creation.
- Repeated invoice-storno success echoes, proforma/delivery-note same-number no-ops, and storno external ID attaching to the new SS.
- Absent reversal marker before storno, observed appearance codes, and date/appearance behavior.
- Sparse/header-free replies, comma-decimal monetary headers, paid-proforma deletion and bounded monetary rounding observations.

These are observations from the recorded TEST account/dates; the underlying exchange logs are not in the repository and were not independently reacquired. They do not establish receipt lifecycle behavior. Numbered code 56 is backed by first-party PHP, not a live observation. Conflicting official schemas likewise are not “live-tested exceptions.”

## Historical findings and rejected proposal

The September 9 report is not current status. Reinspection confirmed the relevant fixes for date suffixes, known codes, comma headers, complete-document checks, taxpayer namespace/path extraction, business-text preservation, exact cookie matching, `simpleItems`, PDF metadata, create/storno payment method, taxpayer business additions and much of the domain/recovery guidance. Each specialist report records its closure checklist.

**IR-02 was rejected:** the raw writer can emit an XML-forbidden control character, but `AgentRequest::to_wire()` rejects it with `InvalidXmlCharacter` before `Client::send` performs HTTP. The first reviewer stopped before the checked request boundary. Its raw report is retained as review history; this report and the judgment govern the disposition.

## Verification and limitations

The coordinator ran:

```text
cargo test --locked --offline -p szamlazz-agent --all-features
```

**243 passed:** 183 unit tests, 52 integration tests and 8 doctests. **4 live tests ignored.** No failures. Existing tests passing does not invalidate independently reproduced gaps; a compiling recovery example does not check its absent-marker decision.

Specialists and judge also ran focused offline/loopback probes for the findings and checked positive/negative controls. Their overlapping test counts are not added to the 243. Temporary repository probes were removed; external scratch work used `/tmp/opencode`. No production code or fixtures were modified.

Coverage is detailed source/code comparison and targeted execution, **not full automated XSD validation of every request combination**. No live receipt lifecycle, email delivery, rendering, combined preview, multi-match deletion or account-specific optional feature was exercised. Current source contradictions prevent certification against one universal schema.

The session began at `bc71841f7754dc81e1c53a77e173303240a315ff`. A concurrent commit advanced HEAD to the reviewed revision, changing only Restate harness files; the coordinator verified the agent crate, fixtures and behavior record are identical across those commits. Unrelated working-tree artifacts were preserved.

## Recommended work order

1. Correct both P2 documentation paths.
2. Repair the two VAT interpretation causes together, with separate regression cases.
3. Make numeric conversion explicit and multipart boundaries bounded.
4. Correct VAT/credit-entry/transport guidance.
5. Harden recognized response namespaces coherently across the shared parsers.
6. Resolve preview ordering and layout semantics with the vendor; decide whether a consumer needs NAV response metadata.

**Audit trail:** [independent judgment](JUDGMENT.md), [invoice requests](raw/01-invoice-requests.md), [invoice operations](raw/02-invoice-operations.md), [receipts](raw/03-receipts.md), [queries/taxpayer](raw/04-query-taxpayer.md), [protocol](raw/05-protocol.md). The judgment contains every proposal's disposition and sibling-path checks; this report supplies the consolidated outcome and final suite result.

[invoice-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[query-response]: https://docs.szamlazz.hu/agent/querying_xml/response
[delete-hu]: https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml
[vat-rules]: https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates
[xml-space]: https://www.w3.org/TR/xmlschema-2/#rf-whiteSpace
[receipt-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[mime]: https://www.rfc-editor.org/rfc/rfc2046#section-5.1.1
[envelope-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd
[vat-pdf]: https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf
[credit-hu]: https://docs.szamlazz.hu/hu/agent/credit_entry/xml
[reqwest-retry]: https://docs.rs/reqwest/0.13.4/reqwest/retry/index.html
