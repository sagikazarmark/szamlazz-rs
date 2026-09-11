# Reviewer C — second-round capability audit

**2026-09-09 · independent review · code `a804c740eb8446211c1cdca3eea4fb93d298d25d`**

## Main verdicts

Retain **C1 as a missing request capability (P2)** and **C2/C3 as useful response-exposure enhancements (P3)**. C2/C3 are not proof that the existing SDK violates its advertised contract or fails on ordinary replies. Nevertheless, the best implementation is to expose the identified business data, rather than cement these particular omissions as the SDK boundary.

| Finding | Decisive recommendation | Existence confidence | Solution confidence | Live-emission evidence |
|---|---|---|---|---|
| **C1 · P2** | Add `InvoiceHeader::simple_items: Option<bool>`, default `None`; emit **after `elonezetpdf`**, following current first-party PHP and the downloadable XSD. | **High:** explicit per-document feature, no existing request representation. | **Medium overall:** High for field/default; Medium for combined-preview order because inline documentation contradicts executable first-party source. | None in the repository's account probes; no new call. PHP emission is source evidence, not server acceptance. |
| **C2 · P3** | Extend `InvoicePdf` with outstanding amount, customer URL, document id, payment method and the already-interpreted notification-warning flag; preserve its required PDF. | **High** for the two schema-field omissions and observed local discard. Auxiliary omissions are confirmed, but are not three additional proven protocol defects. | **High:** existing extraction/projection patterns suffice; field presence remains optional. | PDF-specific emission of these optional fields is unverified. Create/storno/credit headers have recorded live evidence; that does not establish PDF emission. |
| **C3 · P3** | Add all five business fields, including an open incorporation token and **lossless textual `info_date`**, with no assumed timezone or freshness policy. | **High** for the omitted documented data; not a mandatory full-NAV-consumption requirement. | **High:** optional, pass-through data avoids both invented values and a narrower datetime parser. | `infoDate` occurs in the current Számla Agent page's dated 2020 example. That is documented example evidence, not a fresh live lookup. Current forwarding of all five is unverified. |

Confidence in existence means the capability/coverage gap exists, not that an incident has occurred. P2/P3 are implementation priorities; no P0/P1 is justified. This report owns C1–C3, including the adjacent auxiliary PDF metadata question. F2/F3 and R1/R2 remain separately owned work.

## Evidence and independent checks

Read [REVIEW](../REVIEW.md), [ADJUDICATION](../ADJUDICATION.md), and raw [01](../raw/01-invoice-requests.md), [02](../raw/02-query-responses.md), [04](../raw/04-other-operations.md), then inspected actual request, envelope, PDF and taxpayer code. Also checked [ADR 0008](../../../adr/0008-agent-request-types-are-plain-data.md), the recorded [account probes](../../../szamlazz-hu-behaviour.md), the worker's explicit projections and the CLI download boundary. Code references below are relative to `crates/szamlazz-agent/src/` unless qualified.

Public sources fetched again during this round:

| Ref | First-party source | What it establishes |
|---|---|---|
| S1 | [Tour-operator `simpleItems`](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Exact field, optionality, defaults, account prerequisites, item rules and inheritance. |
| S2 | [English inline invoice XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml), [Hungarian inline](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | `szamlaSablon → simpleItems → elonezetpdf`; group id and erasure-code fields present. |
| S3 | [Downloadable invoice XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd), [sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | `szamlaSablon → elonezetpdf → simpleItems`; sending guidance explicitly identifies this download as the processing schema. Download omits two supported fields. |
| S4 | [PHP package page](https://docs.szamlazz.hu/php/), [official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Concrete first-party writer order and invoice-header metadata extraction. Published package dated 2026-08-12. |
| S5 | [PDF-query response](https://docs.szamlazz.hu/agent/querying_pdf/response), [create response/header vocabulary](https://docs.szamlazz.hu/agent/generating_invoice/response) | PDF reply's optional balance/URL; common header names and meanings. |
| S6 | [Számla Agent taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) | NAV response contract, actual `infoDate` example, link to NAV §1.8.9. |
| S7 | [NAV invoiceApi.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1552-L1581), [business fields](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1846-L1889), [invoiceBase.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd) | Exact paths/types, incorporation meanings, county code; unrestricted `xs:dateTime` versus `InvoiceTimestampType`. Fresh `master` resolves to the cited commit. |
| S8 | [NAV specification v3.0](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf), §1.8.9.2, printed pp.66–69 | Last-change meaning; VAT-group membership; client discretion in using returned data. |
| S9 | [W3C XSD 1.0 dateTime](https://www.w3.org/TR/xmlschema-2/#dateTime), §§3.2.7.1–4 | Optional timezone, offsets, fractional seconds, distinct zoned/unzoned values. |

Fresh inline XSD extraction reproduced the first review's hashes: English `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4`, Hungarian `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a`; download `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498`. PHP ZIP SHA-256: `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Fresh NAV PDF hash `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` matches the previously extracted text read for §1.8.9.2. Current Számla Agent pages report build `v202608271632`.

Temporary independent probes, all under `/tmp/opencode/reviewer-c-capabilities/`:

```sh
cargo run --offline --quiet --manifest-path /tmp/opencode/reviewer-c-capabilities/Cargo.toml
python3 /tmp/opencode/reviewer-c-capabilities/schema_check.py
```

Results:

- A numbered PDF success with body outstanding/URL and header id/payment method returns only number/net/gross/PDF. A synthetic numbered code-56 reply with PDF is accepted and its warning disappears at the same projection.
- A NAV 3.0-shaped reply using common/base namespaces, leading-zero ids and all five business fields parses; all five are discarded. Existing prefix/name/VAT code survive.
- Fresh XSDs compiled with system libxml2. Minimal request with `simpleItems` alone validates against all three. With both flags, **only inline order validates against inline; only PHP/download order validates against download**. Expected rejection diagnostics are part of the successful conflict reproduction.

These are offline parser and schema checks, not live issuance, preview, lookup or PDF-rendering evidence. No PHP runtime was available; its writer and recursive serializer were traced in source, not executed. No production edit, account access or agent delegation occurred. Existing concurrent changes outside the reviewed crate were left untouched. No full workspace test run is claimed.

## C1 — expose the actual per-document switch

### Is this genuinely missing?

**Yes.** `InvoiceHeader` (`ops/invoice.rs:135–218`) has template and preview but no `simpleItems`; its writer (`758–768`) cannot emit it. S1 explicitly distinguishes the Agent's per-document switch from the UI's account setting. An arbitrary `InvoiceTemplate::Other` cannot represent a separate boolean element. A custom `AgentRequest` can emit it but replaces the built-in operation, rather than demonstrating built-in coverage.

This is a new feature request, not proof that existing ordinary invoice serialization is wrong. Raw 01's unknown-JSON-field demonstration adds little: official XML camelCase is not the Rust JSON contract. Do not turn C1 into a request-wide `deny_unknown_fields` redesign.

### Best implementation

Add to `InvoiceHeader`:

```rust
#[doc(alias = "simpleItems")]
pub simple_items: Option<bool>,
```

- `InvoiceHeader::new` sets `None`; missing/null JSON reads as `None`. JSON name is `simple_items`, consistent with surrounding snake_case fields.
- `None` omits the XML element; `Some(false)` emits `<simpleItems>false</simpleItems>`; `Some(true)` emits the exact-case true element. Do not conflate omitted and explicitly false in the public representation, even though S1 describes the same default-mode result for independently selectable kinds.
- This controls the **invoice image**, not the amount data sent to NAV, the invoice's paper/electronic appearance, or a new `InvoiceKind`. Keep all existing monetary item fields. Do not replace this with a special template enum variant or alter the selected template locally.

### Serializer-order decision: PHP/download order wins this particular dispute

Choose one deterministic tail:

```text
szamlaSablon? → elonezetpdf? → simpleItems?
```

**New evidence relative to round one:** S4 `szamlaagent/src/szamlaagent/Header/InvoiceHeader.php:398–405` appends template, preview, then simple items. `SzamlaAgentRequest.php:259–280` recursively emits the associative array in iteration order, without sorting. The same writer permits both flags. This is concrete implementation corroboration for the schema S3 that the sending guide identifies as the processing schema. It outweighs the contradictory inline order for the implementation choice; it does not prove which orders the live server accepts.

Use an explicitly annotated maintained test schema: retain current inline field coverage, but place this tail in PHP/download order. Record the exact sources and the one ordering correction in fixture provenance. Do not represent the result as an unmodified vendor XSD or replace the entire schema with the download: the latter loses `csoportazonosito` and `torloKod`. Keep the source-difference evidence independently of the maintained fixture so a locally edited golden cannot masquerade as vendor agreement.

No runtime order flag, schema fetch, alternative-order retry or combination guard. In particular, never drop `elonezetpdf=true` to get an issuing request past a schema conflict. An offline test must prove both flags are emitted in the chosen order. Requests with `simple_items=None` retain their former XML bytes.

### Document the rules; do not implement an account model

State S1's rules on the field/module with a link:

- The vendor limits lawful use to tour operators supplying travel services under VAT Act §210/A; exposing the switch does not establish the caller's eligibility.
- Independently selectable on invoice, proforma (including its conversion to invoice), and prepayment.
- Final inherits its prepayment's setting; a request value is **not an override**. Storno inherits the original, and its separate request schema gets no new switch.
- Corrective/delivery-note simplified image is prohibited (556); a simplified original cannot be corrected even when the corrective omits the flag (554).
- Account OSS must be off and seller tax number Hungarian (551); normally at most two items (552), final exception up to four, described as two negative plus two new items.
- Allowed VAT tokens are `0`, `5`, `18`, `27`, `TAM`, `AAM`, `K.AFA`, `F.AFA` (553); final VAT rates must match the prepayment (555). Template is overridden by the server's simplified view. `K.AFA` needs the statutory wording supplied in the invoice comment, per S1.

**Add no new local business guards in this change**, including duplicated item-count/VAT/kind validation. Even the locally inspectable subset would be incomplete for inherited finals and corrections of originals. Preserve the low-level request as data, serialize the caller's option on all creation kinds, and let the server return the actual refusal. Do not derive OSS status from `eu_vat`, infer eligibility from buyer tax data, pre-query originals, or store a remembered setting. PHP's local corrective/delivery-note check is corroboration of its own policy, not a requirement to copy that policy. Complete 551–556 classification under F2 separately.

**Precise outstanding vendor question:** “When both `fejlec/elonezetpdf` and `fejlec/simpleItems` are present, which order does `action-xmlagentxmlfile` accept: the current inline XSD's simpleItems-first order, PHP 2.12.4/download's preview-first order, or both? Please align the published schemas and confirm preview still creates no document for this combination.” Do not block safe field/default/serialization work on the answer; qualify combined server interoperability until answered. The simpleItems-only subsequence is undisputed by all three schemas.

### Compatibility and acceptance

Adding a field to this exhaustive public request struct **is Rust source-breaking** under ADR 0008: complete literals and complete destructures need the new field or `..`. Constructor/functional-update callers continue to compile. Schedule for the next breaking 0.x minor release. Old JSON input still decodes; serialized JSON gains the optional member under the existing serde convention. Do not add `#[non_exhaustive]` to avoid the acknowledged break.

Required offline acceptance: external-consumer absent/false/true construction and serde cases; unchanged default XML; both-preview-values combined with both simple-items values; chosen schema order with full existing group-id/erasure fields; all six creation kinds retain full items and do not acquire account-dependent guards. The existing worker builder uses `..InvoiceHeader::new` (`restate-szamlazz/src/gateway/build.rs:138–161`), so it naturally gets `None`. Exposing a worker option or account default is a separate product decision.

## C2 — complete the PDF result without weakening its useful invariant

### Challenge to the finding

`InvoicePdf` promises “a fetched invoice PDF with the totals reported alongside it,” not a lossless envelope. A PDF-centric projection is legitimate; the existing four fields work. The shared schema may describe values that this read does not currently emit. Those facts justify **P3 enhancement**, not a mandatory parser-fix or live-outage claim.

However, `parse_issued` already pays the extraction cost and can fail parsing `outstanding` before `QueryInvoicePdf::parse` drops it (`ops/envelope.rs:205–229`; `ops/query_pdf.rs:75–83`). Exposing the amount and URL costs two optional fields and avoids another custom parser/read for documented business data. An XSD's body/header distinction alone is not a good reason to discard an already-read document id. The best bounded result is a PDF plus its recognized document metadata.

### Exact public surface

Keep existing `invoice_number: InvoiceNumber`, `net_total/gross_total: Option<Decimal>`, **`pdf: Pdf`**, and add:

| Field | Type/default | Source and policy |
|---|---|---|
| `outstanding` | `Option<Decimal>` / `None` | Move existing parsed `kintlevoseg`, body before `szlahu_kintlevoseg`. Amount as reported, not recomputed or clamped. Missing is not zero. |
| `customer_account_url` | `Option<String>` / `None` | Move existing body `vevoifiokurl`, else decoded `szlahu_vevoifiokurl`. Keep URL opaque; no network access, scheme/host validation, account-enabled inference or synthesized URL. Do not percent-decode an XML-body URL. |
| `document_id` | `Option<i64>` / `None` | Move existing `szlahu_id` extraction. Same lenient auxiliary policy as `CreatedInvoice`: absent/blank/malformed/negative/out-of-range becomes `None`. Not an account id or ownership proof. |
| `payment_method` | `Option<PaymentMethod>` / `None` | Header-only `szlahu_fizetesmod`, using the same decoded, open-token reading as credit-entry `header_payment_method` (`ops/credit_entry.rs:281–286`). Extract that private helper to a shared home; no invented body element. Unknown text becomes `Other`. |
| `notification_delivery_failed` | `bool` / `false`, explicit `#[serde(default)]` | Move the flag **already computed** by the shared parser. It means this reply was accepted with numbered code 56; false means no such warning was reported, not proof of mail delivery. |

The last three are a deliberate enhancement boundary, not additional mandatory XSD coverage findings. Document id has first-party PHP support (`InvoiceResponse.php:132–134`) and live cross-operation evidence. Payment method has a documented common header and an existing crate reader, but no established PDF-specific emission. Notification failure is not a newly asserted PDF-query capability: **the current parser already accepts it synthetically**, and preserving that fact is better than reporting an indistinguishable ordinary success. Do not change error acceptance while fixing projection or claim the read sends a notification.

Use explicit `#[serde(default)]` on each new member for clarity; preserve current serialization convention for optional fields. Add the payment method on `InvoicePdf` directly from the shared header helper; broadening `CreatedInvoice`/`InvoiceBalance` is unnecessary for this task. No public `CreatedInvoice` nesting, alias, deref or replacement: those would weaken `pdf` to optional or expose issuance-only helper semantics such as `reverses` on a read. No raw-response bag or new client method is needed to expose this finite set.

### Compatibility, tests, and unresolved emission

`InvoicePdf` is already `#[non_exhaustive]`; additive members preserve normal downstream Rust use. Old JSON deserializes with absent optionals and false warning; emitted JSON grows. Strict external JSON validators/goldens may need updating, which must be noted in release notes. Keep missing/invalid PDF and missing number behavior unchanged. Monetary-header F3 is a separate fix in the shared parser; do not bypass it by silently defaulting balances.

Acceptance: body/header/absent cases, conflicting body/header precedence, signed and zero outstanding, encoded header versus literal XML URL, valid/invalid auxiliary id, unknown payment method, numbered code 56 with a valid PDF preserving the warning, 56 without number remaining an error, and old JSON decoding. Assert an ordinary success has `notification_delivery_failed=false` without describing it as delivery confirmation.

**Vendor question:** “Which of `kintlevoseg`, `vevoifiokurl`, `szlahu_id`, `szlahu_fizetesmod` and the corresponding monetary/URL headers does `action-szamla_agent_pdf` currently emit, under what conditions, and can it return 56?” The currently implementable part is to preserve any supplied values with the documented/common meanings and optionality. Do not make their availability a promise. Existing live rows at `docs/szamlazz-hu-behaviour.md:133,144–145` concern other operations; actual PDF download observations at `64–66` establish download/selector behavior only.

## C3 — complete the taxpayer business record, preserve time information

### Challenge to the finding

NAV §1.8.9.2(5) explicitly leaves the extent of client-side use discretionary. `TaxpayerResponse` even describes its intentional reduction (`ops/taxpayer.rs:187–199`); public rustdoc promises registered name/addresses. Therefore **not a conformance violation merely because fields are omitted**. The narrower worker projection is particularly defensible.

For the general Számla Agent SDK, though, county code completes a returned tax-number component set; VAT group and economic type can inform a caller's invoicing; short name and last-change text are already defined business data. Their five-field footprint is small, and none needs a full NAV transport/software model. Selectively exposing these five is the best SDK boundary. S6's example alone already proves `infoDate` belongs in the supported 2.0-era response vocabulary.

### Exact fields, paths, defaults

Add the following to `TaxpayerInfo`; all are `#[serde(default)]`, missing/null JSON and absent/blank XML elements become `None`, and there is no new required business field even where NAV 3.0's XSD requires one inside `taxpayerData`:

| Public field | Type | Declared path under `QueryTaxpayerResponse` | Meaning |
|---|---|---|---|
| `short_name` | `Option<String>` | `taxpayerData/taxpayerShortName` | NAV's returned shortened name; do not generate it from `name`. |
| `county_code` | `Option<String>` | `taxpayerData/taxNumberDetail/countyCode` | Two-digit county-code component of the tax number, not an address region or a number to parse as an integer. Preserve leading zeroes. |
| `vat_group_membership` | `Option<String>` | `taxpayerData/vatGroupMembership` | Returned eight-digit VAT-group identifier when the taxpayer is a member; not a boolean, member list or full group tax number. |
| `incorporation` | `Option<Incorporation>` | `taxpayerData/incorporation` | NAV's economic-type token; missing is unknown, not Organization. |
| `info_date` | `Option<String>` | **root child** `infoDate` | Source text of NAV's last-change dateTime, not the response header timestamp. |

Place `Incorporation` in `ops::taxpayer`, using the crate's open-token implementation pattern, `#[non_exhaustive]`, serde as the wire string, and `#[doc(alias = "IncorporationType")]`:

```text
Organization  ↔ ORGANIZATION   — gazdasági társaság (business company)
SelfEmployed  ↔ SELF_EMPLOYED  — egyéni vállalkozó
TaxablePerson ↔ TAXABLE_PERSON — adószámos magánszemély (private person with a tax number)
Other(String)                 — preserve an unrecognized token
```

Do not expand `ORGANIZATION` into a stronger legal-personhood guarantee or interpret `TAXABLE_PERSON` as the ordinary-language category of every VAT taxpayer. Do not derive `Buyer::taxpayer_status`, choose a VAT rate, or copy a VAT-group id into another request field automatically.

Keep existing `tax_number` meaning **eight-digit taxpayerId**; no rename or replacement with `NNNNNNNN-N-NN`. A caller can format that full number only when all three reported components are present. No county derived from addresses, group derived from VAT code, or fallback to the requested prefix if the response omits its id. Keep new response ids as strings rather than applying request-side `TaxpayerPrefix` validation to previously accepted responses.

### Why `info_date: Option<String>` is the best immediate type

This is a deliberate **source-text projection**, not an assertion that an arbitrary string is a valid datetime. No new generic temporal type is needed for one optional advisory field the SDK performs no arithmetic on. Preserve the decoded nonblank element text (including its offset spelling and fractional precision); use XML whitespace only for the blank check, and do not truncate, canonicalize or convert it. Invalid nonblank temporal text also survives as text and does not newly reject an otherwise usable lookup. State this explicitly in its rustdoc. Coordinate nonempty-string fidelity with R2 instead of routing the new field through the current unconditional trim.

The reasons are source-derived:

1. S7 declares `infoDate` as **`xs:dateTime`**, not `base:InvoiceTimestampType`. The latter has a UTC-`Z` pattern and a 2010 lower bound. Neither restriction applies here; S6's real example is **`2004-12-26T23:00:00.000Z`**.
2. S8 calls this the last change of the taxpayer's data. It is not query time, incorporation date, a guaranteed effective-from date, cache insertion time or cache expiry. The scope/granularity of “data changed” is not further specified by the cited text.
3. XSD dateTime admits no suffix, `Z`, or a numeric `±hh:mm` offset. A missing timezone is **unspecified**, not UTC, Europe/Budapest or the host timezone. A numeric offset identifies an offset, not an IANA timezone/DST rule. XSD 1.0 treats `Z`, `+00:00`, `-00:00` as the same zero offset; do not import RFC 3339's special unknown-offset interpretation for `-00:00` into this field.
4. `jiff::civil::Date` loses the time; civil `DateTime` loses the offset; an instant-only `Timestamp` cannot represent an unspecified zone. Blindly parsing with one of them would recreate F1's “optional new metadata makes the whole read fail” problem. Raw source text also preserves legal fractional precision beyond nanoseconds and `24:00:00` spelling without creating a temporal-library project.

A caller wanting arithmetic must deliberately parse the text under the wire contract and handle an unspecified zone. Lexicographic ordering of differently offset strings is not chronological ordering. No cache TTL, staleness rejection or instant helper is introduced by C3.

**Precise remaining vendor question:** “Does Számla Agent forward the current NAV QueryTaxpayerResponse unchanged, including these five fields? Is `infoDate` always timezone-qualified in practice, and which taxpayer-data changes advance it?” None blocks textual preservation. If a future parsed-time helper is requested, answer the semantic question before promising freshness semantics; even a UTC-only observed sample is not a guarantee overriding the published unrestricted type.

### Parser boundary and acceptance

Extend extraction and final projection together. Read new leaves only at the paths in the table. NAV 3.0 puts tax-number child components in the base namespace, business fields and root `infoDate` in the API namespace, and result in Common; NAV 2.0's tax components use its data namespace. Prefix spelling is irrelevant. Coordinate with R1's complete-document/path-aware parser work; adding five global local-name setter arms would perpetuate the exact structural collision being fixed there. Unknown subtrees must not contribute recognized leaves.

For the new textual business fields, blank-to-`None` must not mean trimming meaningful nonblank names or inventing values. Use the open token mapping for incorporation and retain unknown spelling. Keep existing `OK + taxpayerValidity=false` as a successful result and retain supplied optional data rather than fabricating defaults based on validity.

Acceptance: old official 2.0 success/invalid/error fixtures still pass; the success now exposes exact `2004-12-26T23:00:00.000Z`. Add a genuine 3.0 common/base fixture with all five and two addresses, field omissions and leading-zero codes, every known incorporation plus a future token, root-only `infoDate`, and same-named leaves under unrelated wrappers/namespaces that cannot populate the result. Text cases include `Z`, positive/negative offset, **no offset**, more than nine fractional digits, `24:00:00`, malformed nonblank text and multibyte text: all retained verbatim by this field, with no guessed datetime. Old JSON defaults all five to `None`; known and unknown incorporation serialize as strings, not enum-tag objects.

## Cross-crate scope and implementation sequencing

- **Agent first:** C1 is a breaking public request addition; C2/C3 are additive `#[non_exhaustive]` response additions. Do not rename existing fields or replace response types while delivering them. Update README/rustdoc, request/response fixtures and provenance with the selected policies.
- **Worker separately:** `restate-szamlazz/src/contract/agent.rs:286–324` intentionally projects `TaxpayerInfo` into its own journaled result. Adding five agent fields does not authorize widening that contract/journal. Its field-access conversion continues to compile. The worker's header constructor gives `simple_items=None`; no new account default, ownership pin or durable state follows from C1. The worker has no PDF-query handler to extend here.
- **CLI separately:** current invoice download writes `fetched.pdf` and outputs a small explicit number/path JSON object (`szamlazz-cli/src/commands/invoice.rs:233–246`); it does not automatically expose all `InvoicePdf` fields. Any CLI output expansion needs its own decision. Agent serde consumers do see new fields; strict schemas should be called out in release notes.
- **Ordering of work:** implement C1 alongside F2's known refusals; C2 alongside or after F3's monetary-header reader; C3 on the R1 path-aware extraction boundary, coordinating R2's text policy. These are implementation dependencies, not a request to merge all findings into a global schema validator.

**Judge handoff:** the meaningful change from round one is that C1 now has a defensible fixed serializer order corroborated by first-party PHP, with the vendor conflict still honestly unresolved. C2's auxiliary data has an explicit bounded disposition rather than “transport metadata” as a blanket exclusion. C3 has a concrete timezone-safe field policy and is classified as a useful SDK enhancement, not mandatory NAV conformance. All three have implementable scopes without issuing a document or inventing account state.
