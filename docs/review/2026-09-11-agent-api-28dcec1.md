# Számla Agent — whole-crate API conformance review

**Reviewed:** 2026-09-11 at `28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`.

## Verdict

**Broad operational coverage, with actionable contract-completeness gaps; not an unconditional conformance pass.** All eleven operations in the main Számla Agent action table are implemented. The review found three groups of request/response representation gaps, a taxpayer diagnostic omission, and two public-documentation defects. No demonstrated live interoperability incident or P0/P1 defect was established.

The distinction matters: a reproduced mismatch with a published definition does not require a captured production failure to deserve work. Conversely, an XSD-valid synthetic response does not prove szamlazz.hu actually emits it or establish what an uncertain write did. Existing Rust documentation and passing tests do not create exemptions from the vendor contract.

Six specialist subagents reviewed the operation families and shared protocol against freshly retrieved primary sources. A seventh independently challenged their findings. This report gives the **final consolidated disposition** where specialist priorities differ. Detailed inventories, source hashes, code locations and reproductions are linked below.

## Findings

P2 means normal-priority contract-support work, **not a confirmed production incident**. P3 means lower-priority diagnostics/documentation work. Confidence refers to the demonstrated mismatch; operational occurrence is stated separately.

### F1 — P2: optional reported invoice numbers are required by response types

**Locations:** [`credit_entry.rs:316–322`](../../crates/szamlazz-agent/src/ops/credit_entry.rs#L316), clearing delegation at [247–249](../../crates/szamlazz-agent/src/ops/credit_entry.rs#L247); [`query_pdf.rs:83–92`](../../crates/szamlazz-agent/src/ops/query_pdf.rs#L83); [`envelope.rs:275–279`](../../crates/szamlazz-agent/src/ops/envelope.rs#L275), also used by storno.

The official [credit-entry](https://docs.szamlazz.hu/agent/credit_entry/response), [PDF-query](https://docs.szamlazz.hu/agent/querying_pdf/response) and [storno](https://docs.szamlazz.hu/agent/reversing_invoice/response) response definitions make `szamlaszam` optional. Their prose explicitly says optional elements may be omitted and headers may arrive. The corresponding Rust results require a nonblank reported number from either channel.

Reproduced at HTTP 200 with no number header:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
</xmlszamlavalasz>
```

- Register/clear: `Parse(Missing("szamlaszam"))`.
- Storno: the same parse failure.
- PDF query: adding a decodable `pdf` still yields that failure.

Fresh XSD checks admit these envelope shapes. The PDF control used minimal decoded bytes to isolate the number gate; it was not a rendered PDF. The number requirement is independent of artifact content.

**Impact:** a readable acknowledgement, or an otherwise available fetched artifact, cannot be represented through the typed result without extra identity. A mutation may therefore look like an unreadable answer to the caller.

**Recommendation:** preserve acknowledged credit operations and fetched PDFs with optional **reported** identity. Preserve a numberless storno acknowledgement separately from a numbered document, with reconciliation still required. Never manufacture a vendor echo from the request, weaken the numbered document's invariant, or interpret a bare storno success as verified reversal.

**Evidence limit:** no numberless vendor success was recorded. The schemas cover success and failure together, and successful examples and observed clearing replies are numbered. That leaves a success-specific guarantee worth asking the vendor about; it does not establish such a guarantee. Storno's observed success-shaped no-ops justify conservative reversal decisions, not a universal number requirement. No repetition permission follows from any of these parse failures.

See [adjudication A1–A3](2026-09-11-agent-api-28dcec1-adjudication.md), [mutations](2026-09-11-agent-api-28dcec1-mutations.md), and [queries](2026-09-11-agent-api-28dcec1-queries.md).

### F2 — P2: unreported taxpayer validity cannot be represented

**Location:** [`taxpayer.rs:594–600`](../../crates/szamlazz-agent/src/ops/taxpayer.rs#L594); public `valid: bool` at [188–190](../../crates/szamlazz-agent/src/ops/taxpayer.rs#L188).

The [Agent taxpayer response page](https://docs.szamlazz.hu/agent/querying_taxpayer/response) promises NAV's taxpayer response and links its specification. Both inspected [NAV 2.0](https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1682) and [NAV 3.0](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1566) schemas make `taxpayerValidity` optional. The linked NAV PDF's field table agrees. Rust rejects `funcCode=OK` without it in both supported namespace layouts.

**Recommendation:** represent unreported validity explicitly while retaining the successful lookup's data and diagnostics. Absence must remain distinct from false and from malformed boolean text. Applications requiring a definite verdict can gate on the decoded result.

**Evidence limit:** NAV's narrative describes true for an existing taxpayer and false for an invalid/nonexistent number; Agent examples include the boolean. Actual successful omission and its business meaning are unobserved. This is a confirmed schema-coverage gap with unresolved success semantics, not proof of an incorrectly accepted or rejected taxpayer.

See [taxpayer inventory](2026-09-11-agent-api-28dcec1-taxpayer.md) and [adjudication](2026-09-11-agent-api-28dcec1-adjudication.md).

### F3 — P2 completeness: explicit `fizetve=false` is unavailable

**Locations:** [`invoice.rs:178–180`](../../crates/szamlazz-agent/src/ops/invoice.rs#L178), [823–825](../../crates/szamlazz-agent/src/ops/invoice.rs#L823).

All three freshly inspected [EN-inline](https://docs.szamlazz.hu/agent/generating_invoice/xml), [HU-inline](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) and [downloaded](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) schemas declare an optional boolean with no default. `paid: bool` emits only true: false becomes omission. Callers cannot express all three wire states.

**Recommendation:** expose omission, explicit false and true independently, preserving omission as the default if desired. This enables a documented wire capability without guessing what omission means.

**Evidence limit:** official PHP also omits false, corroborating a client policy. No recorded execution establishes omission/false equivalence for every account and payment method, nor a cash-specific override or wrong outstanding amount. The representation gap is certain; financial effect is unproven. See [invoice C-01](2026-09-11-agent-api-28dcec1-invoices.md).

### F4 — P3: taxpayer exchange diagnostics are discarded

**Locations:** [`taxpayer.rs:188–225`](../../crates/szamlazz-agent/src/ops/taxpayer.rs#L188), recognized paths at [340–397](../../crates/szamlazz-agent/src/ops/taxpayer.rs#L340), conversion at [592–622](../../crates/szamlazz-agent/src/ops/taxpayer.rs#L592).

The [official success example](https://docs.szamlazz.hu/agent/querying_taxpayer/response) contains header correlation/version fields and software metadata. Neither is exposed. Successful result messages are discarded; declared NAV 3.0 informational notifications are also omitted. All inspected taxpayer **business/address fields** are represented.

**Impact:** `Client::send` consumers cannot retain these diagnostics from the typed result. A custom transport can retain raw XML, but that is additional consumer work rather than built-in coverage.

**Recommendation:** expose optional exchange/result metadata alongside taxpayer data. Preserve notifications without requiring their presence. Current Agent notification forwarding is unverified, whereas header/software data is shown in the Agent example. Direct NAV generic error roots are a separate, unestablished forwarding question. See [taxpayer T-1](2026-09-11-agent-api-28dcec1-taxpayer.md).

### F5 — P3: public code-56 guidance overstates the success exception

**Locations:** [`error.rs:369–372`](../../crates/szamlazz-agent/src/error.rs#L369), [`README.md:414–415`](../../crates/szamlazz-agent/README.md#L414).

The guidance says 56 surfaces as an error only without a document number and otherwise becomes success with `notification_delivery_failed`. Register/clear retain numbered 56 as an API error; PDF's shared parser does not expose that flag and still requires its artifact. Malformed identity and some HTTP-status combinations can also remain errors despite number text.

**Recommendation:** scope the statement to the issuing-response interpretation and supported shapes. Keep the operation-specific runtime behavior; an invoice number does not establish a credit mutation's success. The [official PHP guidance](https://docs.szamlazz.hu/php/valasz-feldolgozas) and source corroborate issuance with notification failure, not a universal numbered-error rule. No live 56 was established. See [adjudication A8](2026-09-11-agent-api-28dcec1-adjudication.md).

### F6 — P3: receipt evidence provenance is stale

**Locations:** [`error.rs:373–374`](../../crates/szamlazz-agent/src/error.rs#L373), [`recovery.md:57–62`](../../crates/szamlazz-agent/src/recovery.md#L57).

The documentation still groups the thirteen receipt/simplified-image additions as unobserved. The [September 11 receipt execution record](../research/2026-09-11-receipts-live.md) records **337**, directly contradicting that blanket statement. It separately records a completed-create duplicate returning **338**; 338 was already implemented and is **not one of the thirteen additions**.

**Recommendation:** retain the documentation-derived origin, add the dated 337 corroboration and separately identify the observed 338 behavior. Preserve account/date boundaries and remaining unknowns. No numeric classification change is implied.

## Vendor conflicts and unresolved contracts

These deserve clarification; competing first-party definitions cannot all be satisfied by one implementation.

| Topic | Evidence and disposition |
|---|---|
| Preview + simplified items | EN/HU inline invoice XSDs require `simpleItems` before `elonezetpdf`; download and PHP put preview first, as Rust does. Combined requests fail the inline sequence but pass the download. No combined live execution settles which the server applies or verifies non-issuance. Preserve source-labelled checks; do not claim swapping the elements is a proven fix. |
| Missing downloadable declarations | Invoice download omits `csoportazonosito`/`torloKod`; receipt-create download omits `torloKod`; receipt-query download omits `rendelesSzam`. Contemporary inline definitions/rules support them. Receipt order querying additionally has execution evidence. Removing them would lose capabilities. |
| Hungarian PDF-request XSD | Malformed XML plus conflicting number optionality and selector order. EN/download and both request descriptions support all three alternatives; current writers match those usable definitions. |
| Template labels | Agent tables and linked knowledge-base examples disagree on traditional versus envelope-friendly token labels. Current tokens are exposed correctly; actual rendered-token comparison remains open. |
| Customer-account URL header encoding | Rust form-decodes `szlahu_vevoifiokurl`, converting literal `+` to space. The source does not specify its exact outer encoding. **Correction to the specialist PHP comparison:** PHP initially uses `rawurldecode`, but its public getter then calls `urldecode` again. It therefore does not prove literal-plus preservation. Obtain wire examples distinguishing `+`, `%2B`, `%20`, `%252B`; do not copy double decoding or change every text header globally. |
| Taxpayer generic errors | Direct NAV defines generic error roots; Agent promises a taxpayer wrapper and shows its own errors inside it. Generic-root forwarding is not established, so unsupported direct-NAV roots are not counted as a confirmed Agent omission. |
| Receipt settings/reporting | Agent and knowledge-base pages disagree on order-toggle scope and reporting rollout. No missing reporting-status XML field was found. Successful receipt issuance is not reporting confirmation. |
| HTTP/session corner cases | Exact contradictory header/body/status precedence, existing-session behavior after key deletion/rotation, and repeated-header semantics lack vendor guarantees or captured experiments. Current conservative policies are inventoried, not certified as universal server rules. |

Several vendor examples contain unescaped URL ampersands, abbreviated/non-base64 PDFs, inconsistent totals or broken schema links. Rejecting their literal malformed bytes is not a valid-response defect. The deletion request has a working download under [`dijbekerodel`](https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd), freshly confirmed during follow-up; its printed example links and attempted response sibling remain broken.

## Evidence-supported behavior retained

The review checked [historical invoice observations](../szamlazz-hu-behaviour.md), [receipt executions](../research/2026-09-11-receipts-live.md) and [explicit clearing executions](../research/2026-09-11-credit-clearing-live.md) before interpreting apparent mismatches.

| Behavior | Assessment |
|---|---|
| External ids are nonunique, select the newest holder and are not echoed | Supported by historical observations and current schema. Do not introduce invoice-idempotency or echo assumptions. |
| Storno external id attaches to the reversal when original number is supplied | B6/XPRB justify the implemented meaning despite ambiguous original-reference prose. External-id-only storno remains unestablished. |
| Repeat storno echoes an existing reversal; proforma/delivery-note storno can be a no-op | B4/B5 justify distinguishing a success-shaped reply from verified reversal. |
| Storno fulfillment date/appearance can follow request/defaults despite mismatch | P48/P73 support the low-level controls and advice to derive them from the original. Vendor acceptance is not proof the chosen values are correct for that reversal. |
| Final invoices require caller-supplied negative prepayment deductions | C6 establishes no automatic netting. Current line-item representation supports the documented accounting. |
| Body-only refusals and comma monetary headers | Recorded exchanges support broader readers than a header-only or dot-only interpretation. |
| Explicit zero-entry credit clearing | Populated and already-empty invoice clearing both succeeded September 11. The separate `ClearCreditEntries` capability is implemented; accidental empty-registration protection does not remove it. |
| Receipt prefix, duplicate call, automatic MNB, number/order queries and email resend | September 11 establishes the five-character prefix reply, completed duplicate 338, omitted MNB numeric rate, matching queries and first-send/delayed empty-block resend with operator-confirmed delivery. Immediate resend was throttled; the subsequently paced full probe was not rerun. |

These are bounded account observations. Historical raw invoice logs and fresh receipt/clearing raw HTTP captures are not available in the repository. Test definitions are not execution evidence. Invoice observations do not automatically transfer to receipts. Numbered 56 is corroborated by first-party PHP source, **not** an executed live trigger.

## Scope and coverage

| Slice | Coverage | Detailed report |
|---|---|---|
| Invoice creation | Six kinds; every current inline element path; settings, header, seller/buyer, ledgers, waybill, items, attachments, templates, VAT/currency/rounding, preview and simplified items | [Invoices](2026-09-11-agent-api-28dcec1-invoices.md) |
| XML/PDF queries | Three selectors; all 125 current XML child declarations across 19 structures; all six PDF success-payload fields; namespace/scalar/artifact behavior | [Queries](2026-09-11-agent-api-28dcec1-queries.md) |
| Other invoice mutations | Storno, register/add/replace/clear credit entries, deletion by number/all matching order; request/response fields and recovery distinctions | [Mutations](2026-09-11-agent-api-28dcec1-mutations.md) |
| Receipts | All four operations, every declared request/response field, tenders, call IDs, exchange rates, templates and email presence | [Receipts](2026-09-11-agent-api-28dcec1-receipts.md) |
| Taxpayer/inventory | NAV 2/3 paths and business/address fields, diagnostics, verdicts, action inventory across 72 English Agent sitemap pages | [Taxpayer](2026-09-11-agent-api-28dcec1-taxpayer.md) |
| Common protocol | Multipart, authentication/cookies, HTTP/headers, response versions, XML integrity, incomplete transfer, all 43 named numeric mappings | [Transport](2026-09-11-agent-api-28dcec1-transport.md) |
| Independent challenge | Optional facts versus inferred guarantees, unpaid emission, diagnostic omissions, complete PHP URL path, documentation/provenance | [Adjudication](2026-09-11-agent-api-28dcec1-adjudication.md) |

**Explicit scope exclusion:** the wider documentation includes [`action-agent_ceg_mb`](https://docs.szamlazz.hu/third-party-invoicing/szamla-agent/request), delegated company-account creation/join, also exposed under `/agent/self_billing`. It is not implemented and belongs to the project's excluded third-party-invoicing surface. Thus coverage is **eleven mainstream actions**, not every action anywhere in the vendor documentation. IPN and Adatkapcsolat are separate crates/surfaces. Explicit clearing shares the credit-registration action.

Other deliberate limits: response version 2 is selected wherever available; legacy mode 1 is not exposed. Decimal and civil-date models are narrower than the full XSD mathematical domains, and readers are typed projections rather than raw archives/full validators. Gross-first calculation is caller-owned through explicit line-item amounts, which can express the documented result; lacking a second convenience constructor is not a missing invoice wire capability. Blank response-number hardening and retaining document identity alongside a corrupt optional PDF remain optional improvements.

## Verification

Executed by the parent reviewer:

```sh
cargo test -p szamlazz-agent --all-features --locked --offline
env PATH="/nix/store/6xp8y3aclw6m89sy7r12sf6l98s2di0m-libxml2-2.15.3-bin/bin:$PATH" TMPDIR="/tmp/opencode" python3 scripts/check-agent-schemas.py
```

- **286 tests passed:** 277 unit/integration tests and 9 doctests. Zero failed. Ten live/probe tests remained ignored; the separate offline schema-export test was run by the schema checker.
- **430 generated requests, 860 schema-instance checks:** 790 valid, **70 expected source conflicts**, zero unexpected results. All declared element paths occurred in a fully valid request for each of the 22 source-labelled schemas. All seven negative controls were refused as intended. The checker verifies corpus hashes and exact expected-conflict diagnostics.
- This required checker uses the recorded corpus. Separately, specialists fetched definitions anew: invoice 402 schema evaluations; receipt 678; mutations 66 after deletion-download follow-up; all 12 query request combinations; four taxpayer request checks. Additional focused response probes reproduced the findings. Counts overlap and are not a single unique-test total.
- Official pages displayed documentation build `v202608271632`. Fresh artifacts, source URLs, hashes and exact commands are in the specialist reports. Synthetic controls distinguish structure/artifact decoding from actual rendering or vendor acceptance.

No authenticated vendor requests were performed for this review. No fresh browser/wasm execution, real PDF rendering/signature validation, NAV reporting verification, combined-preview experiment or receipt concurrency/idempotency-lifetime experiment is claimed. Successful tests establish the current implementation's consistency; several intentionally assert the stricter response behavior identified above, so they do not close the conformance gaps.

Tracked Agent source, Cargo inputs, schema fixtures/checker and cited live-evidence files remained unchanged from the reviewed commit. This work adds reports only; pre-existing and concurrently authored reports were preserved.

## Recommended order of work

1. Separate acknowledgements/artifacts from optional reported identity; represent unreported taxpayer validity without inventing facts. Resolve F1/F2 against the published contract while pursuing success-specific vendor guarantees.
2. Expose tri-state paid emission for F3 without silently changing existing default requests.
3. Correct F5/F6's public guidance and preserve taxpayer diagnostics for F4.
4. Obtain authoritative answers for conflicting schemas and customer-URL encoding; retain source-labelled validation and bounded live evidence.

**Bottom line:** the crate implements the mainstream operational and business-data surface thoroughly. Optional facts, explicit unpaid emission and diagnostic preservation still prevent a claim of complete API support. The remaining schema conflicts and unverified server guarantees should stay visible rather than being treated as either proven bugs or evidence-backed exemptions.
