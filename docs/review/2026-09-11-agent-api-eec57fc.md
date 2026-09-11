# Számla Agent API conformance review — eec57fc

**Reviewed commit:** `eec57fcf3036d93cd68c9cfc017338cd3020e7dd`

**Review and public-source retrieval:** 2026-09-11

**Scope:** the complete `crates/szamlazz-agent` implementation against the current Számla Agent definitions at [docs.szamlazz.hu](https://docs.szamlazz.hu/agent/), including linked XSDs, English/Hungarian discrepancies and relevant first-party PHP/NAV sources.

## Executive conclusion

**The crate has broad, complete coverage of the documented operation fields. No confirmed defect in ordinary documented valid requests/responses was established after adjudication.** There is **one P3 malformed-response hardening finding**, two useful model/recovery improvements, and several unresolved vendor-contract questions. No P0/P1/P2 implementation defect was confirmed.

This is not an unqualified claim of complete XSD conformance: the vendor publishes mutually incompatible request schemas; the crate deliberately supports finite Decimal/civil-date domains and lenient sparse response content. The report identifies those boundaries rather than treating local tests as vendor guarantees.

### What was verified

- All **11 multipart operation actions**, authentication placement, root namespaces and response-version choices.
- All six invoice kinds; seller/buyer/ledger fields; all four carrier blocks; attachments; credit registration and explicit clearing; both invoice queries; proforma deletion; all four receipt operations; taxpayer lookup.
- Every declared request element path in each of **22 independently retained schemas** has coverage in at least one fully valid generated request for that source.
- All **134 descendant paths** of the freshly downloaded queried-invoice response schema have model destinations; no missing invoice response block was found.
- Every declared receipt response field and the NAV 2.0/3.0 taxpayer business fields/address components are covered. Taxpayer exchange metadata and notifications are deliberately not retained in the typed result.
- All **43 named error codes** have identified provenance: 30 in the general table, five receipt supplement codes, two operation-example codes, the first-party numbered-56 exception, and five recorded observed codes.

## 1. Method and evidence

Six parallel subagents independently reviewed invoices, mutations, queries, receipts, taxpayer lookup, and shared transport/errors. A seventh adjudicated the candidate findings against current code, primary sources and recorded decisions. The parent inspected the disputed source paths, reproduced the empty-verdict classification and ran the offline checks below.

The current documentation footer reported **`v202608271632`**. That is a site build label, not a date or guarantee for individual protocol statements. The detailed reports retain source URLs, quotes, schema inventories and acquisition hashes where available.

Evidence was ranked and kept explicit:

1. **Published contract:** current operation prose, XSDs and examples. Conflicting sources are identified individually.
2. **First-party implementation:** PHP 2.12.4 and NAV schemas explain ambiguities but do not prove Számla Agent execution.
3. **Recorded account observations:** `docs/szamlazz-hu-behaviour.md` and dated research records justify specific deviations, within their account/settings limits.
4. **Local execution:** tests and synthetic reproductions establish Rust behavior, not server acceptance, email delivery or accounting results.

No new vendor-account operations were performed. In particular, existing receipt probe definitions were not promoted into executed evidence. Source references below are relative to the reviewed commit; the Agent source remained unchanged during review. Concurrent Restate work advanced HEAD to `10f00b322fd9c368b8c5f5189e799a7ea741e4ac` before the final check. An explicit diff against `eec57fc` confirmed no changes to the Agent crate, Cargo.lock, reviewed request-schema corpus or schema runners. That Restate commit was outside this review.

## 2. Confirmed hardening finding

### H-01 — P3: an empty required success flag can become a settled rejection

**Location:** [`src/xml.rs:478–499`](../../crates/szamlazz-agent/src/xml.rs#L478), especially line 481; [`src/xml.rs:769–778`](../../crates/szamlazz-agent/src/xml.rs#L769); classification at [`src/error.rs:375–421`](../../crates/szamlazz-agent/src/error.rs#L375).

The [official response schema](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) requires:

```xml
<element name="sikeres" type="boolean" maxOccurs="1" minOccurs="1"/>
```

[XML Schema boolean literals](https://www.w3.org/TR/xmlschema-2/#boolean) are `true`, `false`, `1` and `0`. The shared reader nevertheless converts an empty or whitespace-only `sikeres` into `false`.

**Executed synthetic reproduction:** parse the following at HTTP 200 without error/down headers through `ClearCreditEntries::new("I-1")`:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres/>
  <hibakod>57</hibakod>
</xmlszamlavalasz>
```

| Verdict input, same code 57 | Actual result |
|---|---|
| Missing `sikeres` | `Parse` / `OutcomeClass::Unknown` |
| `sikeres=garbage` | `Parse` / `Unknown` |
| Empty `sikeres` | `Api(MalformedXml)` / **`Rejected`** |
| Explicit `sikeres=false` | `Api(MalformedXml)` / `Rejected` |

**Why it matters:** this is more than different diagnostic wording. An invalid required verdict can acquire settled-outcome confidence. The readable code is genuine evidence, but no checked source establishes its authority when the body verdict is malformed. This is shared envelope behavior, not a query-only issue.

**Severity limit:** no vendor empty-verdict emission or actual false refusal was established. Valid boolean values are parsed correctly. This is low-priority hardening, not a demonstrated failure on an ordinary valid reply. With no code, the result is `Api(Absent)` and remains Unknown.

**Recommendation:** use the existing `required_bool` for the required envelope verdict. Preserve separately readable error evidence if desired, without obtaining a false verdict through empty-text coercion. Keep the independently defined header precedence and malformed-optional-message handling intact.

Reproduction command used an isolated path-dependent scratch executable:

```sh
cargo run --offline --manifest-path /tmp/opencode/transport-eec57fc/Cargo.toml --bin adjudicate
```

## 3. Model and recovery improvements

### M-01 — preserve or document the presence of five invoice indicators

`cash_payment`, `cash_accounting`, `kata`, `kata_ledger` and `buyer.private_person` are plain booleans with missing/empty → false conversion at [`query_xml.rs:737–758`](../../crates/szamlazz-agent/src/ops/query_xml.rs#L737) and [`:886–891`](../../crates/szamlazz-agent/src/ops/query_xml.rs#L886).

The [official XML-query example](https://docs.szamlazz.hu/agent/querying_xml/response) omits **three** of them: `keszpenz`, `katafokonyv`, `privatePersonIndicator`. It explicitly supplies `penzforg=false` and `kata=true`. The [schema](https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd) requires all five without defaults.

Presence loss is confirmed: missing, empty and false become indistinguishable. However, no valid boolean is misdecoded and no actual wrong business value is established by the sparse example. The initial query review's P2 severity was therefore downgraded after independent adjudication.

**Recommendation:** document the defaults and obtain field-specific omission semantics; consider `Option<bool>` when evolving the model. Do not describe omission → false as vendor-proven. This is a model-fidelity concern, not five separate API defects.

### M-02 — retain readable receipt identity when only the PDF is corrupt

[`receipt.rs:690–701`](../../crates/szamlazz-agent/src/ops/receipt.rs#L690) decodes receipt data, then propagates malformed nonblank PDF errors before returning the receipt. Through `Client::send`, the error carries no typed partial receipt. Missing/blank PDFs already preserve the receipt with `pdf: None`.

A receipt plus an explicit artifact error could improve recovery after creation/storno. Current behavior is consistent with the Agent's strict-artifact policy and remains `Unknown`; the [official sample's `...` PDF placeholder](https://docs.szamlazz.hu/agent/generating_receipt/response) is not valid base64. **This is an optional result-model improvement, not rejection of a valid documented PDF.**

## 4. Vendor contradictions and unresolved guarantees

These are the most valuable remaining clarification targets. None should be silently resolved by treating a synthetic test or one numbered example as a universal server guarantee.

| Priority | Question / discrepancy | Current implementation and assessment |
|---|---|---|
| High | **Combined preview/simple-items order.** [EN/HU inline invoice XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml) orders `simpleItems` before `elonezetpdf`; [download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) and PHP reverse them. | Rust follows download/PHP. Both fields together cannot validate against both sources. Combined-preview acceptance and non-issuance remain unverified live. |
| High | **Stale downloadable field sets.** Invoice download lacks buyer `csoportazonosito` and item `torloKod`; receipt-create download lacks `torloKod`; receipt-query download lacks `rendelesSzam`. | Current inline definitions and relevant first-party sources support these capabilities. Retain them; schema checks report the exact conflicts rather than patching schemas. |
| High | **Numberless successful credit/PDF responses.** [Credit](https://docs.szamlazz.hu/agent/credit_entry/response) and [PDF](https://docs.szamlazz.hu/agent/querying_pdf/response) schemas make the number optional; additional headers “may” arrive. Success examples are numbered. | Rust requires a nonblank reported number in body/header. Schema-only numberless successes reproduce a parse error. Whether successes guarantee the echo is unresolved; the shared success/error schema alone cannot settle it. Registration and clearing share the issue. |
| Medium | **Template labels disagree.** [Agent template page](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template) reverses traditional/envelope-friendly labels relative to the [knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok) and PHP. | Literal tokens are covered. Confirm token-labelled rendered outputs before changing semantic names/mappings. |
| Medium | **Receipt execution evidence.** [Receipt order settings](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number) conflict with older shared-toggle prose; call-ID scope/retention, post-storno order selection and email inheritance are unspecified. | Request fields/recovery guidance are supported. Dated receipt lifecycle, automatic-MNB and email-resend execution records are still absent. |
| Medium | **Query-specific headers and NAV forwarding.** Query pages do not enumerate a complete header contract; NAV schemas allow shapes beyond the published wrapper examples. | Clarify XML/PDF query metadata, numbered-56 behavior on PDF reads, taxpayer OK-without-validity, generic NAV error forwarding and which optional NAV 3.0 fields are currently forwarded. |
| Lower | **PHP TOF `shippingID` vs XSD `shipmentID`; explicit paid false vs omission.** | Rust follows all three XSDs for `shipmentID`. `paid=false` omits `fizetve`, as PHP does. Distinct server behavior for omission/false is unestablished. |

The [Hungarian PDF-query inline XSD](https://docs.szamlazz.hu/hu/agent/querying_pdf/xml) additionally has malformed XML, requires the number despite documented alternative selectors, and changes selector order. Rust follows the agreeing EN/download sources. That broken HU source is outside the executable schema matrix and remains explicitly documented as a gap.

The existing [vendor clarification draft](../research/2026-09-11-agent-vendor-clarification.md) is **unsent; no answer received**. It is a question backlog, not evidence resolving these items.

## 5. Justified deviations and policies to retain

| Behavior | Evidence / conclusion |
|---|---|
| Storno external id belongs to the SS, rather than identifying the original | Recorded B6/XPRB observations in [behavior notes](../szamlazz-hu-behaviour.md), despite ambiguous [storno request prose](https://docs.szamlazz.hu/agent/reversing_invoice/request). Retain number-based targeting and distinct storno id. |
| Repeated invoice storno echoes the existing SS; D/SL storno is a same-number no-op | Recorded invoice observations justify qualified handling. They do not establish identical receipt-storno behavior. |
| Storno appearance is request-selected; dates need original-document care | P73/P48 records support the low-level options and guidance. Worker restrictions are a separate contract. |
| Final invoices need caller-supplied prepayment deductions | Recorded C6-2: reference/order linking does not net the totals. Keep negative-line guidance. |
| Credit clearing is explicitly supported | [September 11 execution record](../research/2026-09-11-credit-clearing-live.md) confirms populated and already-empty cases. It records parsed results, not a universal raw-channel/echo guarantee. |
| Decimal comma in monetary HTTP headers | P60 records justify comma handling; XML amounts keep their own dot/exponent grammar. |
| Numbered code 56 preserves issuance | Fresh official PHP 2.12.4 source corroborates the code-specific rule. It was not triggered in the recorded account probes; label it first-party-source evidence, not live evidence. |
| Receipt automatic MNB with omitted numeric rate | Receipt-specific PHP comments support it despite general bank-and-rate prose. No executed receipt experiment establishes it yet. |
| Response version 2, credential alternatives, open tokens | Explicit supported protocol choices. Supporting every v1 representation or imitating every PHP default is unnecessary. |
| Sparse business content and missing/blank optional PDFs | Deliberate reader policy accommodates sparse examples and preserves usable results. Malformed nonblank PDF policy remains distinct. |
| Finite exact money and civil dates | Decimal is narrower than unrestricted XSD double; Jiff dates are a finite compatibility domain. Exactness and unsupported-value refusal are explicit policies, not proof of full XSD value-space coverage. |

Historical raw September 3/6/7 probe logs are not in this repository. The summaries are useful recorded evidence, but their conclusions should remain scoped. Newly defined tests are not evidence of a vendor run.

## 6. Executed verification

| Check | Result in this review |
|---|---|
| `cargo test -p szamlazz-agent --all-features --locked` | **274 tests + 9 doctests passed**, zero failures. Ten account-dependent tests/probes and the standalone schema exporter remained ignored in this ordinary run. |
| `python3 scripts/check-agent-schemas.py` with existing Nix-store `xmllint` on PATH | **430 generated requests**, each checked against EN-inline and download definitions: **790 VALID + 70 EXPECTED-SOURCE-CONFLICT**. All 22 schemas had complete declared-element-path coverage from valid cases. **Seven negative controls** succeeded. |
| `python3 scripts/test-agent-schema-runner.py` with the same PATH | **8 tests passed**. |
| Isolated transport controls | Subagent ran 15 synthetic controls against current source; all passed. |
| Isolated query checks | Subagent compared fresh schema declarations, exercised a fully populated synthetic document, boolean presence, numeric/date boundaries, namespace checks and PDF edge cases. Field inventory is not independent response-XSD validation. |
| Parent empty-verdict reproduction | Four classification controls passed; H-01 behavior confirmed. |

The first standalone schema invocation correctly failed before validation because `xmllint` was absent from PATH. The executable already existed at `/nix/store/6xp8y3aclw6m89sy7r12sf6l98s2di0m-libxml2-2.15.3-bin/bin/xmllint`; adding its directory to the command environment enabled the successful run. No schema/tool workaround changed the input definitions.

Schema validation used the separately retained September 11 sources in `fixtures/upstream/agent/request-xsd-2026-09-11`, with provenance/checksums. Fresh web acquisition was performed independently by reviewers. The 70 conflicts are expected **validation failures against particular vendor sources**, not 70 requests proved acceptable by the live server. The matrix covers syntax, presence and order, not the Cartesian product of business rules, rendered artifacts or live account behavior.

## 7. Detailed reports and adjudication

This summary and the adjudication give the final classifications. The scoped reports retain their original candidate reasoning so severity disagreements are visible.

| Report | Coverage |
|---|---|
| [Invoices](2026-09-11-agent-api-eec57fc-invoices.md) | Complete field matrix, all invoice rules, kinds, waybills, attachments, first-party source conflicts |
| [Mutations](2026-09-11-agent-api-eec57fc-mutations.md) | Storno, credit registration/clearing, proforma deletion, recorded behavior |
| [Queries](2026-09-11-agent-api-eec57fc-queries.md) | All 134 response paths, selectors, PDF/header metadata, numeric/date domains |
| [Receipts](2026-09-11-agent-api-eec57fc-receipts.md) | All four operations, tenders, amount rules, call IDs, email, reporting/evidence limits |
| [Taxpayer](2026-09-11-agent-api-eec57fc-taxpayer.md) | Full NAV 2.0/3.0 business/address mapping, namespaces, verdicts, omitted diagnostics |
| [Transport/errors](2026-09-11-agent-api-eec57fc-transport.md) | All actions/codes, multipart, credentials, sessions, response channels, retries |
| [Independent adjudication](2026-09-11-agent-api-eec57fc-adjudication.md) | Boolean-presence downgrade, malformed-verdict confidence, PDF recovery, numberless success questions |

## Recommended follow-up order

1. Harden the required `sikeres` decoder (H-01), with controls for empty verdict plus known rejection codes.
2. Obtain vendor confirmation of effective schemas/combined preview and success-specific number guarantees.
3. Decide and document invoice-indicator presence semantics (M-01); consider partial-artifact results (M-02) as a separate API design change.
4. Record targeted receipt executions and rendered-template evidence to settle the remaining behavior questions.

**Bottom line:** the documented operation coverage is strong and the known live-backed deviations are justified. The remaining work is a small parser-hardening item plus explicit contract/evidence gaps, rather than missing operations or large unimplemented portions of the Számla Agent interface.
