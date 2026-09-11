# Számla Agent candidate adjudication — eec57fc

**Date:** 2026-09-11. **Reviewed HEAD:** `eec57fcf3036d93cd68c9cfc017338cd3020e7dd`.

## Recommendation for the final report

**These candidates establish no ordinary documented-valid-response conformance defect and no P0/P1/P2 finding.** Retain one **P3 malformed-verdict hardening finding (Q-02)**, with its outcome-confidence consequence stated accurately. Put boolean presence, receipt artifact recovery and numberless success guarantees in explicitly separate design/clarification sections.

| Candidate | Final classification | Include / exclude recommendation |
|---|---|---|
| **Q-01: five missing/empty booleans → false** | Confirmed presence-information loss; **not five confirmed P2 conformance defects**. Three omissions appear in the official query example; the other two do not. Empty values are malformed boolean input for all five. | **Exclude from confirmed conformance findings.** Retain a P3-priority model/documentation question, identifying the three example omissions individually. Preserve absence if evolving the model; do not claim a vendor false-default guarantee. |
| **Q-02: empty `sikeres` → API error** | **P3 malformed-response hardening**, shared across envelope-based operations. With a known rejected code it changes classification to **Rejected**, not merely the error's diagnostic category. Vendor emission and actual false refusal remain unestablished. | **Include once in a hardening section**, not as a query-only or normal-response defect. Recommend strict required-verdict decoding, preserving any separately useful code as diagnostic evidence if the error model permits. |
| **R-H1: corrupt PDF discards readable receipt** | Confirmed all-or-error result boundary on **malformed base64**, consistent with the Agent's existing strict-artifact policy; optional P3 recovery ergonomics. | **Exclude from conformance defect counts.** Mention an optional partial-result/artifact-diagnostic design improvement. The official `...` placeholder does not establish a valid PDF that Rust mishandles. |
| **Numberless credit success** (mutations/transport Q1) | Genuine **unresolved success-specific vendor guarantee**, with a reproducible stricter result contract. | **Include as a high-value clarification**, not a scored implementation defect. Keep the recorded interim decision; registration and explicit clearing share the question. |
| **Numberless PDF success** (queries U-02) | Same unresolved conditional-presence issue, with a read-only consequence. | **Include as a clarification**, not a demonstrated mutation/duplicate-write defect. Do not manufacture reported identity from any request selector. |

P3 here indicates low-priority hardening or design work, not a release blocker. A useful capability improvement need not be relabeled a protocol violation to be worth doing.

## Method and evidence boundary

- Independently traced current production code and fetched the primary sources below after reading the four candidate reports: `2026-09-11-agent-api-eec57fc-{queries,receipts,mutations,transport}.md` in this directory. Their findings and earlier test results were not treated as vendor evidence.
- `git diff --exit-code HEAD -- crates/szamlazz-agent` succeeded with no differences; HEAD remained the commit above. Unrelated concurrent Restate changes were present and were not adjudicated.
- Source citations below use **current exact lines**; paths beginning `src/` or `README.md` are relative to `crates/szamlazz-agent/`. `docs/` paths are workspace-relative. Live URLs identify the relevant page, heading, declaration or literal example; rendered pages have no stable source-line numbers.
- Public documentation/XSD GETs only. No credential access, account calls, source/test changes or crate-suite execution. The parent owns the suite. No synthetic executable was run by this adjudication: input/result tables below are **source-traced controls**, not claimed new execution results. Only this report was written.

### Fresh primary sources

All URLs in this register were fetched during this adjudication. The documentation site displayed `v202608271632`; this is a site build label, not a server-behavior guarantee.

| Ref | Live URL | Evidence used |
|---|---|---|
| X | [XML-query response EN](https://docs.szamlazz.hu/agent/querying_xml/response), [HU](https://docs.szamlazz.hu/hu/agent/querying_xml/response) | Full `szamla` success versus false-verdict error; exact five-field example inventory below. |
| S | [Queried `szamla` XSD](https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd) | Each of the five fields is `type="boolean" minOccurs="1" maxOccurs="1"`, with no `default` or `fixed` value. |
| A | [Adatkapcsolat outgoing invoices](https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak) | Same `szamla` shape and field annotations; payment-method explanation. Supporting shared-field semantics, not an Agent omission guarantee. |
| E | [Shared invoice envelope XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Required boolean `sikeres`; optional `szamlaszam`; optional `pdf` of `base64Binary`. |
| C | [Credit-entry response EN](https://docs.szamlazz.hu/agent/credit_entry/response), [HU](https://docs.szamlazz.hu/hu/agent/credit_entry/response) | Numbered success example; numberless failure; optional-number XSD and optional headers; v1 `DONE` versus v2 XML. |
| P | [PDF-query response](https://docs.szamlazz.hu/agent/querying_pdf/response) | v2 XML with base64 PDF; numbered success example; optional-number schema and additional headers that “may” arrive. |
| R | [Receipt-create response](https://docs.szamlazz.hu/agent/generating_receipt/response) | Success includes `nyugta`; requested PDF is base64; literal `nyugtaPdf` placeholder; error-code supplement. |
| RS | [Receipt-storno response](https://docs.szamlazz.hu/agent/reversing_receipt/response) | Storno receipt data and optional requested base64 artifact. |
| RQ | [Receipt-query response](https://docs.szamlazz.hu/agent/querying_receipt/response) | Delegates to the receipt-create response shape. |
| B | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling#error-codes-and-messages) | General code meanings, including 3 login failure, 53 missing XML file and 57 XML reading error. |
| W | [W3C XML Schema 1.0 datatypes §3.2.2.1](https://www.w3.org/TR/xmlschema-2/#boolean) | Legal boolean literals are `{true, false, 1, 0}`; empty is not a boolean literal. |

## 1. Q-01: adjudicate the five fields individually

### Current mapping and official examples

Every field below has its **own** `serde(default)` and `flexible_bool` use. Omission gets Rust's `false` default; present empty/whitespace text reaches `src/xml.rs:769–778`, whose explicit empty branch also returns false. The public fields carry no presence marker. All four valid boolean tokens map correctly through `src/xml.rs:795–799`; a nonempty unsupported token fails.

| XML path → public field | Exact current locations: public / deserialize / conversion | Agent example X, EN and HU | Annotated example A | Adjudication |
|---|---|---|---|---|
| `alap/keszpenz` → `info.cash_payment` | `src/ops/query_xml.rs:271–272 / 737–738 / 784` | **Omitted**. `fizmod=credit_card`, `fizmodunified=other`. | `true`; “true if the invoice is cash-based”; payment section says true for cash, otherwise false. | A documentation-level omission exists. False happens to be consistent with the example's card method; this example does **not** demonstrate an incorrect cash classification. “Otherwise false” describes non-cash payment, not an omitted element. General omission semantics remain unknown. |
| `alap/penzforg` → `info.cash_accounting` | `src/ops/query_xml.rs:289–291 / 753–754 / 792` | **Present, `false`**. | `true`; cash-accounting/cash-flow-based invoice annotation. | No omission or empty example established in these primary sources. Missing/empty → false is a malformed/incomplete-input policy concern, not demonstrated loss of a documented reported value. |
| `alap/kata` → `info.kata` | `src/ops/query_xml.rs:292–293 / 755–756 / 793` | **Present, `true`**. | `true`; invoice under KATA. | Same boundary as `penzforg`; the official true value is preserved. Do not cite X as omitting this field. |
| `alap/katafokonyv` → `info.kata_ledger` | `src/ops/query_xml.rs:294–295 / 757–758 / 794` | **Omitted**. | `false`; whether, in the issuer's opinion, this invoice should receive KATA accounting treatment. | Documentation-level absence is collapsed. X's `kata=true` does not prove `katafokonyv=true`: they are independent indicators, as A's true/false pair illustrates. No demonstrated wrong KATA-ledger fact, but no justified omission→false rule either. |
| `vevo/privatePersonIndicator` → `buyer.private_person` | `src/ops/query_xml.rs:394–395 / 886–891 / 909` | **Omitted**. Buyer name is illustrative, tax number empty. | `false`; customer is a private individual. Sample comment lacks `REQ`, while its XSD still says `minOccurs=1`. | Documentation-level absence is collapsed. Empty tax number/name text does not independently establish private-person status. Neither false-default semantics nor an actual misclassified buyer is proved. |

**Exactly three omissions, zero empty boolean examples, and two explicit booleans in X.** A explicitly supplies all five. All five are mandatory booleans without defaults in S. A's sample annotation/XSD discrepancy for `privatePersonIndicator` adds presence ambiguity; it supplies no false-default guarantee.

### What follows, and what does not

1. **Presence loss is real.** After a successful parse of a suitable otherwise-readable document, omitted, empty and explicitly false fields are indistinguishable in `InvoiceDocument`. Accepting an incomplete document does not logically establish its absent facts as negative. `Option<bool>` would expose information the current result loses.
2. **No conforming boolean value is misdecoded.** A missing mandatory element or an empty boolean is outside S. A parser's permissive extension is not, by itself, a failure to consume the documented valid value domain. There is no requirement here to expose an XML-presence bit for every mandatory field.
3. **The official sparse example is important but not a live capture or a clean valid instance.** It contradicts S's presence rules and includes a prose PDF placeholder, which the actual XML-query path rejects as base64 (`src/ops/query_xml.rs:604–607`). Thus “Rust successfully parsed the literal official example and fabricated facts” would be inaccurate. The omission inventory is still direct documentation evidence; repairing/removing the placeholder for a synthetic check would not establish vendor emission or the omitted fields' business values.
4. **Other optional booleans are useful model precedents, not vendor proof.** `test` and `reversed` use optional readers (`src/ops/query_xml.rs:761–764`), as does ledger continuous fulfillment (`:836–837`). Their semantics are not automatically transferable to these five. The local reversal observation concerns `sztornozott`, not any Q-01 field (`docs/szamlazz-hu-behaviour.md:82–89`).
5. **Local decisions do not settle these defaults.** ADR 0010's presence/strictness summary (`docs/adr/0010-two-szamla-models-shared-vocabulary.md:38–58`) explains why the Agent and receiver have separate parsing contracts. Its broad “agent requires what the schema requires” wording is not an exact description of today's individual fields and is not evidence that the vendor means false on omission. No field-specific false-default guarantee was found in the checked decisions or dated observations.

**Decision:** downgrade Q-01 from blanket P2 conformance finding. Retain the three omitted fields as a concrete **presence-model/documentation question**, and the empty branch/all-five omission behavior as a broader hardening/design question. If changing the public model, preserving absent/empty as `None` is reasonable, but is a deliberate API evolution, not a proven vendor-required five-field fix. At minimum document current defaults; seek field-specific omission semantics rather than extrapolating from `kata`, a payment-method string, or Adatkapcsolat's different ingestion policy.

## 2. Q-02: empty verdict, especially with a known rejected code

### Mechanism and source-traced controls

`Verdict::parse` requires the `sikeres` element but uses `flexible_bool` (`src/xml.rs:478–499`). Consequently **missing** and **present empty** behave differently. `api_error` trusts the resulting false boolean and maps the independent code (`:502–516`). `ResponseError::outcome_class` delegates an API error to its code; parse errors remain Unknown (`src/error.rs:752–769`).

For a complete expected envelope at HTTP 200 **without error/down headers**, the following results follow from those branches:

| Body facts | Current result / confidence |
|---|---|
| `sikeres` missing; body `hibakod=3` | Deserialization failure → `Parse` / **Unknown**. |
| `<sikeres>garbage</sikeres>`; body `hibakod=3` | Invalid boolean → `Parse` / **Unknown**. |
| `<sikeres/>`; no code or blank code | `Api(Absent)` / **Unknown**. Paired-empty and whitespace-only verdicts take the same branch. |
| `<sikeres/>`; `hibakod=9999` | `Api(Unknown("9999"))` / **Unknown**. |
| `<sikeres/>`; `hibakod=3`, `53` or `57` | `Api` with that known code / **Rejected**. |
| `<sikeres/>`; `hibakod=463` | `Api(PaymentOnReversedInvoice)` / **Rejected**. Code meaning has local observation evidence, not an entry in B's general table. |
| `<sikeres>false</sikeres>`; `hibakod=3` | Ordinary documented refusal → **Rejected**. |

The known-code classification is explicit at `src/error.rs:375–421`; Rejected promises “refused this request before acting” at `:438–440`. XML queries use this verdict at `src/ops/query_xml.rs:580–586`; PDF queries use `src/ops/envelope.rs:289–294,197–203`; credit entries and receipts reach `src/xml.rs:528–553` through `src/ops/credit_entry.rs:316–317` and `src/ops/receipt.rs:690–691`.

**Answer to the confidence question: yes.** Compared with rejecting the invalid boolean, an empty verdict plus a recognized rejection code changes the result from **Unknown to Rejected**. The queries report's no-code example is correct, but its low-severity explanation must not be generalized to all empty verdicts. “It never creates ordinary success” is not enough to establish conservative outcome classification.

Two qualifications matter:

- **The code is independently readable evidence, not invented text.** B describes 3/53/57 as actual failures; the credit response example uses false plus 3. A deliberate policy could salvage a known code despite a malformed verdict. But none of the fetched sources guarantees that such a code remains authoritative when the required verdict is invalid. The current behavior comes indirectly from empty→false, and does not expose that confidence decision. A false refusal after a real successful send has **not** been observed or established.
- **This is not the same as discarding a broken optional diagnostic.** Current `Verdict::parse` deliberately isolates a malformed `hibauzenet` so it cannot erase readable facts (`src/xml.rs:473–499`). Here the malformed field is itself the required verdict. Tightening that field can preserve the existing optional-diagnostic policy. An authoritative error header is also a separate case: it is checked before the body (`src/wire.rs:262–310`), so strict body-verdict decoding alone does not change header-first outcomes.

The shared issuing parser has a further bounded exception: empty verdict + body code 56 + usable number can enter numbered-notification-warning handling (`src/ops/envelope.rs:197–248`). Receipt/credit parsers do not use that exception. Therefore even “empty verdict can never accompany a successful parser result” is too broad; this does not establish that the vendor emits such a combination or that a normal PDF query performs notification delivery.

### Disposition

**Retain P3 as malformed-verdict hardening, not ordinary valid-response nonconformance.** E, C, P and R require boolean verdicts; W excludes empty. No checked live record establishes empty-verdict emission. The potential confidence escalation makes this more useful than a merely cosmetic diagnostic cleanup, but does not justify P2 operational impact without evidence of an affected exchange.

**Recommended change if taken forward:** decode envelope `sikeres` with the existing `required_bool` (`src/xml.rs:759–766`), so missing/empty/invalid body verdicts remain unparseable/Unknown. If retaining a readable code is desirable, carry it as diagnostic evidence without silently converting an invalid verdict into a settled refusal. Keep valid false/code results and independently established header handling intact. If the project instead chooses code-only salvage, document and test that explicit trust policy; do not justify it by the empty-as-false coercion or by the optional-message decision. Tests would verify the policy, not vendor guarantees. In all cases a later refusal does not settle an earlier send (`src/recovery.md:4–9`).

## 3. R-H1: corrupted receipt artifact and readable identity

**Behavior confirmed from source:** `src/ops/receipt.rs:690–701` first deserializes the receipt body, requires `nyugta`, then propagates `Pdf::from_base64` failure at line 694 before returning `Receipt`. Missing/blank PDF produces `None`. Standard base64 decoding is at `src/types.rs:104–116`. The convenience client returns only the parsed result at `src/client.rs:403–405`, so a base64 error carries no typed partial receipt. A caller retaining its own `RawResponse` still has the source body; “the document is deleted/lost everywhere” would overstate this.

R says the successful call contains receipt data and, when requested, a **base64-encoded** PDF. Its XSD uses string for `nyugtaPdf`, but that broad lexical declaration does not cancel the prose's artifact contract. The literal `<nyugtaPdf>...</nyugtaPdf>` is an abbreviation, not valid base64. RS explicitly separates storno data from its requested artifact; RQ uses the same response. None requires a client to return a partially decoded success when a supplied artifact is corrupt.

There is relevant intentional local policy: ADR 0010 explicitly says a PDF that does not decode fails the Agent document (`docs/adr/0010-two-szamla-models-shared-vocabulary.md:51–58`), unlike the receiver. That ADR directly discusses the two invoice readers, so it is **support for the general Agent policy, not a receipt-specific vendor rule or proof that writes are repeatable**. Receipt README guidance promises preservation for missing/empty PDFs (`README.md:268–271`), which current code implements; it does not promise preservation for malformed nonblank base64. Ordinary XML-query PDF decoding similarly propagates failure (`src/ops/query_xml.rs:604–607`). The numbered-56 leniency is a narrower explicit exception (`src/ops/envelope.rs:170–177,219–246`).

**Decision:** agree with the receipts report's “malformed-artifact recovery ergonomics” classification, but exclude it from verified conformance defect totals. A partial receipt plus explicit artifact error could materially improve recovery after create/storno, and is a reasonable optional P3 design improvement. Do not silently present corruption as a valid PDF. The current error stays **Unknown**, not “receipt was not issued” (`src/error.rs:752–769`); receipt recovery retains logical call identity rather than issuing anew (`src/recovery.md:15–16`). The dated September 11 record expressly says receipt probes were not executed (`docs/research/2026-09-11-credit-clearing-live.md:75`); synthetic placeholder tests are not evidence of corrupt vendor output.

## 4. Numberless successes: real contract questions, not proven incompatibilities

### Credit registration and explicit clearing

`src/ops/credit_entry.rs:316–322` requires a nonblank **reported** invoice number after accepting the verdict. `ClearCreditEntries` delegates to the same parser at `:247–249`; `InvoiceBalance.invoice_number` is nonoptional at `:256–258`. `src/ops/envelope.rs:123–133,326–329` reads the body first, then the decoded header, trimming and rejecting blank numbers. It never substitutes the request's target.

C explicitly states additional header data “may also arrive” and that `minOccurs="0"` elements “may not always be included”; HU agrees. `szamlaszam` is optional and has no nonblank string facet. Thus a true-verdict-only envelope is **schema-permitted**, yet the current parser returns `Missing("szamlaszam")` / Unknown. This is a genuine stronger result requirement, not something to dismiss because a test expects it.

However, **the schema and optionality warning cover successes and failures together**. Both EN/HU success examples carry a number; both error examples omit it. A server that always numbers successes and never numbers errors satisfies these sources. The reverse inference also fails: an illustrative numbered success cannot prove a universal guarantee. V1's numberless `xmlagentresponse=DONE` shows the mutation does not conceptually allocate a new document identity, but does not define v2's conditional fields.

Relevant local decision and evidence:

- `docs/research/2026-09-10-credit-entry-success-question.md:22–29`: explicitly keep the current result contract pending clarification; optional reported identity, if supported, must remain distinct from requested identity.
- `docs/research/2026-09-11-credit-clearing-live.md:14–26,64–74`: two actual clearing executions returned their expected numbers and full outstanding balances. Parsed output was captured; raw bodies/headers were not archived. These establish those successes and cannot establish every account/version's echo or which channel supplied it.
- `docs/research/2026-09-11-agent-vendor-clarification.md:3,18–50`: prepared question, **not sent and no answer received**. It already asks the correct both-channels/nonblank/success-specific question for additive, replacing and clearing operations.

**Decision:** retain as the highest-value clarification among these candidates. Keep the interim contract; do not count a confirmed P2 or attach a speculative duplicate-write incident. A supported numberless acknowledgement could warrant a result-model change; a parse failure is already uncertainty, not permission to repeat additive/replacing/clearing writes (`src/recovery.md:18`).

### PDF query

`src/ops/query_pdf.rs:83–93` calls `parse_issued`; `src/ops/envelope.rs:275–279` refuses an unnumbered result even when its PDF decoded. The result has both mandatory reported number and PDF (`src/ops/query_pdf.rs:39–55`). A true envelope with base64 bytes and no number in either channel reaches the missing-number error. Supplying an actual usable number header meets the identity condition; the request's number/order/external id does not.

P and E allow an omitted number structurally; P describes successful v2 responses as XML with base64 PDF and says additional headers may arrive. P's success example nevertheless has a number, its failure example does not, and the schema expresses no success-dependent requirement. The same conditional-presence ambiguity therefore applies. Numbered PDF lookups in the historical record (`docs/szamlazz-hu-behaviour.md:73–75`) do not establish a universal channel guarantee.

**Decision:** retain a separate PDF success-identity clarification, not a confirmed conformance failure. If the vendor permits numberless PDF success, a result exposing PDF with optional reported identity would be a reasonable compatibility change. This is a **read**; its immediate impact is withheld artifact/metadata and an error, not another financial mutation. No selector should be silently promoted into a vendor-reported invoice number, particularly an order or nonunique external id.

## Final synthesis

The reports are strongest when they separate **what Rust demonstrably does**, **what a valid published reply means**, and **what the vendor has actually emitted**. Apply that separation consistently:

- Q-01 establishes five coercion sites, **three official omissions**, and no demonstrated wrong business value. Remove the blanket P2 claim; retain presence semantics as design/clarification work.
- Q-02 merits one **P3 hardening entry** because an invalid empty verdict can yield a settled known-code rejection. Correct the “always conservative” implication, while acknowledging that salvaging a readable code is a possible explicit policy and that no false refusal is observed.
- R-H1 is an optional **partial-artifact recovery design**, with conservative failure under the current policy; no valid PDF incompatibility is established.
- Numberless credit and PDF results are **open success-specific guarantees**. Neither the permissive shared XSD, the numbered examples, the result type nor passing tests settles them.

**Confirmed documented-valid-response defect count for this scoped adjudication: zero. Recommended hardening finding: Q-02, P3, counted once.**
