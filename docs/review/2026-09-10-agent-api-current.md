# Számla Agent — current API compliance review

**Reviewed revision:** `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`  
**Date:** 2026-09-10  
**Method:** six parallel specialist reviews against freshly fetched official documentation, followed by a seventh independent adjudication and lead review.

## Executive conclusion

**The crate covers all eleven documented Számla Agent operations. No missing documented business field or confirmed normal-response P0/P1/P2 defect was established.** Request fields, operation routing, response models, error codes and the principal operational rules are well covered at this revision.

Four **P3 robustness/evidence-preservation findings** remain. They concern interrupted transfers or malformed response content, rather than an established failure on an ordinary conforming response. Two smaller consistency/policy opportunities and several vendor-source ambiguities are recorded separately. This is an evidence-backed review, not a guarantee of every combination on the deployed service.

The earlier report at `382cf761…` is historical. Its arithmetic, refusal-erasure, URL-userinfo diagnostics, diagnostic bounds and principal documentation findings are fixed. **Interrupted-transfer header loss remains open**: the recent envelope-verdict fix addresses a different failure path.

This report is the consolidated disposition. In particular, it supersedes the transport specialist's P2 ranking of T1 and the query specialist's classification of CQ-1 as a counted defect. [Independent adjudication](2026-09-10-agent-api-current-adjudication.md) explains those decisions.

## 1. Findings

P3 means a bounded, lower-priority improvement. Confidence in reproducing behavior is distinct from evidence that szamlazz.hu emits the triggering content. No live occurrence of the malformed-response cases was established.

| ID | Priority | Finding | Impact / recommended action |
|---|---|---|---|
| **F1** | **P3** | Optional metadata can erase a readable body-only invoice number on code 56. | Preserve validated identity independently of optional payload decoding, as the verdict already is. |
| **F2** | **P3** | Interrupted body download discards already received status and headers. | Retain received evidence alongside the incomplete-transfer error; preserve uncertainty. |
| **F3** | **P3** | An empty receipt reversal element becomes definite `false`. | Refuse the empty required boolean on the receipt path, or explicitly represent unknown. |
| **F4** | **P3** | The XML completion checks do not establish full XML lexical well-formedness. | Address illegal characters and the identified syntax gaps, or accurately bound the parser guarantee. |

### F1 — Body-only identity can disappear under notification failure

**Code:** [`ops/envelope.rs:104–118`](../../crates/szamlazz-agent/src/ops/envelope.rs#L104), [203–217](../../crates/szamlazz-agent/src/ops/envelope.rs#L203). **Confidence: high in reproduction; vendor occurrence unobserved.**

This complete, correctly namespaced response, without number headers, returns API error 56 and loses the reported number:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres><hibakod>56</hibakod>
  <szamlaszam>I-2</szamlaszam>
  <szamlabrutto><bad/></szamlabrutto>
</xmlszamlavalasz>
```

Number and metadata deserialize together. The malformed optional amount causes the entire payload to be replaced with its default. A malformed scalar amount instead preserves `I-2`, as does adding a number header. Duplicate PDF elements also reproduce identity loss.

The [response schema](https://docs.szamlazz.hu/agent/generating_invoice/response) does not permit this nested amount. This is nevertheless inconsistent with the crate's explicit [numbered-56 leniency promise](../../crates/szamlazz-agent/src/ops/envelope.rs#L173). The specific number-plus-notification-failure rule is supported by [first-party PHP documentation](https://docs.szamlazz.hu/php/valasz-feldolgozas) and PHP 2.12.4 source, not a successful live code-56 probe.

**Fix boundary:** retain a unique, correctly namespaced, usable number independently; do not salvage arbitrary duplicate or malformed identity. Preserve the fixed non-56 refusal precedence. Error 56 remains uncertain and no automatic resend occurs, so the demonstrated consequence is unnecessary reconciliation, not an established duplicate-write bug. [Detailed reproduction and controls](2026-09-10-agent-api-current-mutations.md#cm1--structural-failure-in-optional-metadata-erases-a-body-only-number-on-code-56).

### F2 — Incomplete transfers lose reconciliation evidence

**Code:** [`client.rs:349–364`](../../crates/szamlazz-agent/src/client.rs#L349). **Confidence: high, independently rerun against a loopback server.**

The client collects status and headers, then `response.bytes().await?` exits on an interrupted body before constructing `RawResponse`. A server declaring 1,000 bytes but sending one byte reproduces loss of:

- credential code 3 and its message;
- `szlahu_down` diagnostics;
- numbered code 56;
- an ordinary reported invoice number.

The caller receives only `Transport` / `Unknown`. The [documented response headers](https://docs.szamlazz.hu/agent/generating_invoice/response) are useful evidence, but neither the vendor docs nor a partial transfer establishes a complete response verdict.

**Fix boundary:** expose status/header evidence with the transfer cause and an explicit incomplete-body condition. Do not substitute an empty body and run the ordinary success parser: unread body content could contradict provisional headers. The existing uncertain classification is correct. This remains P3 evidence preservation, consistent with the earlier consolidated review. [Loopback evidence](2026-09-10-agent-api-current-transport.md#t1--interrupted-transfer-still-loses-received-evidence); [priority adjudication](2026-09-10-agent-api-current-adjudication.md#1-t1--confirmed-evidence-loss-still-p3).

### F3 — Empty receipt reversal text manufactures a negative fact

**Code:** [`ops/receipt.rs:764–765`](../../crates/szamlazz-agent/src/ops/receipt.rs#L764), [`xml.rs:587–598`](../../crates/szamlazz-agent/src/xml.rs#L587). **Confidence: high; vendor occurrence unobserved.**

`<stornozott/>` in an otherwise complete receipt succeeds with `reversed=false`. Omitting the element instead fails. The [receipt schema](https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd) requires a boolean, whose valid lexical forms are `true`, `false`, `1`, `0`; empty is none of those.

This is existing permissive behavior, but no receipt-specific empty-means-false evidence was found. Unlike ordinary string normalization, it creates a definite business-state value, and the [README recovery example](../../crates/szamlazz-agent/README.md#L186) uses `!receipt.reversed`.

**Fix boundary:** make this receipt fact strict or explicitly unknown, preserving all four valid forms. Avoid changing the shared helper indiscriminately: empty-as-false on a success verdict refuses success, a different consequence. A parse failure after a write would still leave the write uncertain. [Detailed evidence](2026-09-10-agent-api-current-receipts.md#h2--empty-reversal-text-is-not-evidence-that-a-receipt-is-unreversed).

### F4 — XML completion checking has lexical gaps

**Code:** [`xml.rs:63–229`](../../crates/szamlazz-agent/src/xml.rs#L63), [`ops/query_xml.rs:588–608`](../../crates/szamlazz-agent/src/ops/query_xml.rs#L588). **Confidence: high; malformed inputs, not ordinary vendor responses.**

Offline XML-query mutations succeeded with literal NUL or `&#x1;` in `supplier.name`. Other accepted cases include `A]]>B`, undefined entities inside ignored extensions, missing attribute separators, literal `<` in an attribute, and an element name beginning with a digit. An independent XML parser refused the corresponding cases; [XML 1.0](https://www.w3.org/TR/2008/REC-xml-20081126/) disallows them.

The strongest consequence is illegal characters reaching a public business string. No downstream persistence failure or namespace/identity bypass was established. Existing EOF, namespace, truncation and trailing-root protections still pass their checks.

**Fix boundary:** lexical validation is distinct from full XSD business validation. Unknown well-formed extensions and sparse content can remain supported. If complete well-formedness is not intended, document the actual boundary rather than presenting parse success as XML certification. Taxpayer extraction has additional checks; not every specimen is established on every operation. [Reproduction matrix](2026-09-10-agent-api-current-queries.md#cq-2--whole-document-checking-is-not-complete-xml-well-formedness-checking).

## 2. Smaller policy and consistency opportunities

| Topic | Confirmed behavior | Disposition |
|---|---|---|
| Blank receipt PDF | Whitespace-only `nyugtaPdf` becomes `Some(Pdf)` with zero bytes; absent or empty becomes `None`. | Small P3 consistency improvement: normalize blank artifacts consistently. `None` already exposes a missing requested artifact, so a new mandatory error/result shape is not required. Recover the PDF by querying the known number, not by reissuing. |
| Customer URL fidelity | Shared envelope XML `vevoifiokurl` is Unicode-trimmed; decoded header URLs are not. Applies to create, storno, credit registration and PDF query. | Policy/fidelity note, not four defects. The XSD says string, but the README explicitly exempts URLs from its exact business-text policy. No broken usable vendor link was reproduced; percent-encoded boundary characters remain intact. |
| Minor prose qualifications | Some descriptions of aggregator, absent booleans, test email, credential-code sequencing and HTTP application errors could be more precise. | Documentation opportunities, with details in invoice/transport appendices; no new wire failure established. |

See [receipt artifact evidence](2026-09-10-agent-api-current-adjudication.md#5-receipt-h1--split-artifact-absence-from-blank-representation) and [URL adjudication](2026-09-10-agent-api-current-adjudication.md#3-cq-1--shared-url-fidelity-with-a-material-policy-counterargument).

## 3. Coverage

| Surface | Reviewed coverage / result |
|---|---|
| Invoice creation | All six kinds; 118 child declarations in named schema structures plus root blocks; settings, references, buyer/seller/postal/ledger, four carrier blocks, templates, preview, simplified image, erasure counts, attachments and tokens. No current field/order defect established apart from unresolved vendor-source contradictions. |
| Derived line arithmetic | Exact representability, rounding order, underflow and boundary products/sums. The previous precision-loss finding is closed: former failing cases now return documented arithmetic errors. Explicit caller amounts remain supported. |
| Invoice storno | Complete request and response; original number, dates, appearance, external id, template and email/tax overrides. Observed echoes and no-ops preserved. |
| Credit-entry registration | Settings, issuer tax number, additive/replacing semantics, bounded five-entry collection, entry fields and returned balance. Empty replacement is an explicit library boundary. |
| Proforma deletion | Number/order selectors, all-matching deletion scope, own success/error envelope and code 335. |
| XML invoice query | All 125 child declarations across 19 structures: supplier/buyer, invoice metadata, items/ledgers, financial items, labels, totals, credit entries and PDF. All three selectors. |
| PDF query | All three selectors and all six declared successful payload fields, including outstanding amount and customer URL. |
| Receipts | All four operations; full request/returned business fields, items, tenders, templates, PDF, erasure count, call identity/order rules, present-empty email resend semantics. |
| Taxpayer lookup | Eight-digit request stem, genuine NAV 2.0/3.0 expanded-name paths, every declared business/address field, validity and error handling. Optional protocol diagnostics remain a deliberate projection. |
| Shared transport | Eleven exact multipart actions, endpoint, XML file framing, credentials, cookies, versions, header encoding, redirect/deadline behavior, incomplete transfers and recovery guidance. |
| Error catalogue | All 37 numeric codes identified in current general tables, supplements and examples have named mappings. PHP-backed 56 plus five live-observed codes bring the named total to 43; unknown codes remain open. This is not a complete NAV/future-code catalogue claim. |

## 4. Justified deviations to preserve

The [behavior notes](../szamlazz-hu-behaviour.md) explicitly bound live observations to one test account and recorded dates; historical raw exchange logs are outside this repository. Reviewers checked those notes before labeling mismatches.

- **External ids are not unique** and queries return the newest holder; they attach on actual creation, not necessarily on replay. No server idempotency guarantee should be inferred.
- **Invoice storno can replay success** with the existing SS; on a proforma/delivery note it can be a success-shaped no-op. A numbered reply alone does not establish the intended reversal.
- **Issue dates, fulfillment dates and appearance** have documented account observations that qualify the prose. Queried `eszamla=1` means paper, not e-invoicing enabled.
- **Sparse responses and body-only errors** are supported. Even vendor examples omit XSD-required content; a universal strict-XSD response gate would regress useful compatibility.
- **Money** retains observed comma-decimal headers, caller-supplied values, server tolerance and independent invoice-storage rounding. Invoice observations are not receipt precision/replay evidence.
- **Current fields missing from stale downloads** remain supported where current pages and first-party code establish them: buyer group id, erasure count and receipt order selector.
- **Numbered 56** is a specific first-party implementation rule, not a successfully observed live shape. Preserve it with that provenance.
- **Receipt email resend** requires a present-empty `emailKuldes`; omitting the block has a different meaning.

Additional permissive behavior—such as widened integer metadata, selected empty defaults and legacy date forms—is library compatibility policy, not necessarily live-tested vendor behavior. Notably, an unzoned datetime can parse as a civil date; the older report's blanket datetime-rejection claim was too broad.

## 5. Vendor ambiguities and evidence gaps

| Question | Evidence and disposition |
|---|---|
| Preview plus `simpleItems` order | EN/HU inline schemas disagree with download/PHP. Current writer follows download/PHP and qualifies the conflict. Do not blindly reorder. |
| Invoice layout labels | API/PHP and linked knowledge base reverse the traditional/envelope-friendly labels for `SzlaAlap` and `SzlaNoEnv`. Tokens are supported; ask the vendor before swapping meanings. |
| HU PDF request schema | Malformed XML, required-number and selector-order contradictions versus EN/download and HU prose. Current writer follows EN/download. |
| **Numberless successful credit registration** | The version-2 combined success/error schema permits it and headers are optional in prose; the crate requires an echoed number. All fetched success examples and recorded success observations have one. Clarify the success-specific guarantee. If omission is supported, represent acknowledgement without inventing reported identity from request data. |
| Receipt order-toggle scope | Both Agent locales say independent from invoices; the linked knowledge-base article says the setting cannot be configured separately by document type. Current crate follows the Agent docs. |
| Receipt NAV-reporting readiness | Agent pages lag linked knowledge-base information. The freshly fetched Hungarian article describes September 10 rollout, retroactive coverage and account/technical-user prerequisites. No extra XML field or NAV acknowledgement is established, so no missing crate capability follows. |
| Receipt call identity | Creation duplicate refusal is documented; scope, retention, storno collisions and query-call-id meaning are not established. No call-ID-only retrieval or indefinite deduplication claim is justified. |
| Other live-only questions | Actual receipt lifecycle/rendering/email, automatic receipt MNB rate, session revocation and contradictory response channels remain unverified. |

Several vendor examples contain unescaped ampersands, placeholder PDF bytes or inconsistent totals; some schema links return 404. Reports identify substitutions used in parser controls. Those substitutions are not reconstructed live responses.

Finite exact Decimal values, civil dates, version-2-only built-ins, lack of streaming/persistent-cookie storage/automatic retry, and deliberate rejection of some risky request combinations are explicit capability boundaries. No demonstrated required business operation depends on removing them.

## 6. Verification and review artifacts

All executed selected tests and scratch reproduction assertions passed. Assertions that demonstrate a defect passing do **not** mean the defect is fixed. Test selections overlap; their counts must not be summed as unique tests.

| Review | Report / executed verification |
|---|---|
| Invoice creation/arithmetic | [Specialist report](2026-09-10-agent-api-current-invoices.md): 204 existing tests, rich requests for all six kinds, fresh schema structure comparisons and arithmetic controls. |
| Mutations/envelope | [Specialist report](2026-09-10-agent-api-current-mutations.md): 220 existing tests and six scratch groups; also linked reproductions against workspace-built artifacts. |
| XML/PDF queries | [Specialist report](2026-09-10-agent-api-current-queries.md): 71 distinct existing tests, fresh EN/HU/download comparisons and lexical/model probes. |
| Receipts | [Specialist report](2026-09-10-agent-api-current-receipts.md): 61 existing tests plus receipt/PDF/reversal controls. |
| Taxpayer | [Specialist report](2026-09-10-agent-api-current-taxpayer.md): 47 existing tests and five scratch checks against fresh Agent and version-pinned NAV sources. |
| Transport/errors | [Specialist report](2026-09-10-agent-api-current-transport.md): 210 existing tests, eight doctests and six scratch checks, including all routes/codes and broken-body loopback transfers. |
| Independent challenge | [Adjudication](2026-09-10-agent-api-current-adjudication.md): 20 existing tests, targeted reruns and independent XML/URL controls; revised severity and scope. |

The reports retain exact source URLs, source quotations/hashes, field inventories, current code locations, reproduction inputs, commands and limitations. Current docs pages report `v202608271632`; some older standalone pages report `v202606031507`. Those labels are not per-rule publication dates.

**Limits:** no live account calls, browser execution, PDF rendering, full workspace suite or full XSD-validator execution. Structural schema comparisons are not claimed as full validation. Historical account evidence was consulted, not recreated. Scratch artifacts under `/tmp/opencode` are ephemeral; material reproduction inputs are recorded in the reports.

The lead's final scoped `git diff --exit-code fbda137e79dc8f5a40016ee03cd5997ed4e0ea78 -- crates/szamlazz-agent fixtures Cargo.toml Cargo.lock docs/szamlazz-hu-behaviour.md` passed. Concurrent Restate work does not change this review's baseline. This review authored reports only.

## Recommended follow-up

1. Preserve body-only identity independently in the numbered-56 path (**F1**).
2. Design incomplete-transfer evidence retention without weakening uncertainty (**F2**).
3. Tighten the receipt-specific reversal boolean and address the identified XML lexical boundary (**F3/F4**).
4. Consider blank-PDF normalization and explicitly settle shared URL text policy.
5. Ask the vendor about numberless credit success and the source contradictions before changing otherwise supported writers.
