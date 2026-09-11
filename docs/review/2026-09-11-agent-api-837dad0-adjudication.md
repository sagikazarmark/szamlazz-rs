# Számla Agent API review — independent adjudication

**Reviewed HEAD:** `837dad024300e2a202c2b6351fcba73df82a7744`

**Date:** 2026-09-11

**Standard:** implementation against docs.szamlazz.hu, allowing specifically recorded live-tested deviations.

## Decision

**The six “no confirmed defect” conclusions do not establish complete API coverage.** One actionable **P2 capability-completeness defect** remains: the checked credit-entry operation refuses the documented zero-entry replacement shape. Explicit `fizetve=false` is a second, confirmed **request-representation exclusion**, with **P3 follow-up priority**; a resulting wrong paid state is not established.

No additional operational/parser defect is established by the disputed evidence. In particular:

- **Numberless credit success:** reproducible parser restriction, unresolved success-specific contract; not a proven vendor-response bug.
- **Optional NAV validity:** success/error schema optionality does not establish a valid indeterminate-success branch. Keep this as a vendor question, never interpret absence as false.
- **Artifact metadata:** all six declared PDF-query payload fields are projected. Additional headers and preview metadata have real projection limits, but their applicability to those particular responses is not established sufficiently to assert another missing documented capability.
- **Conflicting official schemas:** remain source conflicts, not confirmed deployed-server failures. Recorded invoice probes do not settle unrelated receipt or taxpayer behavior.

Priorities below describe work to do, not observed incident severity. There is no confirmed P0/P1 finding in this adjudication. The P2 finding is a documented capability blocked before transmission, **not a claim of a live failed clearing operation**.

## Basis and scope

Read all six newly authored reports in this directory:

- `2026-09-11-agent-api-837dad0-invoices.md`
- `2026-09-11-agent-api-837dad0-transport.md`
- `2026-09-11-agent-api-837dad0-queries.md`
- `2026-09-11-agent-api-837dad0-mutations.md`
- `2026-09-11-agent-api-837dad0-receipts.md`
- `2026-09-11-agent-api-837dad0-taxpayer.md`

Independently fetched the primary sources listed below and inspected the relevant current writers, validation, parsers and projections. Code references are relative to `crates/szamlazz-agent/src/`, at the pinned HEAD. Report references use the six suffixes above. This is focused arbitration, not another full operation audit or an independent rerun of the reported test suites.

Three distinctions govern the result:

1. **A library restriction describes implementation, not vendor entitlement.** A README, a refusal test, or a custom-request escape hatch cannot establish complete built-in support for a documented operation.
2. **Schema validity is not necessarily business validity.** A shared success/error XSD must make branch-specific payload optional. Its accepting a synthetic combination does not prove the service promises that combination. Conversely, absence of live execution is not grounds to discard an uncontradicted documented request capability.
3. **Evidence is operation- and circumstance-specific.** Recorded account observations can justify the behavior they actually exercised. They cannot establish absence/false equivalence, zero-entry clearing, receipt acceptance or current NAV forwarding when those were not exercised.

## A1 — Zero-entry replacement is a real capability-completeness defect

**Priority: P2. Confirmed local restriction; documented request coverage gap.**

The current Hungarian credit-entry XML/XSD page [C1] states:

> “Ha true, a korábbi jóváírások megmaradnak; különben lecserélődnek.”

That is: if true, previous credit entries remain; otherwise they are replaced. The same page declares `kifizetes` with `minOccurs="0" maxOccurs="5"`. The independently downloaded request XSD [C2] agrees. This is a request collection with explicit replacement semantics, not an error/success union or an arbitrary combination of document-kind flags.

**Current behavior:** `RegisterCreditEntry::new("I-1")` has `additive=false` and an empty collection (`ops/credit_entry.rs:175–187`). Its `validate()` deterministically returns `RequestError::EmptyCreditEntryReplace` (`:217–223`). `AgentRequest::to_wire` calls that validation first (`wire.rs:402–408`), and `Client::send` uses `to_wire` (`client.rs:374–375`). Consequently the ordinary checked operation cannot submit:

```xml
<xmlszamlakifiz xmlns="http://www.szamlazz.hu/xmlszamlakifiz">
  <beallitasok>
    <szamlaagentkulcs>placeholder</szamlaagentkulcs>
    <szamlaszam>I-1</szamlaszam>
    <additiv>false</additiv>
    <valaszVerzio>2</valaszVerzio>
  </beallitasok>
</xmlszamlakifiz>
```

This is a source-derived example, not an executed request or a new validator run. The unchecked `write_xml` implementation can serialize it, but requiring separate transport or a custom `AgentRequest` to bypass the built-in refusal is not ordinary operation support.

**Impact:** the caller cannot request replacement by an empty set—the documented shape naturally used to clear all credit entries—through the checked built-in API. Registering a zero-valued entry or a compensating negative entry is not equivalent to an empty set.

**Evidence limit:** `docs/szamlazz-hu-behaviour.md:133,211–215` expressly says zero-entry replacement was never sent. Thus “the deployed server certainly clears all entries” remains an inference from the published contract, not an observed fact. The crate's own “would clear” rustdoc (`ops/credit_entry.rs:144–151,175–179`) is not independent confirmation. No contrary vendor requirement or recorded live deviation was found.

**Disposition:** under the requested completeness standard, do not waive this as an accepted subset merely because README documents the refusal. Support intentional empty replacement in the checked API, or explicitly carry it as an unresolved capability exclusion if the product elects to retain that policy. The constructor's accidental-empty concern can be handled in the interface without eliminating an intentional empty replacement. Vendor confirmation of the exact clearing effect is useful, but the existing local block is already established.

## A2 — Explicit false `fizetve` cannot be represented

**Priority: P3. Confirmed request-representation exclusion; business-effect difference unresolved.**

Both the current inline invoice request XSD [I1] and the download [I2] declare optional `fizetve` of type `boolean`, with no schema default or fixed value. Therefore absence, present false and present true are expressible on the published wire surface.

The model offers only `InvoiceHeader.paid: bool` (`ops/invoice.rs:178–180`); its writer emits `fizetve` only when true (`:802–804`). No value of that field can produce `<fizetve>false</fizetve>`. Unlike A1, even its ordinary unchecked writer lacks this representation.

**Important counter-evidence:** fresh inspection of the official PHP 2.12.4 source [P1], `Header/InvoiceHeader.php:393`, found the same true-only writer:

```php
if ($this->isPaid()) $data['fizetve'] = $this->isPaid();
```

The PHP input table [P2] also lists `paid` with default false. This is substantive first-party support for the chosen wrapper behavior, beyond the Rust README. It still does **not** establish that the server treats omission and explicit false identically under every account setting/payment method. No such live comparison is recorded in the reviewed evidence.

**Disposition:** record the missing wire control honestly. An omission/false/true representation would cover the published field without needing to claim a different server effect. Do not label the present implementation as proven to issue incorrectly paid invoices; equally, do not claim complete optional-boolean fidelity. Ask whether explicit false can override automatic paid treatment and, if so, under which payment methods/settings. A confirmed distinction would raise this to a functional defect.

## A3 — Numberless credit success remains an unresolved contract question

**Priority: high vendor-question priority; potential P2 compatibility impact. No confirmed operational defect.**

[C3] says extra headers “may also arrive”, marks `szamlaszam` optional and states that `minOccurs="0"` elements “may not always be included”. The restriction is real: `ops/credit_entry.rs:250–256` requires a nonblank number from body or header before constructing `InvoiceBalance`.

The mutations report's Q1 reproduction is correctly characterized: HTTP 200, no number header, and a valid `xmlszamlavalasz` containing `sikeres=true` plus `kintlevoseg=0` produces `Missing("szamlaszam")`. Direct source inspection confirms the branch; this adjudication did not rerun the scratch program.

However, [C3]'s **one response schema covers success and error**. Its success example contains the number; its error example does not. The prose expressly describes number/amount omission on errors. The generic optionality warning does not resolve whether a successful version-2 registration may omit the number from both channels. Recorded successful credit headers contain a number (`docs/szamlazz-hu-behaviour.md:145`); that observation also cannot prove a universal echo guarantee. Version 1's bare `DONE` is not a version-2 emission guarantee.

**Ruling:** uphold the current mutations report's question classification, without treating the Rust result type as authority. A full-XSD-domain implementation would accept the synthetic body, but a proven operation-contract violation needs success-specific documentation or a valid vendor response. The question is important because the operation already targets a known invoice and does not allocate a new number: a future acknowledgement type could preserve success with optional **reported** identity. It must not silently relabel the requested number as a vendor echo.

If this form is legitimate, the current parser unnecessarily loses a successful acknowledgement and returned balance. It does not itself resend; its uncertain error must not be described as an automatic duplicate-credit defect.

## A4 — Optional NAV validity does not prove a missing success variant

**Priority: P3 vendor clarification. No confirmed defect.**

Freshly fetched NAV 2.0 and 3.0 `QueryTaxpayerResponseType` declarations [N2, N3] both make `taxpayerValidity` optional and define it as whether the taxpayer exists and is valid. Számla Agent's own response page [N1] supplies three branches:

- `funcCode=OK`, `taxpayerValidity=true` and taxpayer data;
- `funcCode=OK`, `taxpayerValidity=false` for an invalid number;
- `funcCode=ERROR`, code/message, no validity.

Those examples explain a legitimate reason for schema optionality. They do not demonstrate `OK` with an intentionally absent validity value. Code `ops/taxpayer.rs:592–622` correctly permits its absence on ERROR and requires it on OK. It never maps absence to invalidity.

**Ruling:** uphold the taxpayer slice's A1 as a question. Neither the blanket XSD optionality nor README's requirement alone settles it. If the vendor defines an indeterminate successful lookup, expose that state explicitly and retain its data; do not invent `false`. Generic direct-NAV `GeneralErrorResponse` support is likewise not established by direct-NAV documentation when [N1] says this intermediary returns the taxpayer response type.

The taxpayer report's delegated NAV PDF interpretation was read as report evidence. A fresh PDF GET here succeeded, but text extraction failed because `pdftotext` is unavailable; this adjudication does not claim independent verification of its page-68 quotation. The fresh Agent examples and version-pinned NAV XSDs suffice for the narrower ruling above.

## A5 — Artifact metadata: actual projection limits, no additional proven defect

**PDF query:** [Q1]'s six declared payload fields are `szamlaszam`, `szamlanetto`, `szamlabrutto`, `kintlevoseg`, `vevoifiokurl`, `pdf`. All six reach `InvoicePdf` (`ops/query_pdf.rs:39–55,83–93`). The earlier missing outstanding/customer-URL concern is not current.

The shared parser additionally reads `document_id`, payment method and notification status (`ops/envelope.rs:223–248`); `InvoicePdf` drops those. [Q1] only says additional parameters may arrive in headers; it does not enumerate those three as promised PDF-query metadata. Creation's header table [I3] is not sufficient to impose every creation header on a read. The drop is observable if supplied, but no new PDF-specific required mapping is established. In particular, a synthetic numbered 56 on a PDF read is not evidence the vendor emits notification failures on that read.

**Preview:** `InvoicePreview` holds only `pdf` (`ops/invoice.rs:668–675`). The shared numberless branch exits before reading totals/URL/header metadata (`ops/envelope.rs:211–216`), so even supplied metadata would not survive. This is an actual projection exclusion. The absence of an issued document does not logically prove a preview cannot have calculated totals; the current type is not that proof. Nevertheless, the general issuance envelope [I3] supplies no success-specific preview metadata promise or capture. Keep preview metadata as a vendor/design question, not an established missing invoice field.

**Missing or corrupt artifacts:** returning an issued document/receipt with no optional PDF preserves identity; a dedicated PDF query requiring its PDF is also sensible. Nonblank corrupt base64 can still discard typed identity, e.g. `ops/receipt.rs:690–701`. That is a recovery-ergonomics limit. Literal `...`/`....` placeholders and raw unescaped ampersands in examples are not valid wire artifacts establishing a conforming-response failure. No new defect follows from refusing those samples.

**Raw evidence:** `Client::send` returns the typed projection (`client.rs:374–405`), not the complete successful response. A caller explicitly owning `RawResponse` can retain more. That distinction must remain visible when describing “complete field coverage”; it is not lossless response coverage. The taxpayer slice likewise correctly inventories omitted exchange/software/notification data separately from taxpayer business fields.

## Required qualifications to the slice conclusions

| Slice statement | Adjudication |
|---|---|
| Mutations opening/§3/§7/closing: all fields represented, no useful missing operation, retain writers | Too broad for the requested capability standard. A1 is blocked at validation even though its writer has the fields. A documented local restriction is not a conformance waiver. |
| Invoices coverage preamble: optional booleans preserve explicit false/true | Overbroad: `paid` is the concrete exception, acknowledged later in that report. Field-name coverage is not full representability. |
| Invoices delivery-note row: “caller template override expressly documented” | Ambiguous/overstated if read as caller control. `ops/invoice.rs:193–195,811–814` forces `SzlaFuvarlevelesAlap` and overrides the caller's field. This is a report clarification, not a newly established vendor capability defect. |
| Queries: complete six-field PDF payload mapping | Supported. Qualify it as the declared body payload, not every header/preview artifact or raw response. Do not reopen the fixed outstanding/URL finding. |
| Mutations Q1 / taxpayer A1 downgraded despite schema-valid synthetic success | Appropriate caution about success/error union optionality. Neither requires evidence of a production incident if a clear success-specific contract is obtained; none is established here. |
| Receipts business-rules table: retry guidance states “uncertain write/query combined accounting” | Incorrect if intended to imply a defined combined budget. Current `src/recovery.md:38–42` expressly says the same-request wording does **not** define exact combined accounting. The transport report states the narrower rule correctly. |
| Any aggregation of slice test totals into unique suite coverage | Unjustified: the slices overlap heavily. Their reported passes stand as their verification records, not independent vendor acceptance or a deduplicated total. |

The invoice `simpleItems`/preview order conflict was independently reconfirmed here: inline [I1] orders simple items before preview, download [I2] does the reverse. The code follows the download. Changing it just to satisfy one source would contradict another. Other reported source disagreements—HU PDF request schema, template labels, receipt request downloads, receipt order-toggle/reporting prose—remain bounded by the slices' evidence; this adjudication did not re-fetch all of them or elevate them to implementation defects.

## Prioritized vendor questions

These are questions to pursue, **not sent requests or permission for new live operations**.

1. **Successful version-2 credit echo:** must every successful `xmlszamlakifiz` reply contain a nonblank invoice number in the body or header? Is `sikeres=true` without either a complete successful acknowledgement? Obtain a complete example and the scope of the guarantee. This resolves A3's write-outcome interpretation.
2. **Empty replacement:** confirm acceptance/effect of `additiv=false` with no `kifizetes`, including on an invoice with existing entries and on one already empty. Does it clear entries, refuse, or do nothing, and what reply/IPN follows? This settles runtime semantics of A1; the typed capability exclusion is already known.
3. **Paid-state omission versus false:** are absent `fizetve` and explicit false equivalent for all document kinds, payment methods and account defaults? Can false suppress automatic paid treatment? PHP's true-only emission is relevant context, not the answer.
4. **Combined preview/simple items:** which header order is authoritative, and is the combined request guaranteed to render only a preview without issuance? Align inline/download schemas. This is more consequential than cosmetic schema drift.
5. **Taxpayer success and failures:** can OK legitimately omit validity, and what does that mean? Which NAV version and generic failure root/status/header combinations does Számla Agent currently forward? Request updated complete wrapper examples rather than extrapolating direct-NAV behavior.
6. **Artifact metadata:** which fields/headers can accompany a numberless preview and a PDF query? Are totals meaningful on previews; can code 56 appear on a PDF query; which auxiliary headers are part of the supported contract? This determines useful projection extensions.
7. **Remaining source alignment:** resolve layout labels, the HU PDF selector/schema discrepancy, receipt order-setting scope and reporting guidance, and synchronize downloadable request schemas. Preserve positively documented fields while these discrepancies are resolved.

## Primary sources independently retrieved for this adjudication

All successful retrievals below were public documentation/artifact GETs on 2026-09-11. Docs pages displayed build `v202608271632`; that is not proof of each statement's date or deployed behavior.

| ID | Source | Use |
|---|---|---|
| C1 | [HU credit XML/XSD](https://docs.szamlazz.hu/hu/agent/credit_entry/xml) | Explicit replacement semantics and 0–5 cardinality. |
| C2 | [Downloaded credit request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd) | Independent cardinality/order check. |
| C3 | [Credit response](https://docs.szamlazz.hu/agent/credit_entry/response) | Both response branches, header prose and shared optionality. |
| C4 | [PHP credit documentation](https://docs.szamlazz.hu/php/jovairas) | Wrapper inputs/defaults; not evidence of a clearing execution. |
| I1 | [HU invoice XML/XSD](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | Optional boolean `fizetve`, preview/simple-items order. |
| I2 | [Downloaded invoice request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Confirms paid field; independently conflicting header tail. |
| I3 | [Invoice response](https://docs.szamlazz.hu/agent/generating_invoice/response) | Issuance metadata/schema and example defects. |
| P1 | [Official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Downloaded/inspected in memory, never executed/extracted. `Header/InvoiceHeader.php:393` corroborates true-only paid emission; `Document/Invoice/Invoice.php:338–345` builds no credit rows for an empty collection, without proving end-to-end clearing. Paths relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`. |
| P2 | [PHP invoice documentation](https://docs.szamlazz.hu/php/szamla-generalas) | Paid wrapper default false. |
| Q1 | [PDF-query response](https://docs.szamlazz.hu/agent/querying_pdf/response) | Six declared payload fields and generic extra-header wording. |
| N1 | [Taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) | Success, invalid-number and error examples; delegated schema. |
| N2 | [NAV 2.0 XSD at `84442e64`](https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd) | Freshly parsed `QueryTaxpayerResponseType` subtree. |
| N3 | [NAV 3.0 XSD at `cc7a775d`](https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd) | Freshly parsed `QueryTaxpayerResponseType` subtree. |

No broad tests were rerun, no source/test/fixture/older-report files were edited, no credentials or live operations were used, and no further agents were started. Only this adjudication was authored. Existing recorded live observations were accepted within their stated one-TEST-account scope; their unavailable raw logs were not independently verified.

**Bottom line:** address or explicitly retain the empty-replacement exclusion as a capability gap; stop describing optional request booleans as fully represented while explicit paid false is unavailable. Preserve the distinction between those known local limits and unresolved response contracts. The evidence does not justify manufacturing further parser bugs from union optionality, generic metadata prose or conflicting/illustrative vendor sources.
