# Számla Agent — API conformance review

**Reviewed revision:** `f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9`

**Date:** 2026-09-10

**Method:** six parallel specialist documentation/code audits, followed by independent adjudication and lead review. Official EN/HU pages, linked schemas/examples, relevant first-party PHP/NAV sources and recorded live-test evidence were checked.

## Executive conclusion

**Implementation follow-up:** F1–F4 were subsequently addressed in the working
tree: canonical protocol row names plus overlapped-list decoding, normalized
namespace resolution with expanded-attribute checks, and independent optional
diagnostic decoding. Public-boundary regressions cover invoice/receipt lists,
namespace rules, singleton/scalar protection and diagnostic evidence. Independent
review found an attribute-quoting regression in the initial projection change;
it was fixed, regression-tested and independently confirmed closed. Final checks:
265 unit/integration tests and eight doctests passed; four live tests ignored;
strict all-target/all-feature Clippy, formatting and the all-feature WASM build
passed. The [vendor question](../research/2026-09-10-credit-entry-success-question.md)
is drafted, not submitted. The findings below describe the original pinned
revision and are retained as the review record.

**The crate implements all eleven documented Számla Agent operations, with no missing documented business field established. Four implementation findings remain: one P2 interoperability defect and three P3 parser improvements.**

The principal defect is rejection of repeated invoice/receipt rows when equivalent XML namespace prefixes differ between rows. These responses pass the official XSDs. The other findings concern an unusual valid namespace declaration, acceptance of namespace-invalid markup, and loss of otherwise readable response evidence when an optional diagnostic is malformed.

No P0/P1 defect or live-account incident was established. The request writers, routes, main response models, error catalogue and ordinary documented examples are well covered. Recorded live-supported deviations were treated as evidence, not automatically labeled noncompliance.

This is the consolidated disposition for this revision. It qualifies the receipt specialist's initial no-defect conclusion with the independently reproduced shared list defect, merges the taxpayer duplicate-attribute finding into the shared namespace issue, and treats NBSP boolean handling as normalization-policy work. Earlier reports at other revisions remain historical.

## Findings

P2 denotes a meaningful failure to consume a conforming supported response; P3 denotes a bounded lower-priority interoperability or robustness improvement. Reproduction confidence is high for all four. None has established vendor emission frequency.

| ID | Priority | Finding | Scope / consequence |
|---|---|---|---|
| **F1** | **P2** | Repeated rows depend on raw prefix spelling and adjacency. | Whole invoice/receipt parse fails for schema-valid alternate prefixes; ignored extensions between rows also break parsing. |
| **F2** | **P3** | A legal, character-reference-equivalent reserved namespace declaration is rejected. | Otherwise valid XML/PDF query responses fail. |
| **F3** | **P3** | Namespace-invalid attributes and declarations pass XML checking. | Incomplete namespace well-formedness validation, including ignored extensions. |
| **F4** | **P3** | A malformed optional error message erases readable verdict/code and numbered-56 evidence. | Unnecessary reconciliation and loss of useful identity/refusal evidence. |

### F1 — Repeated rows are grouped by prefix spelling, not expanded name

**Code:** [`xml.rs:196–255`](../../crates/szamlazz-agent/src/xml.rs#L196), [`query_xml.rs:588`](../../crates/szamlazz-agent/src/ops/query_xml.rs#L588), [`receipt.rs:688–701`](../../crates/szamlazz-agent/src/ops/receipt.rs#L688), and the corresponding vector adapters.

Take an accepted complete row, duplicate it, and change only the duplicate's opening/closing QName:

```xml
<tetelek>
  <tetel><!-- complete original row --></tetel>
  <p:tetel xmlns:p="http://www.szamlazz.hu/szamla">
    <!-- identical complete row contents -->
  </p:tetel>
</tetelek>
```

Both rows have the same expanded name. The [official invoice XSD](https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd) permits repeated `tetel`; [Namespaces in XML](https://www.w3.org/TR/xml-names/) makes prefix spelling irrelevant to element identity. Complete executed specimens pass libxml2 XSD validation, but the public parser returns `duplicate field \`tetel\``. Removing the alternate prefix makes the same data parse.

Independently confirmed on:

- Invoice `tetel`, `qutet`, `kifizetes`, and `afakulcsossz` lists.
- Receipt `tetel`, `kifizetes`, and `afakulcsossz` lists, against the [official receipt XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd).
- All three receipt record parsers: create, storno and query.

Inserting an unknown or foreign element between same-prefix rows produces the same failure. These extension specimens are well-formed rather than XSD-declared; their relevance is the [README's extension-tolerance promise](../../crates/szamlazz-agent/README.md#L253).

The shared projection preserves protocol QNames, while quick-xml 0.42.0 groups sequences using raw QName equality and stops at intervening elements. This is one shared defect, not one per list. No silent row loss or automatic reissue was demonstrated, and ordinary vendor examples use stable prefixes.

**Recommended correction:** recognize repeated rows by protocol expanded name and handle ignored children at list boundaries. Preserve duplicate-singleton refusal, parent-path isolation and refusal of children inside scalar values. Enabling `overlapped-lists` alone is not established as a complete fix.

**Evidence:** [query reproduction](2026-09-10-agent-api-f83e5fd-queries.md#fq-1--list-grouping-depends-on-prefix-spelling-and-adjacency), [independent invoice/receipt matrix](2026-09-10-agent-api-f83e5fd-adjudication.md#1-fq-1-retain-p2-expand-to-receipts-consolidate-the-manifestations).

### F2 — Equivalent reserved namespace spelling is rejected

**Code:** [`xml.rs:73–81`](../../crates/szamlazz-agent/src/xml.rs#L73), [205–213](../../crates/szamlazz-agent/src/xml.rs#L205), [258–274](../../crates/szamlazz-agent/src/xml.rs#L258).

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"
 xmlns:xml="http://www.w3.org/XML/1998/n&#97;mespace">
  <sikeres>true</sikeres><szamlaszam>I</szamlaszam><pdf>JVBERi0=</pdf>
</xmlszamlavalasz>
```

XML namespace comparison uses attribute values **after character-reference replacement**. This is a legal explicit declaration of the reserved `xml` prefix. The envelope and a corresponding complete invoice pass official-XSD validation; both query parsers reject them with `InvalidXmlPrefixBind`. A literal `namespace` spelling succeeds.

The dependency compares the raw reserved binding before the crate's normalization runs. **Correct the normalization/validation order**, preserving reserved-prefix restrictions. This is an unusual declaration spelling with no observed vendor use.

**Evidence:** [query detail](2026-09-10-agent-api-f83e5fd-queries.md#fq-2--equivalent-reserved-xml-namespace-declaration-is-rejected), [independent confirmation](2026-09-10-agent-api-f83e5fd-adjudication.md#2-fq-2-retain-p3-genuine-valid-input-rejection).

### F3 — Namespace well-formedness checks are incomplete

**Code:** [`xml.rs:96–109`](../../crates/szamlazz-agent/src/xml.rs#L96), [173–194](../../crates/szamlazz-agent/src/xml.rs#L173), [258–274](../../crates/szamlazz-agent/src/xml.rs#L258).

For example, an ignored extension may contain duplicate expanded attribute names:

```xml
<extension xmlns:a="urn:same" xmlns:b="urn:same" a:x="1" b:x="2"/>
```

The parser accepts this despite the [namespace uniqueness rule](https://www.w3.org/TR/xml-names/#uniqAttrs). Other accepted specimens bind the reserved XML/XMLNS namespace as the default namespace, or evade reserved-binding checks through character references. Unused empty prefixed bindings also pass, contrary to Namespaces 1.0; that particular rule differs in Namespaces 1.1.

Independently confirmed on invoice XML, PDF, NAV 2.0 and NAV 3.0 response paths. The attributes are ignored in these reproductions: **no business-field injection, identity spoofing or exploitation was demonstrated**. The earlier illegal-character and XML-token findings remain fixed; this is a narrower namespace-layer gap.

**Recommended correction:** validate normalized bindings and attribute expanded-name uniqueness, including ignored subtrees. Keep sparse business content and legal unknown extensions supported. Full response-XSD validation is not the remedy.

**Evidence:** [namespace matrix and taxpayer merge](2026-09-10-agent-api-f83e5fd-adjudication.md#3-fq-3-retain-p3-merge-taxpayer-duplicate-attributes).

### F4 — Optional diagnostic failure hides readable response evidence

**Code:** [`envelope.rs:292–315`](../../crates/szamlazz-agent/src/ops/envelope.rs#L292), [`xml.rs:347–355`](../../crates/szamlazz-agent/src/xml.rs#L347).

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres><hibakod>56</hibakod>
  <hibauzenet><bad/></hibauzenet><szamlaszam>I-2</szamlaszam>
</xmlszamlavalasz>
```

The public storno parser returns a mixed-content parse error without `I-2`. Adding numbered-56 headers does not rescue it. A body-only credit refusal with code 463 and the same malformed diagnostic similarly loses its typed refusal classification.

The verdict decoder couples optional `hibauzenet` to the verdict/code, so it fails before the separate identity-retention path. The nested diagnostic **violates the response schema**: this is evidence-preservation hardening, not failure on a conforming reply. Its relevance is the existing numbered-56 metadata-tolerance policy, backed by [first-party PHP response guidance](https://docs.szamlazz.hu/php/valasz-feldolgozas) and PHP 2.12.4's number-plus-code-56 handling. That special rule has not been reproduced live in the recorded account probes.

**Recommended correction:** keep unique scalar verdict/code and usable identity independent of auxiliary diagnostic decoding. Preserve complete-XML checks and conservative handling of malformed/duplicate verdicts or identity. Do not restore broad header-56 promotion of malformed XML. Current failures remain uncertain and do not automatically resend.

**Evidence:** [mutation reproduction and controls](2026-09-10-agent-api-f83e5fd-mutations.md#fm1--optional-error-message-structure-still-erases-independently-readable-verdictcode-and-numbered-56-evidence), [independent adjudication](2026-09-10-agent-api-f83e5fd-adjudication.md#4-fm1-retain-p3-as-evidence-retention-hardening).

## Vendor contradictions and contract questions

These should be resolved explicitly rather than counted as confirmed writer defects.

| Question | Evidence / disposition |
|---|---|
| **Numberless successful credit registration** | Current EN/HU response pages and XSD make `szamlaszam` optional and headers conditional; the parser requires an echoed number. A bare successful envelope validates but fails parsing. However, the schema covers both success and error, and all reviewed successful examples/observations contain a number. Ask whether version-2 success guarantees an echo in either channel. If not, represent acknowledgement with optional reported identity, distinct from the requested number. [Analysis](2026-09-10-agent-api-f83e5fd-adjudication.md#6-numberless-credit-success-genuine-contract-ambiguity-not-dismissed-or-promoted). |
| Preview plus `simpleItems` order | Current inline schemas disagree with the download and first-party PHP writer. Current implementation follows download/PHP; do not blindly reorder. |
| Invoice layout labels | API/PHP and linked knowledge base reverse the traditional/envelope-friendly descriptions of `SzlaAlap` and `SzlaNoEnv`. Tokens are supported; vendor clarification is needed before swapping meanings. |
| HU PDF request XSD | Malformed syntax and selector-order/required-number contradictions versus EN/download and HU prose. Current writer follows EN/download. |
| Receipt schemas and settings | Stale downloads omit fields supported in current pages/PHP. Agent pages and linked knowledge base disagree about independent order-number toggles; NAV-rollout information also differs. No extra missing request field follows from the rollout prose. |
| Receipt call identity and live behavior | Duplicate creation refusal is documented; scope, retention, storno collisions and call-ID-only recovery are not established. Receipt lifecycle, rendering/email and current taxpayer-field forwarding were not captured live. |

The detailed invoice, receipt and transport reports retain exact conflicting URLs and quotations. Invalid PDF placeholders, unescaped ampersands, inconsistent sample totals and broken schema links are identified as vendor-source limitations; repaired samples are not treated as live captures.

## Deliberate policies and evidenced deviations

The review consulted [the live-behavior notes](../szamlazz-hu-behaviour.md), whose observations are bounded to one TEST account and recorded dates; original exchange logs are outside the repository.

Preserve the evidenced external-id non-uniqueness/latest-holder behavior, storno repeat echoes and non-invoice no-ops, storno external-id attachment, appearance/date behavior, paid-proforma deletion, final-invoice non-netting, sparse responses/body-only errors, and comma-decimal headers. Invoice observations do not establish receipt replay or precision semantics. Historical worker-design consequences in those notes are not additional vendor guarantees.

Other choices are explicit library policies rather than necessarily live-tested deviations: finite exact Decimal values, civil dates, version-2-only built-ins, optional business-text projection, present-value validation, no automatic application retry, and selected request restrictions such as refusing empty credit replacement.

Optional follow-ups, outside the four findings:

- Align or explicitly document taxpayer-validity NBSP trimming; it accepts a broader lexical domain than `xs:boolean`, but does not misread a conforming true/false value.
- Consider evidence retention when nonblank invalid receipt PDF data prevents a typed result. Current behavior is conservative and the artifact is malformed.
- Shared customer-URL boundary trimming follows an explicit README exception; no altered usable vendor link was demonstrated.
- NAV header/software/notification metadata are deliberately unrepresented by the taxpayer business projection. Retain `RawResponse` when full evidence is needed.

## Coverage and verification

| Area | Reviewed coverage | Report / verification actually executed |
|---|---|---|
| Invoice creation | Six kinds; 118 named-structure child declarations plus root blocks; settings, buyer/seller/postal/ledger, four carriers, attachments, templates, preview, simplified image, erasure count, money/tokens. | [Invoice audit](2026-09-10-agent-api-f83e5fd-invoices.md): 233 tests, arithmetic probes and 18 conflict-isolated XSD controls. |
| Storno, credit registration, deletion | Complete fields/order/routing, references/dates/appearance, bounded five-entry collection, additive/replacing semantics, deletion scope, envelopes and errors. | [Mutation audit](2026-09-10-agent-api-f83e5fd-mutations.md): 227 tests, six scratch groups, 14 generated request variants validated against fresh XSDs. |
| XML/PDF queries | Three selectors each; all 125 queried child declarations across 19 structures; all six PDF payload fields; dates/money/text/namespaces. | [Query audit](2026-09-10-agent-api-f83e5fd-queries.md): 75 distinct tests, fresh example checks, all-fields and selector XSD validation, parser probes. |
| Receipts | All four operations and request/response fields; items, tenders, totals, identity/order, email presence semantics, templates, erasure and PDF. F1 qualifies the specialist's initial conclusion. | [Receipt audit](2026-09-10-agent-api-f83e5fd-receipts.md): 66 tests and offline matrices; independent adjudication added XSD-valid list counterexamples. |
| Taxpayer | Request stem and every NAV 2.0/3.0 taxpayer business/address field; version-specific expanded paths, validity and errors. | [Taxpayer audit](2026-09-10-agent-api-f83e5fd-taxpayer.md): 54 tests, fresh six-example check, both credential modes against EN/HU request schemas, offline field/path matrices. |
| Transport/errors | Eleven multipart actions, authentication, cookies, endpoint, status/header encodings, deadlines, incomplete transfers, WASM limitations and recovery guidance. All 37 documented numeric Agent codes reviewed have named mappings; PHP-backed 56 plus five live-observed codes give 43 named codes. | [Transport audit](2026-09-10-agent-api-f83e5fd-transport.md): 221 tests, eight doctests, six scratch checks; both WASM configurations compiled. |
| Independent challenge | Finding severity, shared receipt reach, namespace validity, diagnostic evidence, scalar policies and credit-success ambiguity. | [Adjudication](2026-09-10-agent-api-f83e5fd-adjudication.md): 40 tests, six mutation scratch groups and 78 list-variation parser calls, plus namespace/policy/XSD controls. |

All selected tests and final scratch assertions passed. **Counts overlap and must not be summed as unique tests.** Scratch assertions reproduce defects as well as successful controls; passing them does not mean findings are fixed. Reports retain source URLs, hashes, exact code locations, reproduction transformations, commands and limitations. Temporary scratch paths are not permanent repository fixtures.

### Earlier findings confirmed fixed

- Derived arithmetic's former silent precision loss.
- Numbered-56 identity loss from malformed optional totals/duplicate PDF; F4 identifies a distinct remaining diagnostic boundary.
- Interrupted-body status/header loss: `IncompleteResponse` now preserves evidence while remaining uncertain.
- Empty receipt reversal interpreted as false; blank receipt PDF represented as zero-byte `Some`.
- The previously identified illegal XML characters, undefined entities, malformed names/attributes and character-data lexical gaps; F3 is narrower namespace validation.

### Limits

No live account operations were performed. This review does not establish vendor emission frequency, every account-specific business rule, email/PDF rendering, a broad fuzz/performance guarantee, or full workspace correctness. Actual XSD validation was used where stated; invoice source conflicts required explicitly bounded controls. Official pages reported site build `v202608271632`, not a per-rule publication date. Code review and source fetching are evidence of conformance coverage, not proof of every possible deployed response.

The reviewed Agent source, workspace manifests/lock, fixtures and behavior notes were unchanged against the pinned revision at final verification. This review produced reports only.

## Recommended order of work

1. Fix **F1** once in the shared repeated-row handling, covering invoice and receipt paths.
2. Address **F2/F3** together at normalized namespace validation, with separate valid/invalid acceptance criteria.
3. Extend evidence isolation for **F4**, preserving the stricter verdict and identity boundaries.
4. Clarify numberless credit success and the vendor-source contradictions before changing those contracts.
