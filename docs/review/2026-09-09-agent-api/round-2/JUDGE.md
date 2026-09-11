# Independent final judgment — Számla Agent, round 2

**2026-09-09 · reviewed source baseline `a804c740eb8446211c1cdca3eea4fb93d298d25d`.**

## Decision in brief

**The real work is six agent implementation defects, one consequential worker interpretation defect, four bounded capability additions, sixteen substantive documentation corrections, and one evidence/test-methodology package.** That is **28 deduplicated actionable findings**, organized into **11 implementation packages**, not 28 observed incidents. Five smaller matters are notes/housekeeping, not extra defects. No P0/P1 is established.

| Category | Count | Canonical IDs |
|---|---:|---|
| Agent parsing/classification/helper defects | 6 | F1, F2, F3, R1, R2, E.E1 |
| Cross-crate interpretation defect, with agent guidance correction | 1 | E.E5a |
| Capability additions | 4 | C1, C2, C3, E.E4 |
| Substantive documentation/provenance corrections | 16 | D1–D7, D9, D12, D13, B.E01–B.E04, E.E2, E.E3 |
| Evidence/corpus/test-methodology package | 1 | E.E6 |
| Notes/housekeeping, excluded from 28 | 5 | D8, D10, D11, B.E05, N.AF |

**Priorities:** seven P2 findings (F1–F3, C1, D1, D2, E.E5a); the other 21 are P3. P2 means consequential normal-priority work, not demonstrated high-frequency failure. The worker portion of E.E5a is a separately owned follow-up, not permission to edit concurrent worker work.

All original **17** findings have dispositions: **16 retained, D8 demoted to housekeeping**. E.E5 and E.E6's subitems, B's five residuals, D10–D13, and all **44** rows of E's register are explicitly accounted for below. `B.E01` is reviewer B's E01; `E.E1` is reviewer E's E1. Reviewer B's own second “judging pass” is evidence, **not this independent judgment**.

### Material changes to the earlier conclusions

1. **C1:** choose `szamlaSablon → elonezetpdf → simpleItems`. Concrete PHP 2.12.4 emission and the downloadable schema justify this implementation policy. The two contrary inline schemas remain contrary evidence; a locally merged XSD cannot settle what the server accepts.
2. **C2:** add PDF outstanding amount and customer URL only. Decline PDF id/payment-method/notification-flag expansion on the present operation-specific evidence. **E.E4 is retained:** create/storno payment method has explicit operation-specific documentation.
3. **C3:** use **`info_date: Option<String>`**, preserving decoded nonblank source text. An instant-only or civil-datetime field is the wrong immediate type for unrestricted `xs:dateTime` with optional timezone.
4. **F1:** fix timezone/padding interoperability without imposing a new CE 0001–9999-only acceptance policy. Strict range/lexical cleanup is not necessary to cure the reproduced failure.
5. **R1:** shared complete-document boundaries plus versioned taxpayer path/namespace extraction; no full XML/XSD conformance project. Sparse payloads and numbered-56 policies survive.
6. **B.E05 / E.E5c:** receipt MNB is **first-party-supported**, not merely an invoice extrapolation. Retain the feature; source-conflict annotation is a note, not another proven public-contract defect.
7. **Receipt order lookup:** “last matching document” is documented by the PHP operation page. Only the meaning of “last,” older-record recovery and post-reversal selection remain open.
8. **E.E5a:** negative-original/positive-storno failure is not observed. Nevertheless, missing *optional* gross already proves that `reverses == false` cannot mean “no reversal.” Correct the helper guidance and the worker's false-to-no-op branch; do not wait for a negative-original live incident.
9. **B.E02:** the stall claim has conflicting provenance, not a proven false history. The checked behavior row reports no issuance; CONTEXT's issue-policy entry reports the general opposite. Preserve uncertainty and the timeout policy while tracing the stronger claim.
10. **E.E6:** the outline comparator erases a documented email-block distinction. This needs a focused semantic-presence assertion, not merely a comment and not a new global XSD gate.

## 1. Basis and independence

Read every line of [A](01-parsers.md), [B](02-errors-receipts.md), [C](03-capabilities.md), [D](04-domain-docs.md), [E](05-scope-challenge.md), [REVIEW](../REVIEW.md) and [prior adjudication](../ADJUDICATION.md). Independently inspected the actual XML, taxpayer, envelope, PDF, credit-entry, receipt, invoice, wire, client and error paths; the worker's storno branch and identity predicate; the outline comparator, fixture provenance and live-test introduction; and the behavior record including its unverified list. This judgment does not adopt a finding by reviewer majority.

**Fresh public-document GETs in this judging pass:** PDF/storno responses, receipt query PHP page, simplified-image page, session-cookie page, receipt-send XML, Hungarian VAT page, general error catalogue, and pinned NAV `invoiceApi.xsd`. Inspected the already downloaded official PHP archive's actual files and verified its SHA-256; traced header array construction through recursive XML emission. Checked all three invoice schema hashes. These are primary sources even though the PHP archive was acquired by reviewer C; I did not execute PHP or re-download that archive.

After inspecting their source, independently reran these offline parser probes:

```sh
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/reviewer-a-parsers/Cargo.toml --bin reviewer-a-parsers
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/scope-challenge-e/Cargo.toml
```

They reproduced date rejection, comma rejection and underscore acceptance, header/body precedence, taxpayer truncation/overwrite/entity corruption, shared trailing-content acceptance, text loss, the exact-cookie-name defect, HTTP precedence, and the storno sign heuristic. These runs verify **current mechanisms**, not proposed fixes. Missing-gross behavior follows directly from `Option::is_some_and` and the parser's optional field; it was source-checked, not a new live observation. No full suite rerun or fresh schema-validation run is claimed. C's recorded libxml2 conflict experiment was inspected, not relabeled as mine.

Only this report was authored in the repository. No production edits, live account operations or further agents. Concurrent changes were present in Cargo.lock, worker, CLI and Adatkapcsolat; conclusions about source refer to the cited baseline/inspected call sites, not an endorsement of those concurrent changes. During the final check, another edit appeared in the agent README's HTTP-precedence paragraph. It partially addresses E.E2's README surface; the cited wire/client/error source claims remain, so E.E2 is not closed. That edit was inspected and left untouched.

### Citation convention and primary sources

`src/…` and `tests/…` below mean `crates/szamlazz-agent/src/…` and `crates/szamlazz-agent/tests/…`. `behavior` means [docs/szamlazz-hu-behaviour.md](../../../szamlazz-hu-behaviour.md). PHP paths are inside `PHPApiAgent-2.12.4/szamlaagent/`. Line references are to the inspected baseline. The round-two reports retain more granular quotations and source inventories; where not re-fetched here, their source comparison is attributed rather than claimed as new verification.

| Ref | Primary source |
|---|---|
| S.DATE | [Queried-invoice XSD](https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd), [receipt response](https://docs.szamlazz.hu/agent/generating_receipt/response), [XSD date/whitespace](https://www.w3.org/TR/xmlschema-2/) |
| S.XML | [XML 1.0](https://www.w3.org/TR/REC-xml/) — document production, references and character content |
| S.ERR | [General errors and retry ceiling](https://docs.szamlazz.hu/agent/basics/error-handling), receipt supplement in S.DATE |
| S.SIMPLE | [Simplified-image rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) |
| S.INLINE | Invoice [English](https://docs.szamlazz.hu/agent/generating_invoice/xml) / [Hungarian](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) inline XSDs |
| S.DOWNLOAD | [Invoice download XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd), [sending guidance](https://docs.szamlazz.hu/agent/basics/sending-requests) |
| S.PHP | [Official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip); SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| S.REPLY | [Create](https://docs.szamlazz.hu/agent/generating_invoice/response), [storno](https://docs.szamlazz.hu/agent/reversing_invoice/response), [PDF query](https://docs.szamlazz.hu/agent/querying_pdf/response), [credit entry](https://docs.szamlazz.hu/agent/credit_entry/response) |
| S.NAV | [Agent taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response); [NAV pinned API schema](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1552-L1581), its `TaxpayerDataType` at 1846–1889; [base schema](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd); [NAV specification §1.8.9.2](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf) |
| S.RQUERY | [Receipt query request](https://docs.szamlazz.hu/agent/querying_receipt/request), [XML](https://docs.szamlazz.hu/agent/querying_receipt/xml), [PHP last-match rule](https://docs.szamlazz.hu/php/nyugta-lekerdezes), [receipt order rules](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number) |
| S.RSEND | [Receipt-send XML](https://docs.szamlazz.hu/agent/sending_receipt/xml), [response](https://docs.szamlazz.hu/agent/sending_receipt/response), [receipt storno response](https://docs.szamlazz.hu/agent/reversing_receipt/response) |
| S.AMOUNTS | [Receipt amounts](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts), [currencies/general foreign-receipt rule](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) |
| S.VAT | [Hungarian VAT definitions/eusAfa](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates), [vendor VAT table](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf) |
| S.BANK | [Hungarian outgoing-invoice schema annotation](https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak) — the same queried `szamla` field, not a transfer of Adatkapcsolat protocol rules |
| S.LAYOUT | [Document types](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types), [templates](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template), S.INLINE's waybill/partner annotations |
| S.ERASURE | [Erasure field](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/data-erasure-code), [stock/template guidance](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor), S.ERR |
| S.SESSION | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie), [RFC 6265 §5.2](https://www.rfc-editor.org/rfc/rfc6265.html#section-5.2) |

**Confidence vocabulary:** existence = confidence that the stated gap exists; solution = confidence in the selected bounded remedy. **High** means direct code/source or executable counterexample with a clear remedy; **Medium** means meaningful source ambiguity or integration design still requiring proof; **Low** means the proposed stronger claim is not established. Live occurrence is a separate statement on every retained finding. An optional field's schema declaration proves representability, not its emission frequency.

## 2. Implementation defects

### F1 — date spelling interoperability

**Retain · P2 · response interoperability. Existence High:** S.DATE permits suffixes/XML padding; all three reader families reject them in the rerun. **Solution High:** a private adapter can preserve the printed civil date with unchanged public types. **Live:** no timezone-qualified/padded-date failure recorded; receipt create/storno impact is conditional on such a reply.

**Boundary:** `src/xml.rs:269–283`; `src/ops/query_xml.rs:703–708,810–825,957–960,994–997,1034–1039`; `src/ops/receipt.rs:735`. Eleven invoice date positions and the common receipt issue date need required/optional adapters.

**Best solution:** a compatibility-preserving date-to-civil adapter. Keep the current finite `Date` domain and existing accepted unsuffixed spellings; add a deliberately parsed wire branch for an ASCII hyphenated date followed by **nothing, `Z`, or `±hh:mm`**. Validate offset hours ≤14, minutes ≤59 and minutes=00 at hour 14, then discard only that validated suffix and parse the calendar portion fallibly. Trim XML whitespace for the new supported forms and required-date padding. Whole-input handling and checked boundaries must reject malformed suffixes/junk without panicking. An invalid nonempty date is still an error; optional absence/blank remains `None`, required absence/blank fails.

This is **not advertised as strict XSD lexical validation**. Retaining already accepted basic-date/Temporal annotation/year-zero forms as compatibility extensions is intentional for this repair. Do not combine arbitrary junk stripping with a Jiff fallback: the newly added offset branch must recognize its entire input. A small explicit legacy fallback may retain the old parser's acceptance; its contract is legacy compatibility, not proof of XSD validity. Preserve the optional helper's existing Unicode-padding acceptance unless deliberately changed in separately reviewed cleanup; no new global CE range or NBSP refusal is necessary here. Required dates need XML padding support, not arbitrary Unicode-padding expansion.

**Acceptance:** cover every annotated path, plain/Z/±02:00/±00:00/±14:00, XML padding on required and optional fields, leap-day/impossible-day, invalid offsets, suffix junk and multibyte short input. Lock in existing accepted compatibility controls so the fix does not secretly tighten range/grammar. Receipt create/storno/query share a complete canonical fixture. Printed calendar date stays the same, never shifted to UTC.

**Dismissed:** A's CE 0001–9999-only rule as a prerequisite (unnecessary new refusal); slicing the first ten bytes (junk/panic risk); public zoned dates (wrong projection/breakage); invalid-to-`None` (changes this crate's content policy). Strict grammar is a possible later policy decision, not another confirmed defect.

### F2 — thirteen known error meanings missing

**Retain · P2 · classifier coverage. Existence High:** `src/error.rs:220–253,314–348` lacks source-documented tokens. **Solution High:** explicit names plus classes fit the existing open enum. **Live:** none of these thirteen observed in the account record; no automatic retry is performed by the client.

| Codes | Required meaning/class |
|---|---|
| 336, 337 | Receipt prefix used for invoices / invalid receipt prefix → `Rejected` |
| 339 | Receipt not found → `NotFound` |
| 340 | Receipt tender total mismatch → `Rejected` |
| 363, 364, 365 | HUF receipt item gross not integral / net precision / VAT precision → `Rejected` |
| 551 | Simplified-image account incompatibility (OSS **or** non-Hungarian seller tax number) → `Rejected` |
| 552, 553 | Simplified-image item count / VAT token → `Rejected` |
| 554 | Cannot correct a simplified-image original → `Rejected` |
| 555 | Simplified final/prepayment VAT mismatch → `Rejected` |
| 556 | Simplified corrective/delivery-note prohibited → `Rejected` |

**Best solution:** add all thirteen named mappings and reverse wire tokens; 339 joins the documented `NotFound` set, all other twelve are rejected, none retryable or credential-related. B's proposed names are a sound naming starting point ([B:58–72](02-errors-receipts.md)). Source is S.ERR/S.DATE/S.SIMPLE. Do not defer behind C1: 554 applies without the flag, and inherited finals explicitly make 551/555 relevant already; 552/553 constraints can also apply to those finals. Direct 556 reach is narrower. This is one catalogue defect, not thirteen incidents.

`Rejected` describes the exchange returning it, not the entire history of a logical operation. Keep 338 duplicate refusal distinct from replayed success, 335 unchanged, 7's missing-input meaning on some writes, and all unknown numeric/NAV text codes open. Document that 55 remains unknown and numbered 56 is handled by the response parser.

**Acceptance:** a source-derived table independent of the implementation registry, invoice/receipt body paths and representative headers, padded numeric/reverse tokens, original Hungarian messages, future codes, 338/335/7/55 and numbered/unnumbered-56 controls. **Dismissed:** ranges classified wholesale, enum names without class changes, operation-aware classifier redesign, arithmetic/account guards.

### F3 — comma monetary-header fallback

**Retain · P2 · conditional response interoperability. Existence High:** behavior:160 records `100,01`; `src/ops/envelope.rs:146–155,285–303` rejects it, including the credit-entry consumer at `src/ops/credit_entry.rs:250–277`. **Solution High:** one header-specific grammar fixes the shared boundary. **Live:** comma create-net emission observed; the missing-body-plus-comma failure is not observed; no independent storno/credit/PDF comma emission claim.

**Best solution:** accept ungrouped ASCII finite numbers, dot or comma as the decimal separator, with optional sign, optional scientific exponent and surrounding HTTP SP/HTAB. Grammar:

```text
[SP/HTAB]* [+-]? (digits+ ([.,] digits*)? | [.,] digits+)
              ([eE] [+-]? digits+)? [SP/HTAB]*
```

Validate the whole grammar, translate a mantissa comma to dot, and use Decimal directly with its existing finite representability policy. **`1,234` means `1.234`, not `1234`.** Sender intent is unknowable from this spelling; do not claim to detect ambiguous grouping. Reject underscores, repeated/mixed separators, embedded spaces and malformed exponents. Accept `.5`, `1.`, ordinary dot/exponent and comma/exponent forms; no two-decimal lexical cap.

Keep XML amounts separate. Valid body wins; malformed nonblank body is not rescued by a header. Missing header is `None`, present blank header remains error. Numbered 56 keeps valid comma metadata and still drops genuinely malformed optional metadata.

**Acceptance:** each net/gross/outstanding header, signs/zero/exponents, separator convention, overflow and malformed input; body absent/empty/blank/present/bad precedence; numbered-56 fallback. One shared grammar table plus representative issuance/credit/PDF integration checks. **Dismissed:** locale guessing, stripping grouping, float conversion, rejecting all exponents, broad malformed-metadata tolerance, XML comma acceptance. Release-note the deliberate underscore/OWS acceptance cleanup rather than claiming no acceptance tightening.

### R1 — complete response and structurally scoped taxpayer extraction

**Retain · P3 · robustness. Existence High:** actual `response_root` returns at the first start tag (`src/xml.rs:63–108`); taxpayer's local-name setter and EOF handling (`src/ops/taxpayer.rs:202–338`) reproduce corruption/false success. **Solution Medium:** the decomposition is clear, but shared integration and versioned extraction require regression proof. **Live:** synthetic malformed/out-of-contract responses only. HTTP framing-detected truncation fails earlier at `src/client.rs:284`; this defect needs a collected truncated body/direct `RawResponse`.

**Best solution, two cooperating responsibilities:**

1. **Shared XML document boundary:** keep quick-xml; check UTF-8, expected expanded root, matching closes, exactly one completed root and legal prolog/epilog through EOF. Use parser-provided checks rather than reimplementing XML tokenization. Track document phase/depth to reject second roots, open-root EOF, outside ordinary text/CDATA/references and malformed tails. Permit BOM, legal declaration, comments, PIs and literal XML whitespace. Do not return as soon as the root matches.
2. **Taxpayer projection:** use `NsReader` with root-selected 2.0/3.0 layouts and a small recognized-parent-path stack. Skip extraction from the whole unknown/foreign subtree; still consume its XML so truncation is visible. Reject duplicate **recognized singleton** verdict/business leaves or containers that would select one conflicting answer, including empty duplicates. Recognized scalar content cannot contain child elements. Append each address from its own completed context. Resolve predefined/numeric references faithfully; undefined references must error rather than disappear. Let the XML library handle decoding; use a tiny XML-character guard only where its numeric-reference result otherwise permits forbidden XML characters. No DTD resolver; unsupported DOCTYPE may be refused explicitly.

| Recognized location | NAV 2.0 namespace | NAV 3.0 namespace |
|---|---|---|
| Root and taxpayer business containers/leaves | `OSA/2.0/api` | `OSA/3.0/api` |
| `result` and `funcCode/errorCode/message` | `OSA/2.0/api` | `NTCA/1.0/common` |
| `taxNumberDetail` child components and address components | `OSA/2.0/data` | `OSA/3.0/base` |

URIs begin `http://schemas.nav.gov.hu/`; prefixes are arbitrary. Actual parent paths are root `result`, root `taxpayerData`, its `taxNumberDetail` and `taxpayerAddressList/taxpayerAddressItem/{taxpayerAddressType,taxpayerAddress/…}`. C3 adds root `infoDate`. A Common *type* does not move an API-declared element into the Common namespace.

**Scope stop:** no complete XML Namespaces validator, DTD processor, schema dependency/import resolution, NAV header/software validation, global facet/order/cardinality engine, or invoice/receipt child-namespace rewrite. Use checked attributes/namespace decoding where this reader consumes them; do not claim full W3C well-formedness certification from `enable_all_checks`. The shared change owns document completion; descendant expanded-name/path semantics here belong to taxpayer extraction. A separate demonstrated collision on a serde invoice field would need its own bounded assessment.

Keep sparse business data, unknown open tokens and repeated address rows. `OK` requires validity, `false` remains data, non-OK remains an API answer. Keep header/down/status precedence and numbered-header-56 non-XML fallback: the boundary checks **XML being interpreted**, not every body before header handling. Optional PDF/content softness on numbered 56 survives.

**Acceptance:** truncation at several depths; two roots/tails; duplicate ERROR→OK in one root; unknown/foreign wrappers at recognized levels; wrong namespaces; scalar nesting; undefined/numeric/predefined references, CDATA/comments/PIs and legal surroundings; independent multiple addresses; empty tags. Replace the fake `/2.0/→/3.0/` test and root-level address fixtures (`taxpayer.rs:408–417,465–529`) with genuine declared layouts. Exercise shared completion through XML query, envelope, receipt and deletion. Keep sparse errors without software/header requirements and numbered-56 controls.

**Dismissed:** EOF-only checking (misses depth and overwrite), namespace-only checking (misses paths), first-verdict wins (hides ambiguity), global XSD validation (breaks deliberate tolerance), a new DOM/dependency solely for this repair, undefined-entity deletion. A's broader scanner wishlist is not a mandate to build a home-grown general validator.

### R2 — decoded optional-text fidelity

**Retain · P3 · fidelity. Existence High:** the generic helper trims before String conversion (`src/xml.rs:269–283`), and taxpayer globally trims at `src/ops/taxpayer.rs:261–266`; rerun shows padded/NBSP-only data loss. **Solution High:** explicit text/scalar separation addresses the mechanism. **Live:** no padded receipt identity collision or production text-loss incident recorded.

**Best solution:** private optional-business-text helper: absent/empty/**XML-whitespace-only** → `None`; otherwise preserve the original **decoded character string**. XML blank is only SP/TAB/CR/LF. **NBSP-only is `Some`, not blank.** Apply deliberately to invoice/receipt identifiers, comments, bank/ledger text and other business strings; preserve optional open wrapper text before converting `PaymentMethod`/`Currency`, including unknown spelling. Use the same policy in R1's taxpayer leaf extraction and C3's new fields.

Keep number/date/bool/verdict parsing separate; keep the explicitly normalized issuance envelope number, URL/base64-specific behavior and worker order projection separate. Audit VAT-token strings instead of bulk-replacing every `Option<String>` annotation. Required string handling already preserves text and does not need global replacement. Entity/CDATA decoding and XML newline normalization remain correct; byte-for-byte XML retention is not promised.

**Acceptance:** padded receipt call/order/original numbers, item id/comment/ledger/tender description; invoice comment/reference/open payment token; taxpayer name/address. Assert spaces, tabs, NBSP-only and entity/CDATA character content exactly. Missing/empty/XML-blank remain none, numeric padding and padded error codes still work, envelope-number and worker normalization remain intentional. **Dismissed:** changing the generic scalar helper globally, Unicode blank detection for business strings, reader-level trim, raw-XML public wrappers, documenting away all text loss.

### E.E1 — exact cookie name

**Retain · P3 · public helper correctness. Existence High:** independently rerun `JSESSIONIDOTHER=wrong` before `JSESSIONID=right` returns the wrong pair; bare `JSESSIONID` also returns as a cookie. **Solution High:** RFC pair/name parsing is small and decisive. **Live:** competing vendor emission unobserved; it is a valid cookie-name input to this public helper regardless of vendor frequency. Native reqwest jar is unaffected.

**Boundary:** `src/wire.rs:308–325`, S.SESSION/RFC 6265. **Best solution:** take each first semicolon-delimited pair, require `=`, compare the cookie name exactly and case-sensitively with `JSESSIONID`, optionally trim HTTP SP/HTAB around name/value, preserve the value, continue past malformed/nonmatching entries. Preserve any further `=` in the value. State that lifetime/path/domain handling requires the transport's jar.

**Acceptance:** competitor before real, competitor alone, bare name, empty valid value, ordinary pair, value with `=`, SP/HTAB and mixed-case *HTTP header* names; differently cased cookie name must not match. **Dismissed:** prefix match, cookie-name case folding, a full jar in a URL-free convenience helper, a live-emission prerequisite.

### E.E5a — a false storno heuristic is inconclusive

**Retain and strengthen · P2 · cross-crate outcome interpretation + agent guidance. Existence High:** `CreatedInvoice::gross_total` is optional (`src/ops/envelope.rs:41–42,214–219`), but `reverses` is false without it (`56–75`); worker `crates/restate-szamlazz/src/gateway.rs:1607–1615` translates *every* false to `NotStornoable`/“no-op.” A missing auxiliary total does not establish non-reversal. **Solution Medium overall:** query type/reference is the right evidence, but its durable-step integration and post-send failure mapping need worker review/tests. Agent prose correction itself is High confidence. **Live:** positive-original negative stornos and same-number proforma/delivery-note echoes were observed (behavior:79,86–87); zero-total/negative-original and this missing-gross compound failure were not. The positive-gross format example in S.REPLY proves no negative-original execution.

**Best solution:** keep the low-level boolean as a documented *heuristic*, explicitly stating false means “not established by this reply,” not “nothing reversed.” Remove the universal “check every caller must make” and unqualified zero-storno fact. At the worker's wire-decision boundary, replace boolean negation as a verdict with **query-backed reconciliation of an ambiguous numbered reply**: a changed number with absent/positive gross must be queried by returned number, and accepted as reversal only when its queried `document_type=Storno` and `referenced_invoice_number=original`. The existing `FoundDocument::is_storno_of` (`gateway/document.rs:135–140`) is the identity predicate. Preserve the existing observed fast path for a changed number/nonpositive gross and the observed same-number echo policy, documenting their evidence bounds.

This needs a richer **internal decision** (observed reversal / same-number echo / needs verification), not a new public boolean pretending to be conclusive. Put the verification inside the query-first storno step, so an unanswered verification after a send remains `Unconfirmed` and cannot become settled no-op. A negative/mismatching query is not automatically proof no reversal occurred; use the existing storno external-id reconciliation and keep the result unknown when it cannot establish the outcome. Credential/unavailable answers retain post-send uncertainty; do not reinterpret them as a rejected send. Review journal/step implications in that worker change.

**Acceptance:** changed number with missing gross and with positive gross takes verification; queried matching SS settles reversal; wrong type/reference, not-found or unanswered verification never silently settles no-op; known positive-original/negative-total and same-number echo controls remain. Unit/mock assertions prove decision handling, not live negative-original acceptance. A future live claim requires an authorized original/storno/query record, separating 14/221 refusals.

**Dismissed:** docs-only closure while keeping the worker's incorrect inference; making gross required (the wire makes it optional, and 56 may soften it); changed-number alone as identity proof; comparing signs with the original as definitive; I/O in `reverses`; claiming the negative-original failure observed. This finding is counted once, including its guidance part.

## 3. Bounded capability decisions

### C1 — optional simplified invoice image

**Retain · P2 · request capability. Existence High:** no field in `InvoiceHeader` (`src/ops/invoice.rs:135–218`) or writer (`758–768`) represents S.SIMPLE's per-document boolean. **Solution Medium overall:** field/default is High; combined-preview server ordering remains uncertain. **Live:** neither simplified-image issuance nor combined preview was observed.

**Best solution:** `InvoiceHeader::simple_items: Option<bool>`, `#[doc(alias = "simpleItems")]`, default `None`; absent/null JSON is none, explicit false/true emits the exact-case element. Keep full item monetary data and existing plain-data construction. Tail is **template, preview, simple items**.

**Independent evidence decision:** S.PHP `src/szamlaagent/Header/InvoiceHeader.php:398–405` appends template, preview, simple items; `src/szamlaagent/SzamlaAgentRequest.php:259–280` iterates and recursively emits without sorting. S.DOWNLOAD's header sequence agrees. Both S.INLINE schemas disagree when both flags are present. These are not three independent server observations or a majority vote. Concrete current first-party serialization plus the named download is the strongest implementation guide available; it does not erase inline contract uncertainty.

**Fixture policy:** keep unmodified dated source snapshots with hashes separately from project-authored expectations. Assert the chosen writer order directly. Retain separate source-schema checks which truthfully show simple-only succeeds against all and the combined order disagrees; retain existing group-id/erasure coverage through the sources that actually contain them. **Do not create a merged “canonical current vendor XSD”** by moving an inline element and then cite its passing validation as upstream agreement. A project-maintained compatibility fixture may exist only as explicitly transformed test data, and is unnecessary to settle this finding.

Document independent selection on invoice/proforma/prepayment, final/storno inheritance, corrective/delivery-note restrictions, account prerequisites, 2-item/4-item final exception, allowed VAT tokens and matching final VAT, server template override and `K.AFA` comment requirement (S.SIMPLE). No local OSS/original-state queries, item-count/VAT gates, kind redesign or worker default. Do not add a storno flag: its state is inherited. Send the creation field as caller data even where the server may refuse or ignore it.

**Acceptance:** external consumer construction/serde absent/false/true; default XML unchanged; both preview values with both simple values in chosen order; six creation kinds retain full item data; group-id/erasure fields survive. Explicitly demonstrate the source conflict, not a fictional universal pass. Never drop preview or retry an alternate order automatically. Adding a field to the exhaustive request struct is Rust source-breaking; schedule the appropriate breaking 0.x release.

**Open question:** which order(s) does `action-xmlagentxmlfile` accept with both flags, and does the combination preserve non-issuing preview? Vendor confirmation or separately authorized combined-field evidence closes it. **Dismissed:** inline-majority policy, blanket download replacement, runtime order switches, combination guard that needlessly blocks the field, auto-resend, enum-template workaround.

### C2 — PDF result balance and customer URL

**Retain, narrowed · P3 · optional exposure. Existence High:** PDF's own S.REPLY schema declares `kintlevoseg`/`vevoifiokurl`, shared parsing reads them, and `src/ops/query_pdf.rs:36–48,75–83` drops them. **Solution High:** project two optionals without changing PDF success invariants. **Live:** PDF-specific emission unverified; existing downloads establish retrieval, not these fields.

**Best solution:** add `outstanding: Option<Decimal>` and `customer_account_url: Option<String>` to `InvoicePdf`, defaulting absent on old JSON. Carry existing parsed values through. Preserve required `pdf: Pdf` and invoice number, body-before-header precedence, missing-not-zero amounts, opaque URL and one-time header decoding. No recalculation, URL fetch or validation.

**Explicit auxiliary disposition:** **do not add PDF `document_id`, `payment_method` or `notification_delivery_failed` in this package.** Their omission is real mechanically, but the generic PHP response reader and shared parser's synthetic acceptance do not establish operation-specific usefulness/emission. In particular a PDF read is not known to send mail. Keep current numbered-56 acceptance behavior; documenting inherited envelope tolerance is sufficient now. Adding a delivery-warning field because a shared parser computes it would turn a convenience implementation into the public operation contract. A concrete PDF consumer need plus operation-specific source/capture can reopen these enhancements. The credit balance's id and NAV transport/software diagnostics are likewise explicit scope exclusions, not forgotten findings.

**Acceptance:** body/header/absent balance/URL, conflicting values, signed/zero amount, literal body URL versus encoded header, old JSON defaults, required PDF/number and existing 56 behavior. F3 owns comma parsing. **Dismissed:** replacing/nesting `CreatedInvoice` (weakens required PDF and imports issuance semantics), all-metadata expansion, raw-response bag, declaring a mandatory conformance defect. This declines C's three auxiliary additions while accepting E's operation-specific standard.

### E.E4 — create/storno payment method

**Retain · P3 · optional exposure. Existence High:** S.REPLY explicitly lists `szlahu_fizetesmod` on **create and storno**, while `CreatedInvoice` omits it (`src/ops/envelope.rs:27–54`). **Solution High** for exposure and reuse of the reader; **Medium** for claiming any broader vendor encoding grammar. **Live:** these operations' payment-method emission/encoding was not independently captured here.

**Best solution:** `CreatedInvoice::payment_method: Option<PaymentMethod>`, default none; share the existing header helper from `src/ops/credit_entry.rs:281–286` with the envelope. Keep its percent-decoded textual/open-token policy and preserve unknown values. Document header origin; no invented XML body element. This meets the same source standard as C2, rather than treating headers as intrinsically unimportant. Worker/CLI projections are separate choices.

**Acceptance:** present known/unknown/absent values in create and storno replies, shared credit-entry controls, decoded text, old JSON. Keep 56 and money policies independent. **Dismissed:** raw-only workaround as built-in coverage, a required field, global result expansion, changing URL/plus semantics without wire evidence.

### C3 — five taxpayer business fields

**Retain · P3 · optional exposure. Existence High:** S.NAV defines the fields; `TaxpayerInfo`/private projection (`src/ops/taxpayer.rs:110–127,187–199,317–354`) omit them. **Solution High** for the optional source-data surface; R1 integration is Medium until tested. **Live:** `infoDate` is in the Agent's dated example, not a fresh lookup; current forwarding of all five is unverified. NAV's client-use discretion means this is a useful feature, not mandatory full-NAV conformance.

| Public addition | Exact type | Extraction/meaning |
|---|---|---|
| `short_name` | `Option<String>` | `taxpayerData/taxpayerShortName`, never generated from name |
| `county_code` | `Option<String>` | `taxpayerData/taxNumberDetail/countyCode`, leading zeroes retained |
| `vat_group_membership` | `Option<String>` | `taxpayerData/vatGroupMembership`, eight-digit group id, not bool/full tax number |
| `incorporation` | `Option<Incorporation>` | `taxpayerData/incorporation`; open wire-token enum |
| `info_date` | **`Option<String>`** | Root `infoDate`, decoded nonblank source text of last data change |

**Best solution:** add those five only, serde defaults none. `ops::taxpayer::Incorporation` uses `Organization` ↔ `ORGANIZATION`, `SelfEmployed` ↔ `SELF_EMPLOYED`, `TaxablePerson` ↔ `TAXABLE_PERSON`, `Other(String)`; string serialization, open enum. The third token means a private person with a tax number, not every taxable person. Keep existing `tax_number` as the eight-digit stem and do not fabricate missing components or derive buyer status/VAT choices.

**Temporal decision:** S.NAV `invoiceApi.xsd:1560–1564` says `xs:dateTime`, not UTC-only `InvoiceTimestampType`; the Agent example is `2004-12-26T23:00:00.000Z`. Preserve timezone absence, original offset spelling and fractional precision. No UTC/Budapest default, nanosecond cap, date-only projection, staleness check or TTL inference. Nonblank malformed temporal text remains text and does not turn a useful lookup into a new parse failure. Rustdoc must call this **source text**, not a validated datetime. A new temporal wrapper or parsed-time helper is not justified for one advisory field.

Coordinate R1 paths and R2 character preservation; do not add five global local-name setter arms. NAV 2.0/3.0 tax components use data/base respectively; root infoDate/business fields use API. Optional missing/XML-blank is none; NBSP-only source text is retained. False validity remains successful data.

**Acceptance:** official 2.0 example exposes exact old infoDate; genuine 3.0 mixed namespaces and two addresses; all omissions, leading zeroes, three known/future incorporation tokens; root-only infoDate and foreign-path negatives. Keep Z/±offset/no-zone/>9 fraction digits/24:00/malformed/multibyte temporal text as supplied; old JSON defaults and string enum encoding. **Dismissed:** Timestamp (cannot represent no zone), civil DateTime (drops offset), Date (drops time), assumed UTC, full NAV transport model, worker journal expansion.

## 4. Documentation decisions and coherent work packages

Every row below is an individual canonical decision even when implemented together. For prose-only changes, **focused acceptance means rendered rustdoc/README review and compiling examples/links**, not tests that compare comment strings or simulate undocumented server behavior. Each row states why its two confidence levels hold and its live boundary.

### 4.1 Invoice/domain semantics

| ID / disposition | Existence / solution confidence; live qualification | Best bounded solution and focused acceptance | Code / source; alternatives dismissed |
|---|---|---|---|
| **D1 · retain P2 · semantic definition** | **High / High:** code names TAM's meaning for TAHK; Hungarian vendor list and VAT table distinguish them directly. No live TAHK misuse observed. | Define TAHK as “áfa tárgyi hatályán kívül (outside the subject-matter scope of VAT).” Acceptance: rendered definition distinguishes TAM and leaves exact `TAHK` token. | `src/types.rs:190–196,258–264,289–294`; S.VAT. No token substitution, tax validator or automatic ATK rewrite. |
| **D2 · retain P2 · consequential flag guidance** | **High / High:** `eu_vat` label omits an explicitly documented processing effect; wording-only correction is direct. No OSS/eusAfa execution observed. | State no Hungarian VAT; **when accepted**, true suppresses NAV submission; vendor seller prerequisites OSS/non-Hungarian tax number, item VAT still required, retroactive submission unavailable. Describe omission/false as wire choices. Acceptance: all these conditions are visible on the field, without promising universal rejection/acceptance. | `src/ops/invoice.rs:181–182,214,755–757`; S.VAT. No local OSS guess, buyer-derived eligibility or automatic flag setting. |
| **D5 · retain P3 · bank-account semantics** | **High / High:** exact Hungarian same-schema annotation contradicts “arrived on” and defines fallback. No bank-linked live entry inspected. | “Sender's bank account when known; otherwise the bank account shown on the invoice.” State no discriminator of the two. Acceptance: direction and fallback both appear. | `src/ops/query_xml.rs:511–512,1042–1043`; S.BANK. No unconditional `sender_account` rename or invented fallback. Mirrored worker prose is a follow-up for its owner, not another finding. |
| **D6 · retain P3 · deletion scope** | **High / High:** behavior:110 is a direct paid-deletion counterexample; removing the adjective is sufficient. Live **test-account** paid deletion observed, not production data loss/universal acceptability. | Say “existing proforma”; operation does not check paid state, and paid deletion occurred on the test account. Caller retention policy is separate. Acceptance: module/type docs no longer imply unpaid-only protection. | `src/ops/proforma.rs:1–2,26–31`; [deletion request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), behavior:109–110. No added read/paid guard. |
| **D7 · retain P3 · partner identity guidance** | **High / High:** S.INLINE explicitly documents partner updates/account-link consequences. Those effects not tested through explicit identifier collision. **Merged temporal subitem: Medium / High:** “as recorded” is ambiguous, while behavior:111 shows later-query mutation on one account. | Document one partner per caller identifier within the billing account, recognized-id updates and access to that partner's documents through its customer account link; distinguish numeric queried buyer id. Describe queried buyer data as returned now, not guaranteed immutable issuance snapshot, with the bounded observation. Acceptance: conditional portal consequence and qualified temporal statement, no universal name-matching rule. | `src/ops/invoice.rs:288–289`; `src/ops/query_xml.rs:355–365`; S.INLINE, behavior:111. No uniqueness database, account vulnerability claim or reconstruction of historical buyer data. |
| **D9 · retain P3 · erasure prerequisites** | **High / High:** S.ERR splits 537/538/539, S.ERASURE explains template/stock. No live erasure operation observed. | Requests a count ≤400; 537 cap, 538 demo/test prohibition, 539 disabled setting. Invoice SzlaMost requirement separately; codes can come from uploaded stock or vendor supply. Acceptance: no guessed template error code and no claim all codes are newly generated. | `src/item.rs:107–114`; correct mappings `src/error.rs:162–167,249–251`; S.ERASURE/S.ERR. No template coercion, test-account guard or enum change. |
| **D12 · retain P3 · model/source attribution** | **High / High:** independent XSD elements do not encode exactly three permitted kinds; naming the Rust boundary fixes the assertion. Explicit ES/VS proforma-reference live acceptance remains unverified. | Explain the enum exposes proforma references on regular/prepayment/final; XSD declares reference independently of kind flags. Acceptance: type and accessor prose attribute the restriction to the model, not XSD. | `src/ops/invoice.rs:21–30,102–114,719–743`; S.INLINE; behavior:234–240. No new combinations or upgrade of implicit linking to explicit-reference evidence. |
| **D13 · retain P3 · waybill/layout guidance** | **High / High** for each constituent: code and annotations establish ordinary-invoice waybill, forced delivery-note template, generic barcode fallback, unused destination and named-template versus omission. No live carrier/PDF rendering failure observed. | Describe optional waybill with compatible layout, not only delivery notes; disclose delivery-note `SzlaFuvarlevelesAlap` override; generic barcode used if carrier data insufficient; `uticel` unused, Sprinter routing uses `iranykod`. Also distinguish named `SzlaAlap` from omission. Acceptance: each interaction identifies library versus server behavior correctly. | `src/ops/waybill.rs:1–5,91–96,127–130`; `src/ops/invoice.rs:183–184,758–764,821–823`; `src/types.rs:971–997`; S.INLINE/S.LAYOUT. No carrier exclusivity validation, new carrier blocks or changed template writer. Simplified-image precedence belongs to C1. |

### 4.2 Recovery, receipt rules and evidence precision

#### D3 — operation-specific recovery, not a generic external-id recipe

**Retain · P3 · caller recovery guidance. Existence High:** receipts and other mutations cannot be reconciled by the universal invoice-existence instruction in `src/error.rs:9–13,256–270,353–379` / `src/client.rs:63–72`. **Solution High:** a short operation table fixes the contract without a retry engine. **Live:** no receipt recovery incident observed; the nonexistent receipt external-id interface is independently evident from S.RQUERY.

**Best solution:** one public recovery table linked by common error helpers and README:

| Operation | Action after an unanswered exchange |
|---|---|
| Invoice create/storno | Reconcile supplied external id with identity/type checks and allowance for an earlier in-flight send; an immediate empty query alone is not permission to duplicate a write. |
| Receipt create | Persist a unique call id **before first send**, reuse it only for that logical issuance. 338 refuses the repeat; it does not recover number/PDF. Query a known receipt number or deliberately managed order, checking returned identity/type/reversal data before adoption. No conclusive match → unresolved recovery, not a fresh call id. |
| Receipt storno | Query known original and inspect reversal; this alone does not recover SN/PDF. Preserve the logical operation's id, but do not promise invoice-style successful repeat or exact storno-338 semantics. |
| Document/taxpayer reads | Read retry decision; the read did not itself issue anything. No call-id generation needed. |
| Credit registration / proforma deletion | Reconcile the particular mutation: current entries / document disappearance. Additive repeats can duplicate amounts; replacement can overwrite intervening state. |
| Receipt email | Receipt existence cannot establish mail delivery; resending can send again. No delivery deduplication implied by `OutcomeClass`. |

State “this exchange” versus an earlier logical-operation send. Incorporate S.RQUERY's documented **last matching document**, not arbitrary older-receipt recovery. Remove `StornoReceipt::call_id`'s categorical “reusing it returns 338” (`receipt.rs:296–298`) unless operation-specific evidence is supplied; creation's 338 rule remains. Apply B.E01's ceiling as documented without inventing a durable counter in the client.

**Acceptance:** compile a real number/order-selector example, keep the same persisted create id on resend, show 338 as duplicate prevention/unresolved original-result recovery; all common error links point to the correct operation row. **Dismissed:** automatic recovery, call-id-only selector, translating 338 into success, local order uniqueness, five unconditional sends. Sources: S.DATE/S.RQUERY/S.RSEND and behavior's in-flight/identity limits.

#### D4 — amount policy is not a universal storage guarantee

**Retain · P3 · amount guidance; includes E.E5b. Existence High:** `src/item.rs:20–26,155–157` and README:213–217 generalize EUR invoice observations; S.AMOUNTS expressly requires HUF receipt integral gross and ≤2-place net/VAT. **Solution High:** precise docs and representable examples suffice. **Live:** EUR independent two-decimal storage and selected HUF invoice tolerance observed (behavior:159–161); no receipt rounding incident, fractional-HUF storage or KWD storage observation.

**Best solution:** separate the calculator's invariant, its chosen currency minor-unit policy and server observations. HUF/Ft receipt **item gross** whole; net/VAT may be fractional to two places; net+VAT=gross exact. `Scale(2)` does not ensure gross is whole. Show explicit `787.40 / 212.60 / 1000` and a minor-unit example. Keep raw `LineItem::new`, `Exact` and scale choices. Scope “stored as sent” to observed cases; do not claim KWD's local three-decimal policy matches server storage.

**Acceptance:** examples compile and represent fractional net/VAT correctly; local calculation example has whole gross/exact sum. Rendered guidance separates local arithmetic from server acceptance. **Dismissed:** silently rounding asserted totals, whole net/VAT requirement, a new receipt calculator/validator, KWD=2 by EUR analogy, treating unknown currency tolerance as a demonstrated arithmetic bug.

| ID / disposition | Existence / solution confidence; live qualification | Best solution and focused acceptance | Evidence and dismissed alternatives |
|---|---|---|---|
| **B.E01 · retain P3 · retry/code-55 wording** | **High / High:** S.ERR says five sends and two signing-failure causes; precise correction is clear. No observed 55 or excessive-retry incident. | Say **five total sends including initial**, stop/escalate after unsuccessful ceiling; code 55 means signing failed, not proven issued. Retain `Unknown` and `is_retryable=true` as potentially transient hint: timestamp access may recover, certificate expiry may require action. Acceptance: no “~5”/“five retries”/guaranteed issued text; no unsafe-write permission. | `src/error.rs:256–270`; S.ERR. No class change, interval invented for 55 or client-global retry counter. Do not extrapolate a precise accounting policy for every reconciliation read beyond the vendor's same-request wording. |
| **B.E02 · retain P3 · evidence provenance** | **High / High** for blanket “verified” wording and omitted number condition on 56. **Medium / High** for the stall provenance correction: the detailed row and CONTEXT conflict, so history is incomplete. No 55/56/credential-code live observation established by the checked record. | Distinguish catalogue meanings, observed 14/73/221/352/463, PHP-backed number-dependent 56. Restore README's number condition. Rephrase timeout explanation as observed stalls plus the conservative fact that a timeout does not establish completion; attach/recover the source for “stalled and issued” before calling it observed. Acceptance: every named observation has provenance, conflicting assertions remain noted rather than rewritten as a disproven event. | `src/error.rs:299,854–857`; `src/client.rs:201–204`; README:229; behavior:152,180–181,255–261; CONTEXT **Issue policy**. No weakened credential/56 handling, timeout change, or assertion that delayed issuance never happened. |
| **B.E03 · retain P3 · receipt query semantics** | **Medium / High:** request call-id interpretation is unestablished, not disproven; neutral wording is accurate. **High / High** for adding now-documented last-match guidance. No live receipt selection evidence. | Query call id is an optional wire identifier with unspecified query effect; recommend none with normal number/order selectors. Order lookup returns the vendor-documented last match, not a selected older record; ordering criterion/SN selection remain open. Acceptance: real query example with no call id, existing optional emission preserved, no unsupported selector. | `src/ops/receipt.rs:342–370,399–409`; S.RQUERY; S.PHP `ReceiptHeader.php:24–29,182–194`. PHP creation label plus omission on query cannot prove ignore/filter/invocation semantics. No field removal or call-id-only lookup. |
| **B.E04 · retain P3 · receipt email promises** | **Medium / High:** per-child merge/list syntax is not specified in checked sources; conservative wire wording is clear. No observed partial-default/multiple-recipient failure. | `None` omits that child. Separately document previous-email resend when details are absent, preserving the **present empty block**. Remove comma-list guarantee; single-recipient, fully supplied first-send example. Partial overrides/empty-string behavior remains unspecified. Acceptance: present empty block, child None versus Some("") and field order remain distinguishable; E.E6 owns semantic presence protection. | `src/ops/receipt.rs:420–456,469–487`; S.RSEND; S.PHP receipt writer discussed in B:41. No required-all-fields type, local merging/cache, recipient splitter, or absent-block equivalence. |

### 4.3 Transport contract and lifecycle

#### E.E2 — document actual HTTP/decoding policy

**Retain · P3 · public contract documentation. Existence High:** `src/wire.rs:129–136,194–200` claims body-before-status, contradicted by `279–305` and the rerun's 200/500 body-only 335 countercase. **Solution High:** align prose/diagnostic wording with deliberate policy. **Live:** no non-2xx body-only Számla Agent failure captured; no basis to change policy.

**Best solution:** state nonblank `szlahu_down`, then error-code header, then non-2xx status, then body, with operation-specific numbered-56 handling. A success-number or unrelated `szlahu_*` header does not bypass status. Remove certainty that proxy/CDN rather than szamlazz.hu produced the HTTP response. Describe `szlahu()` (`231–242`) as a utility for encoded textual headers; numbers/error codes use raw `header()`. Body URL must not be URL-decoded again. Apply same wording to `src/client.rs:40–43` and `src/error.rs:645–655`.

**Acceptance:** body-only error 200/500, error-header 500, success-header-only 500, down header and 56 precedence controls; documentation matches them. **Dismissed:** body-before-status runtime change without operation evidence, arbitrary-header bypass, origin attribution from status, a new URL decoder. S.REPLY explicitly exempts numbers/codes from encoding.

#### E.E3 — session ownership and refresh guidance

**Retain · P3 · transport lifecycle guidance. Existence High:** S.SESSION advises refresh after company/email edits; the injected-client hook lacks refresh/shared-jar ownership guidance (`src/client.rs:124–155,209–246`). Reqwest clones share client/store state. **Solution High:** fresh construction/existing jar hook suffice. **Live:** no wrong-account session incident or edit-refresh failure observed; credential-versus-cookie precedence remains unverified.

**Best solution:** document reuse within one account, separate jars for independently authenticated accounts, and a new default client/fresh injected jar after relevant account changes; cloning is not refreshing. Treat a fresh jar on credential/account changes as the caller's ownership policy, not a vendor-proven key invalidation mechanism. Explain 90-minute **inactivity** expiry consistently, and that no persistence means reauthentication, not lost correctness. A caller needing persistence can supply a cookie provider.

Merge **E.T06 browser clarification** here: native cookie guidance is not proof of direct-service browser feasibility. `src/lib.rs:48–54`, README:192 and native/wasm docs should distinguish transport compilation from CORS/header/cookie accessibility. **Existence Medium** for misleading breadth (not an explicit promise that all cross-origin calls work), **solution High** for qualification; live browser behavior untested. Reqwest 0.13.4 wasm uses Fetch's default credentials unless a per-request include option is set; an injected client alone does not establish that policy. Do not blindly add include: XML credentials may work without cookies and CORS remains external.

**Acceptance:** rendered hook/default/wasm guidance explains clone versus fresh jar and platform limits. A focused two-call loopback test may prove cookie reuse/rotation and distinct-jar isolation through the existing injection hook; label it as such, not proof of vendor account selection or production default-root-store behavior. **Dismissed:** reset/persistence APIs, a 90-minute timer, opaque-jar introspection, browser-support removal or unconditional credential include, asserting XML credentials override a reused cookie.

## 5. Notes and housekeeping — explicit non-defect dispositions

These five do not inflate the 28. They have precise actions where useful; none warrants a standalone functional-defect ticket.

| ID | Judgment, confidence and best action | Acceptance, source, live boundary and rejected expansion |
|---|---|---|
| **D8** | **Housekeeping. High existence / High solution:** stale “37” count; remove it, link current list and explain open type plus HUF/Ft alias. | `src/types.rs:361–390`, S.AMOUNTS; no missing representable currency. Rendered docs avoid frozen inventory. No whitelist/count test, ISO substitution or rounding-policy change. Current inventory is source evidence, not live acceptance of every token. |
| **D10** | **Housekeeping. High / High:** credentials-location generalization is false, built-ins are correct. Say credentials are injected where each operation requires them, XML/PDF queries directly at root. | `src/credentials.rs:45–48`, **also `src/wire.rs:353–355`**, query XML/PDF writers; [query XML schema](https://docs.szamlazz.hu/agent/querying_xml/xml) and PDF request. Acceptance: both broad statements fixed; no writer move. No authentication failure observed. |
| **D11** | **Housekeeping. High / High:** false flags and empty seller container are not absent. Describe false `e_invoice`/`download_pdf`, None optionals, empty seller fields/attachments; header paid=false omits `fizetve`. | `src/ops/invoice.rs:190,523–526,549–563,688–689,749–751,770–779`; S.INLINE. Acceptance: Rust defaults and XML presence separately stated. No paid Option change or claimed server equivalence of false/omission. No runtime failure. |
| **B.E05** (supersedes **E.E5c**) | **Source-conflict note, not retained defect. High confidence in receipt-specific first-party support; Medium that existing unqualified wording needs a caveat; High solution confidence:** keep `automatic_mnb`, add a short source/provenance note if revising receipt docs. | S.PHP `Header/ReceiptHeader.php:61–79` explicitly describes MNB omission; `examples/document/receipt/create_receipt_with_custom_data.php:38–44` independently comments on it (actual example supplies 300.0). General S.AMOUNTS/S.DATE says bank plus rate. `src/types.rs:932–962`, `src/ops/receipt.rs:117–120,189–199,948–959`. Local test proves crate validation/emission only; retain absent-rate MNB and explicit-rate checks. No live receipt rate success/failure. No explicit-rate gate, zero substitution, new receipt rate type or “wholly undocumented” claim. A supported feature need not have a local live test to be supportable. |
| **N.AF** (`afalevon`, no D14) | **Open-unit note. Medium existence confidence** for unsupported specificity, **Low** that percentage is actually false. **High solution confidence** for neutral “VAT-deductibility value, reported integer; unit/range unspecified.” | `src/ops/query_xml.rs:479,982–1016`; S.DATE queried schema/S.BANK, D:423–443. Acceptance: retain integer and make no calculation/unit guarantee. No observed value distribution; a synthetic 50 proves no percentage. No bool/amount reinterpretation or 0–100 gate. |

The buyer temporal wording is already D7's Medium-confidence constituent, not a sixth note. Browser breadth is E.E3's Medium-confidence constituent, not a new capability defect. Simplified template precedence is C1. D13's related small fields are one coherent finding.

## 6. E.E6 — evidence corpus and verification methodology

**Retain · P3 · one evidence/test package. Existence High overall:** actual source inventory and comparator mismatch are inspectable. **Solution High:** dated source provenance and targeted external expectations fix the overclaims. **Live:** these are test/documentation claims, not production failures. Subitem confidence is preserved here; each numeric subitem is stable (`E.E6.1`…`E.E6.6`).

| Subitem | Independent disposition and confidence | One best action / acceptance |
|---|---|---|
| **E.E6.1 — historical source descriptions** | Retain correction, **High / High**. `fixtures/SOURCES.md:36–39,66–67,86–88` is a July acquisition record; current S.REPLY/S.NAV differs. | Keep July facts dated; append September observations and newly acquired fixtures with URL/date/hash. Acceptance: structured storno/credit examples and NAV linkage are represented as new/current sources, not retroactively attributed to July. No erase-and-relabel refresh. |
| **E.E6.2 — receipt-create provenance** | Retain discrepancy annotation, **High** that cached/current files differ, **Low** that the July acquisition was wrong; **High** solution confidence for honest annotation. | `fixtures/SOURCES.md:92–103` attributes direct download; current [download](https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd) lacks cached `torloKod`. Recover original hash/transform record if available; otherwise state historical acquisition mechanism unverified. Acceptance: no invented patch history or accusation based on a changed endpoint. |
| **E.E6.3 — conflicting schemas** | Retain, **High / High** for evidence handling; server authority unresolved. | Replace singular “canonical/normative everywhere” claims at `fixtures/SOURCES.md:121–135` with per-conflict evidence and chosen comparison policy. Preserve group-id/erasure/order fields and all original sources. C1 decides writer order, not the server schema. Acceptance: source snapshots versus local transformations visibly separate. |
| **E.E6.4 — empty-email semantic distinction** | Retain, **High / High**: `tests/upstream.rs:1091–1098,1131,1151–1158` erases empty/omitted; S.RSEND explicitly distinguishes them. | Scope the outline comparator to lossy example comparison. Add an independent structural assertion that `SendReceipt::new` emits exactly one **present empty `emailKuldes`** block. Its assertion must fail if the block is removed, and distinguish a control document with no block from one with an empty block; both `<emailKuldes/>` and paired empty syntax represent presence. Preserve child None/empty distinction. Acceptance is a test of actual generated output/presence, not two equal normalized outlines or a mock delivery. No global XSD gate. |
| **E.E6.5 — live-test claims** | Retain, **High / High**. `tests/live.rs:4,11–14` overstates covered kind/empty-element cases and gives old rate figure. | Describe actual taxpayer/HUF invoice/proforma/appearance coverage; cite current 500 invoices/10 minutes as vendor limit if retaining a number, or link it. A conservative local pacing rule may remain explicitly local. Acceptance: every claimed scenario exists in test code; no live run required. |
| **E.E6.6 — independent expectations** | Retain, **High / High**. An enum round trip cannot reveal a missing code; cached outline cannot reveal missing simpleItems; fake namespace substitution cannot certify NAV 3.0. | Add source-derived code coverage under F2, field/order cases under C1, genuine NAV mixed namespaces under R1/C3, lexical cases under F1/F3/R2. Acceptance: each test can fail for the named omission independently of the implementation's own list. These are attached checks, not a duplicate global suite or a Cartesian matrix mandate. |

**Dismissed package alternatives:** replacing all cached files with downloads, conflating schema pass with live action, counting overlapping suite runs, making a locally edited schema evidence of server acceptance, retrospective allegations about July fetches, and simulating vendor semantics as acceptance proof.

Verified hashes of the existing C acquisitions: EN inline `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4`; HU inline `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a`; download `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498`. Their disagreement is part of acceptance, not noise to normalize away.

## 7. Alias ledger — no omissions or double counting

### Original 17 and new package aliases

| Original/canonical ID | Disposition / owner | Earlier aliases or overlapping residuals |
|---|---|---|
| F1 | Retain §2 | QR-01; receipt F-01; A.F1; E.R03 date part |
| F2 | Retain §2 | W05-01; receipt F-02; B.F2; E.E6.6 code coverage |
| F3 | Retain §2 | QR-04, O4-02; raw 05 comma exclusion overturned; A.F3 |
| C1 | Retain §3 | IR-01; C.C1; E.I01; D's simpleItems/template note |
| C2 | Retain two fields §3 | QR-02; C.C2; E.T12 PDF balance/URL; auxiliary expansion declined explicitly |
| C3 | Retain five fields §3 | O4-03; C.C3; E.R08 business fields |
| R1 | Retain shared completion + taxpayer paths §2 | O4-01; E.R01; A.R1; E.E6.6 genuine NAV layout |
| R2 | Retain §2 | Receipt F-03; A.R2; E.R05 trimming |
| D1 | Retain §4.1 | IR-02; E.I09 actual VAT defect |
| D2 | Retain §4.1 | `eusAfa` semantic omission |
| D3 | Retain expanded operation guide §4.2 | Receipt F-04, W05-02; E.T09/T10; B storno-call-id qualification |
| D4 | Retain §4.2 | Receipt F-05; **E.E5b**, E.I03 minor-unit claim, E.R13 amount guidance |
| D5 | Retain §4.1 | QR-03 |
| D6 | Retain §4.1 | O4-04; behavior's probe D3 is evidence, not review finding D3 |
| D7 | Retain with temporal subitem §4.1 | IR-04; E.R06 buyer; behavior's probe D6 is evidence, not review finding D6 |
| D8 | Housekeeping §5 | E.I10 currency inventory |
| D9 | Retain §4.1 | IR-06 |
| D10 | Housekeeping §5 | E.T08 credentials prose; includes wire trait documentation |
| D11 | Housekeeping §5 | **E.E5h default part**, E.I02 |
| D12 | Retain §4.1 | **E.E5g**, E.I02 reference provenance |
| D13 | Retain §4.1 | **E.E5h waybill/template part**, E.I02; barcode/destination/Default constituents |
| B.E01 | Retain §4.2 | **E.E5f ceiling/55 part**, E.T09 |
| B.E02 | Retain §4.2 | **E.E5f observed/56 part**, plus timeout provenance |
| B.E03 | Retain §4.2 | **E.E5d**, E.R10 query meaning; E.R12 last-match rule |
| B.E04 | Retain §4.2 | **E.E5e**, E.R11 |
| B.E05 | Note §5 | **E.E5c** superseded by receipt-specific PHP evidence; E.I04 |
| E.E1 | Retain §2 | E.T01; raw WE exact-cookie note |
| E.E2 | Retain §4.3 | E.T02/T03 |
| E.E3 | Retain §4.3 | E.T04 plus E.T06 qualification; E.T05 stays open |
| E.E4 | Retain §3 | E.T11 |
| E.E5 | **Aggregate only, not another count** | a → E.E5a; b → D4; c → B.E05 note; d → B.E03; e → B.E04; f → B.E01/B.E02; g → D12; h → D11/D13 |
| E.E5a | Retain §2 | E.V01; helper + worker are one mechanism/decision finding |
| E.E6 | Retain one package §6 | E.E6.1–6; E.I01, E.V04/V05; checks delegated to their implementation owner |
| N.AF | Open-unit note §5 | D's unnumbered afalevon note; E.R06 |

C's proposed PDF auxiliary additions have **no new retained ID**; their explicit declined scope is C2/E.T12. B's “judging pass” repeats B's IDs and adds no findings. Confidence for every E.E5 constituent is the canonical entry's confidence, not an inherited blanket “High” from the aggregate heading.

### All 44 residual-register rows

`E.T…`, `E.I…`, `E.R…`, `E.V…` refer to [E's register](05-scope-challenge.md#complete-residual-disposition-register), which preserves raw-report line origins. **M** = merged into a retained/housekeeping owner above; **N** = accepted boundary/no change; **Q** = open behavior, no demonstrated defect. A Q is not a live-backed exclusion. When a row mixes subjects the disposition names each part; no row receives a blanket dismissal.

| Row | Final disposition and closure boundary |
|---|---|
| **E.T01** | **M E.E1.** Exact-name fix justified by public helper inputs; vendor emission frequency unnecessary to establish it. |
| **E.T02** | **M E.E2; Q runtime precedence.** Change prose now. Runtime change needs complete status/headers/body capture of an Agent non-2xx body-only answer or an explicit Agent guarantee, not direct NAV behavior. |
| **E.T03** | **M E.E2; N decoder redesign.** Numbers/codes raw, textual headers decoded, body URL not decoded twice. New escaping rules need raw operation-specific header evidence. |
| **E.T04** | **M E.E3; N reset/disk persistence API.** Existing fresh client/custom jar suffices. |
| **E.T05** | **Q.** Shared jar is concrete; account selection when key/cookie disagree and key deletion's session invalidation are unknown. Closure: vendor contract or separately authorized two-account/rotation evidence. No observed wrong-account claim. |
| **E.T06** | **M E.E3 browser qualification; Q direct browser feasibility.** Need actual origin POST/readable body/exposed headers/credentialed cookie+CORS evidence. Default Fetch same-origin alone does not prove all calls fail. No blind include patch. |
| **E.T07** | **N.** Native timeout/no redirects/custom TLS/endpoint policy intentional; diagnostic excerpts are bounded, not promised secret scrubbing. **M B.E02** only for timeout observation provenance. No mandated timeout, TLS defect or vulnerability inferred from mocks. |
| **E.T08** | **N credential representations; M D10 prose.** Opaque key/lowercase caller responsibility and legacy forms remain representable. Query credential placement already correct. |
| **E.T09** | **M B.E01/B.E02/D3.** Keep numbered-56 success and unnumbered unknown; 55 is uncertain signing failure. Five-send wording corrected without new retry engine. |
| **E.T10** | **M D3.** Mutation/mail recovery cannot use generic invoice existence; 335/338/7 are exchange/operation-dependent. No blanket classifier redesign. |
| **E.T11** | **M E.E4.** Create/storno payment method optional exposure accepted. |
| **E.T12** | **N explicit projection boundary; M C2/C3 for accepted fields only.** Decline PDF id/method/notification flag, credit-balance id, NAV header/software/success diagnostics. Reopen on concrete consumer need plus operation source/capture; not every parsed field must be public. |
| **E.T13** | **N.** Five attachment names and conservative size bound supported; accepted multipart is not delivery proof. A per-file result API needs an actual upstream result channel, not invented success data. |
| **E.T14** | **N.** v2-only deliberate; unexpected v1/HTML not a missing normal parser. Four selector writers pin 2; other operations have no selector. Custom request remains the extension boundary. |
| **E.I01** | **M C1/E.E6.3; Q server combined order.** PHP/download preview-first chosen; inline conflict retained. No locally canonicalized schema as closure. |
| **E.I02** | **M D11/D12/D13.** Defaults/omission, reference attribution, waybill/layout are precise prose corrections. |
| **E.I03** | **M D4; Q storage and paid=false.** Keep local minor-unit table. Explicit false versus omission needs controlled request/queried-paid-state evidence under payment methods/defaults before `Option<bool>` or an override. XSD absence/default silence proves no equivalence. |
| **E.I04** | **N foreign-rate gate; M B.E05 note.** Receipt MNB has explicit receipt-specific PHP support; older “only invoice” premise overturned. Foreign AAM/proforma/delivery-note omissions remain Q; relaxing those gates needs operation-specific evidence. No receipt explicit-rate restriction. |
| **E.I05** | **N date/appearance/reference request surface; Q broader server acceptance.** Live/e-invoice backdating and explicit ES/VS proforma links need their own create/query evidence. Implicit ES consumption is insufficient. |
| **E.I06** | **N.** Nonunique/newest external ids, no echo, attachment only on creation and SS-owned storno id are account-bounded observations. No general request uniqueness/length validator; worker bounds are separate. |
| **E.I07** | **N.** Raw lines express gross-first amounts; caller supplies prepayment deduction; special VAT-derived zero/open-token behavior deliberate. No automatic netting or new wire feature. |
| **E.I08** | **N plain-data boundary; Q account-dependent options.** Aggregator/guardian/logo/payable-adjustment/margin-VAT/rendering interactions require field-specific rules/output evidence, not generalized local validation. |
| **E.I09** | **N translations/tokens already correct; M D1** for actual TAHK defect. Keep KBAUK primary-source interpretation, fulfillment terminology, SzlaNoEnv, exact szamlaSablon casing. Default-vs-omission clarification is D13. |
| **E.I10** | **N representations; M D8** stale count. No missing seller-identity request block/carrier sub-block, weight coercion, parcel-bound change or download-copies feature established; text fields already hold dynamic email tags/languages/open values. |
| **E.I11** | **Q.** Test-account routing and inability to trigger 56 prove no production/receipt/storno delivery behavior. Closure needs separately authorized actual delivery evidence, not a new guessed request field. |
| **E.R01** | **M R1.** Shared tail completion retained; taxpayer descendant path/expanded-name collisions retained. No global descendant schema enforcement inferred for serde models. |
| **E.R02** | **N current blank verdict/contradictory success-code policy.** Empty verdict is false/Absent, missing verdict fails. Other header/body contradictions lack a vendor tie-breaker; no new false-success incident. Any tightening needs its own exact policy controls. |
| **E.R03** | **N sparse/finite-domain choices; M F1/R2** lexical/text defects. Keep empty lists, repeated labels, wider signed ids, future tokens, finite Decimal and civil dates. No NaN/INF/ancient-date expansion or unnecessary new year-zero refusal. |
| **E.R04** | **N.** Optional PDF on ordinary replies, required PDF on PDF-query/preview and numbered-56 softness intentional. Abbreviated source PDF is not valid base64 evidence; no signature checking promise. Parse failure after numbered write does not prove nothing landed. |
| **E.R05** | **M R2** optional text; **N** absent response fields not in schema (queried waybill/attachments/erasure/contact input/outstanding/external-id). Preserve simple-address-detail compatibility only at the proper taxpayer address path; no global local-name matching. |
| **E.R06** | **M D7** qualified buyer mutability; **M N.AF** unverified unit. No universal “all buyer fields are master data” or percentage gate. |
| **E.R07** | **N open JS/TEHK/appearance/source values; Q operation availability.** Shared-schema superset does not prove emission on XML query. Named variants/frequency claims need useful operation evidence. |
| **E.R08** | **N direct GeneralErrorResponse; M C3.** Agent promises wrapped QueryTaxpayerResponse. Direct NAV error roots require Agent forwarding evidence. infoDate already has Agent example evidence; others' current forwarding remains open. |
| **E.R09** | **N.** OK requires explicit validity; false is normal data. Relaxation requires a legitimate complete OK-without-validity answer and its specified meaning. |
| **E.R10** | **M B.E03/D3; Q identity scope.** Creation duplicate prevention supported; query id role, storno 338, uniqueness scope/retention/cross-operation collisions unresolved. Need vendor semantics or separately authorized per-operation matrix; no auto-generated ids/call-id selector. |
| **E.R11** | **M B.E04/E.E6.4; N current resend representation.** Empty-present sends previous email; absent block does not. No create-email field or richer send result established. |
| **E.R12** | **Partly settled by S.RQUERY: last matching receipt documented; M B.E03. Q** last-by-id/date and selection after reversal remain. **N** amount aliases, schema `all` order, receipt-specific repetition toggle, open TEHK. Copied NY reversal-reference/unbalanced format examples are not accounting executions. |
| **E.R13** | **N runtime guards/calculators; M D4 guidance.** Unsupported receipt-only incompatibilities refused explicitly; arithmetic/tender/prefix rules delegated; fractional net/VAT representable. Empty replacing credits stays refused; repeated receipt call is not recovered original success. |
| **E.V01** | **M E.E5a; Q negative-original live outcome.** Missing optional gross already warrants correcting false→no-op. Actual negative-original evidence remains separate, not a prerequisite to the bounded decision repair. |
| **E.V02** | **Q, account-bound matrix retained:** zero-total storno; HS/ES/VS and settled ES storno; HS replay; final reissue; sixth-entry server code; SS credit; incoming-credit issuer matching; empty replacing credits; receipt operations generally. Closure requires the specific request/complete HTTP answer/queried resulting state, not constructors or fabricated responses. No operations authorized by this report. |
| **E.V03** | **Q, no agent change:** two-day replay/fingerprint remainder; last-by-id/date; UI conversion references/order; internal whitespace/NFC; external-id limit beyond 110; second D after consumption; seller-id stability/source values; cross-account by-number behavior. Keep existing bounded observations/worker protections, not a general conformance claim. Each stronger guarantee needs its own before/after evidence. |
| **E.V04** | **M E.E6; N count-inflation allegation.** Prior report already avoided summing overlapping suites. Six-kind request schema experiments do not certify receipt/other schema validation, browser/TLS/delivery or all lexical forms. |
| **E.V05** | **M E.E6.1–3.** Date source drift, preserve seller text/abbreviated-PDF provenance; current endpoint difference does not prove historical acquisition error. |
| **E.V06** | **Verification backlog, not six defects:** cookie capture/reuse/rotation/separate jars → E.E1/E.E3; status/body → E.E2; non-56 body versus header56 → F3/R1 envelope controls; multipart boundary collision across XML/attachment remains a targeted transport check; `%2B`/`+`, Unicode header decoding/body URL controls → E.E2/E.E4/C2. No current failure claimed for unexecuted cases. Test the actual contract as its owner changes it; do not fabricate live semantics. |

## 8. Ordered implementation packages

This order balances impact and dependencies. Documentation packages can proceed alongside code; E.E6's provenance rules apply from the first changed fixture. The 28 findings remain individually traceable through these packages.

| Order | Package | Actionable boundary / done when |
|---:|---|---|
| **P01** | **Invoice/domain guidance** — D1, D2, D5, D6, D7, D9, D12, D13; D8/D10/D11/N.AF housekeeping | Fix consequential VAT guidance first, then the coherent field-semantics sweep. Render rustdoc/check links and existing examples; no comment-string tests or runtime business gates. |
| **P02** | **Response lexical interoperability** — F1, F3 | Private date/header adapters, all affected paths, source-derived positive/negative controls and compatibility policy. Full body/error/56 precedence preserved. |
| **P03** | **Known refusals** — F2 | Thirteen explicit names/classes and independent catalogue tests; exchange-vs-logical-history wording. No need to wait for simpleItems. |
| **P04** | **Storno evidence decision** — E.E5a | Agent heuristic guidance plus separately owned worker follow-up for inconclusive changed-number replies and query-backed identity. Review durable boundary; no false settled no-op after missing metadata/unanswered verify. |
| **P05** | **Simplified-image request** — C1 | Optional field in next breaking request release, deterministic PHP/download order, original source conflict retained; no auto-order retry or preview loss. |
| **P06** | **Response boundaries and text** — R1, R2 | Shared completion, scoped NAV projection, genuine version fixtures and decoded-text policy. Keep full-validator scope out. |
| **P07** | **Document response exposure** — C2, E.E4 | PDF two fields, create/storno payment method, shared helper, old JSON compatibility and unchanged PDF invariant. No auxiliary PDF additions. |
| **P08** | **Taxpayer business record** — C3 | Add five optional fields on P06's path-aware reader; info_date source string and open incorporation. Worker/CLI projection expansion not implicit. |
| **P09** | **Transport helper/contract** — E.E1, E.E2, E.E3 | Exact cookie pair, accurate precedence/decoding and jar/browser guidance, focused loopback/policy checks where meaningful. |
| **P10** | **Recovery/receipt guidance** — D3, D4, B.E01–B.E04; B.E05 note | One operation recovery table, amount-policy boundaries, precise retry/provenance and receipt selectors/email wording. Supported MNB remains. |
| **P11** | **Evidence corpus closure** — E.E6 | Dated originals/transforms, honest acquisition unknowns, meaningful empty-email assertion, corrected live-test descriptions and implementation-owned external expectation coverage. |

**Release effects:** C1 breaks exhaustive request literals/destructures and needs the appropriate release. C2/C3/E.E4 add fields to already non-exhaustive responses; old JSON should decode through defaults, serialized output grows. F2 deliberately changes formerly unknown classifications. R2 changes returned characters; F3's grammar rejects previously accepted underscores/extra whitespace. R1 deliberately refuses malformed/ambiguous shapes, not sparse legitimate records. State these concrete effects rather than presenting all work as non-breaking cleanup.

**Completion standard:** implement the focused acceptance for each retained finding, run the affected agent suites/doc checks, and for P04 the worker's decision/gateway and any affected durable tests. Passing those does not close the explicitly listed vendor behavior questions. The review itself ran only the offline probes above and public evidence checks; none of these implementation packages is claimed completed by writing this judgment.
