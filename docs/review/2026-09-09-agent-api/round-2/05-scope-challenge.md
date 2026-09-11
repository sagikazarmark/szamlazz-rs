# Reviewer E — residual inventory and scope challenge

**2026-09-09 · independently reviewed code baseline:** `a804c740eb8446211c1cdca3eea4fb93d298d25d`.

## Decision for the judge

The seventeen main findings are **not the complete actionable inventory**. Promote the exact-cookie-name defect, the HTTP interpretation documentation defect, session-lifecycle guidance, the documented create/storno payment-method exposure gap, and the specific evidence/coverage corrections below. Most other exclusions stand, but several stand only as **unverified behavior**, not as live-backed intentional deviations.

There is **no new established P0/P1 runtime defect**. In particular, a positive-gross reversal of a negative original remains a conditional failure, not an observed one. Its public helper guidance nevertheless needs qualification now. Similarly, browser transport compilation is not evidence that direct browser access to Számla Agent works.

This report gives every meaningful residual note in all five raw reports an explicit disposition, grouping closely related notes. Main findings F1–F3/C1–C3/R1–R2/D1–D9 retain their own ownership; expansions are identified rather than counted twice. Priority means work priority: P2 normal consequential work; P3 narrow robustness, coverage, or documentation. **Existence confidence and solution confidence are stated separately.**

### Additional actionable packages

| ID | Priority / kind | Existence confidence | Recommended solution / confidence |
|---|---|---|---|
| E1 | P3 helper correctness | High: independently reproduced | Parse the cookie pair and match exactly `JSESSIONID`; high |
| E2 | P3 public contract documentation | High: code, fresh header documentation, independent counter-case | Describe actual error/down-header → status → body precedence and header-specific decoding; high |
| E3 | P3 session lifecycle documentation | High: fresh vendor refresh instruction is absent from the client guidance; shared-jar behavior is explicit in dependency code | Document refresh by a fresh jar and per-account ownership of an injected jar; high. No reset/persistence API required |
| E4 | P3 optional capability | High: create/storno payment-method header is documented and dropped | Add optional `payment_method` to `CreatedInvoice`, using the existing header-reader policy; high for exposure, medium for exact encoding beyond present evidence |
| E5 | P3 evidence-qualified public guidance | High for the identified overclaims; server counter-behavior unverified | Correct each named claim to its actual evidence boundary; high. No speculative serializer or rounding change |
| E6 | P3 corpus / verification methodology | High for current source/coverage mismatch; historical receipt provenance unresolved | Date the source observations, retain source conflicts and transformations, correct test claims and add source-derived checks where needed; high |

E5 and E6 are **work packages with enumerated subitems**, not a claim that every documentation sentence is one independent functional defect. The inventory below gives those subitems stable references for adjudication.

## Evidence and verification

Read **every line** of `raw/01-invoice-requests.md` (IR), `raw/02-query-responses.md` (QR), `raw/03-receipts.md` (RC), `raw/04-other-operations.md` (OO), `raw/05-wire-errors.md` (WE), plus `REVIEW.md` (RV) and `ADJUDICATION.md` (AD). Read the relevant implementations, the full client integration test, the request-outline comparator, live-test introduction/scenarios, fixture provenance, and the behavior record's relevant observations and open questions. Citations to source below are relative to `crates/szamlazz-agent/src/` unless otherwise stated; raw-report citations use their exact line numbers.

Fresh public GETs in **this round**:

| Key | Primary source / section checked |
|---|---|
| S1 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie), step-by-step and Recommended procedures: no cookie means reauthentication; 90-minute **inactivity** expiry; renew after company/email changes |
| S2 | [Invoice response](https://docs.szamlazz.hu/agent/generating_invoice/response), header table and inline XSD |
| S3 | [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response), header table, structured examples and inline XSD |
| S4 | [Credit-entry response](https://docs.szamlazz.hu/agent/credit_entry/response), structured examples, header table and inline XSD |
| S5 | [Taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response), wrapper guarantee, `infoDate` example and linked NAV specification |
| S6 | [Receipt query XML](https://docs.szamlazz.hu/agent/querying_receipt/xml), selector prose and inline XSD |
| S7 | [Receipt-send XML](https://docs.szamlazz.hu/agent/sending_receipt/xml), email detail/block distinction |
| S8 | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling), five-send ceiling and code 55 |
| S9 | [Receipt-create downloadable XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd), absent `torloKod` |
| S10 | [RFC 6265 §§4.1.1, 5.2](https://www.rfc-editor.org/rfc/rfc6265.html#section-5.2), name/value split, `=` requirement, whitespace handling |

Vendor pages above reported `v202608271632`. Other source comparisons are attributed to the raw reports rather than presented as fresh re-fetches by E. For browser behavior, inspected **reqwest 0.13.4 source** in the local Cargo registry: `src/wasm/request.rs:27,48,349–393` and `src/wasm/client.rs:222–223`. Its default leaves Fetch credentials unset; include is a **request-builder** option. Cross-checked [MDN Request.credentials](https://developer.mozilla.org/en-US/docs/Web/API/Request/credentials): default `same-origin`, affecting both sending cookies and honoring `Set-Cookie`. This is platform documentation, not a live Számla Agent observation.

One new offline scratch consumer, `/tmp/opencode/scope-challenge-e`, was built and executed:

```sh
cargo run --offline --quiet --manifest-path /tmp/opencode/scope-challenge-e/Cargo.toml
```

It imports the checkout without `client-reqwest` and performs no networking. Results:

| Input | Observed result |
|---|---|
| `Set-Cookie: JSESSIONIDOTHER=wrong; Path=/`, then `JSESSIONID=right; Path=/` | `session_cookie() == Some("JSESSIONIDOTHER=wrong")` |
| Only `JSESSIONIDOTHER=wrong; Path=/` | Same incorrect `Some` |
| Only `JSESSIONID; Path=/` | `Some("JSESSIONID")`, not a cookie pair |
| Only `JSESSIONID=right; Path=/` | Correct `Some("JSESSIONID=right")` |
| Complete body-only deletion error 335, HTTP 200 versus HTTP 500 | `Api(ProformaNotFound)` versus `HttpStatus(500)` |
| Numbered storno success `SS-2`, original `ORIGINAL-1`, gross −127 / 0 / +127 | `reverses` true / true / false |
| Deletion `<sikeres/>` | `Api(Absent)`, never success |

The last two rows establish implementation behavior only. No negative-original issuance, session authentication precedence, browser CORS, or live cookie-name collision was tested. No full test suite was rerun; previous passing counts are not new E verification. No account calls, production edits or subagents. Existing concurrent work was inspected only where necessary and not modified.

## Promoted findings and best bounded solutions

### E1 — Match the cookie's name, not a prefix

**Code:** `wire.rs:308–325`; prior note WE:76. The documented method returns **the `JSESSIONID` cookie**, but `starts_with("JSESSIONID")` selects another valid cookie name and prevents a later correct cookie from being selected. An unrelated cookie in a response is legal HTTP, not malformed Számla Agent XML. The bare-name case also violates the returned `Cookie`-header-value contract.

**Impact:** callers of the sans-I/O helper can miss session reuse or send a malformed cookie value. Native bundled clients are unaffected: they use reqwest's jar. No account mix-up or live outage is established. Lack of evidence that the vendor currently emits the competing name limits frequency, **not the existence of a public helper defect**.

**Solution:** for each repeated `Set-Cookie`, isolate the first semicolon-delimited pair, require `split_once('=')`, compare the name exactly and case-sensitively to `JSESSIONID`, and preserve its value. If allowing HTTP whitespace, trim only SP/HTAB around name/value as in S10. Continue past malformed/nonmatching pairs. Test competing name before the real cookie, only competing name, bare name, valid pair, and mixed-case HTTP header names. Do not turn the helper into a cookie jar: it lacks request URL, expiry and path context. State that attribute/lifetime handling belongs to a transport needing full cookie semantics.

### E2 — HTTP status and decoding prose contradict the implementation

**Code:** `wire.rs:129–136,194–200,231–242,279–305`; `client.rs:40–43`; `error.rs:645–655` (`ResponseError::HttpStatus`). Prior notes QR:277, OO:350, WE:128,271; RV:139 compresses this into an unranked sentence.

The code checks nonblank `szlahu_down`, then `szlahu_error_code`, then non-2xx, **then** XML. It does not examine body-only answers before status; nor does any arbitrary `szlahu_*` header bypass status. A success-number header is insufficient. The scratch 335 counter-case confirms that. Further, status plus missing error headers cannot prove that “a proxy … spoke, not szamlazz.hu”; that is a diagnostic hypothesis, not an authenticated origin determination.

`RawResponse::szlahu` also says szamlazz.hu URL-encodes the headers generically while S2 explicitly excludes totals and error codes. Built-in numeric readers correctly use `header()`, so the problem is guidance to custom transports, not an additional monetary-parser defect.

**Solution:** align all public descriptions and the HTTP diagnostic wording with the actual recognized-header policy. Describe `szlahu()` as a decoding utility **for encoded textual headers**; direct callers to `header()` for numbers/codes. Keep the policy until evidence warrants changing it. A future non-2xx interoperability ticket needs a captured status + all headers + complete body from Számla Agent showing the body-only answer, or an explicit vendor guarantee. Direct NAV's status table does not supply it. Test the policy using body-only 200/500, error-header 500 and success-header-only 500; preserve the existing 56 exception.

### E3 — Document session refresh and injected-client ownership at the actual hook

**Code:** `client.rs:124–155,209–246`; dependency `reqwest-0.13.4/src/async_impl/client.rs:93–94,1189–1193,1213–1218`. Prior notes WE:74–75,261,269 and OO:107.

S1 explicitly advises a **new cookie after account detail edits**. Current `http_client` guidance only encourages enabling cookies; there is no refresh guidance or statement that cloning an injected HTTP client shares its jar across separately credentialed Számla Agent clients. The dependency holds `Arc<ClientRef>` and an `Arc` cookie store, so this is concrete transport ownership information an embedder needs. XML credentials overriding an existing session is **unverified**; do not imply they do.

**Solution:** document reuse within one account, a distinct jar for independently authenticated accounts, and replacement by a newly built default client/fresh injected jar after relevant edits or credential changes. **Cloning is not refreshing**. Preserve the hook and do not pretend the library can inspect an opaque client's jar or stop all sharing. A host-managed persistent jar is already possible through reqwest's `cookie_provider`; absent default persistence costs reauthentication according to S1, not correctness. No mandatory disk store/reset method or automatic 90-minute timer follows: expiry is inactivity-based and the server renews a missing/expired session.

**Verification gap:** `tests/client.rs:18–38` deliberately injects a no-root-store client; its six tests contain no two-call cookie exchange. A local two-call test can establish capture/reuse/rotation, and two separate jars can establish no cookie propagation. It cannot establish which account the vendor selects when XML credentials disagree with a reused cookie. Such an account-selection claim needs vendor confirmation or separately authorized two-account evidence.

### E4 — Create/storno payment method is a documented optional exposure gap

**Code:** `ops/envelope.rs:27–54,205–229`; compare `ops/credit_entry.rs:195–210,281–286`. Prior notes WE:263, OO:198,344; AD:84 calls auxiliary metadata a separate scope choice.

S2 and S3 explicitly list **`szlahu_fizetesmod`**. `CreatedInvoice` retains several optional headers but discards this one; credit-entry results expose it. Calling it “header metadata” does not make it less documented than the body metadata promoted as C2. This is **P3 optional capability**, not a parsing failure, mandatory vendor requirement, or evidence every response emits it.

**Preferred solution:** expose `Option<PaymentMethod>` on `CreatedInvoice`, share the existing header extraction with `InvoiceBalance`, retain unknown tokens, and test present/absent values. Keep decoding consistent with the current reader and record any unresolved exact encoding. Do not invent a payment-method body element: the XSD has none, despite S2/S3's broad “same data” sentence. If the judge chooses a deliberately narrow result instead, record this exact omission as an accepted scope decision rather than implying complete response exposure. A custom `AgentRequest` wrapper can retain raw headers through `Client::send`; the built-in result cannot.

### E5 — Unsupported certainty is actionable even when runtime behavior remains open

| Subitem | Exact claim and evidence | Recommended correction / confidence |
|---|---|---|
| E5a | `ops/envelope.rs:56–70`: `reverses` is “the check every caller must make”; zero-total storno described as fact. `docs/szamlazz-hu-behaviour.md:79,87,176–179,224–225` bounds observations to positive originals, leaves zero/HS/ES/VS open. Scratch positive-gross success fails the heuristic | **High-confidence overgeneralization.** Describe an observed heuristic with an inconclusive false result, not proof of no reversal. Explicitly distinguish a same-number echo from a changed-number reply with missing/positive gross. Runtime remediation and evidence below |
| E5b | `README.md:213–217`, `item.rs:155–157`: minor-unit rounding reconciles to storage; README says “what is sent is what szamlazz.hu stores.” `types.rs:403–424` chooses KWD=3, while live storage evidence is EUR/HUF only; behavior:249–254 leaves other scales/tolerance open | **High-confidence evidence overreach, not confirmed KWD arithmetic defect.** State the exact on-wire invariant and local ISO/HUF policy separately from observed server storage. Do not switch KWD to two places on an EUR analogy. Expand D4's documentation work rather than count another calculator bug |
| E5c | `types.rs:932–942,955–957`, `receipt.rs:948–959` present receipt automatic MNB as supported. RC:262–266 found the actual defaulting annotation only on invoices; foreign receipt prose requires bank/rate | **High-confidence unsupported promise.** Qualify shared rustdoc and rename/reword the test as serialization acceptance, not vendor validation. Keep wire capability pending receipt-specific evidence; require an explicit rate in examples that need demonstrated receipt behavior |
| E5d | `receipt.rs:368` says query call id is “as supplied at creation”; S6 only calls it optional unique call identifier | **High confidence the current attribution is unestablished, not that the opposite is true.** Use neutral protocol wording. Ask which operation it identifies and precedence alongside number/order; do not add call-id-only lookup on XSD optionality alone |
| E5e | `receipt.rs:422–428` guarantees per-field fallback and comma-separated recipients; S7 only establishes previous-email fallback when details are unspecified | **High confidence of evidence gap; medium solution confidence on eventual semantics.** Qualify these two promises now; retain empty-present block resend behavior. Need first-send/resend, each partial override, empty value and actual multiple-recipient delivery evidence to restore stronger wording |
| E5f | `error.rs:256–264,299,854–857`: “~5”, 55 means “issued”, and named-code table “observed”. S8 says **five total sends**, signing may fail due to expiry or timestamp access; behavior:180–181,255–261 explicitly leaves 56/credentials unobserved | **High-confidence documentation defect.** Say five sends including initial send, retain uncertain issuance for 55, distinguish potentially transient timestamp failure from certificate remediation, and label documented / first-party implementation / live evidence separately. Fold receipt and other-write recovery into D3, not a new code-class change |
| E5g | `invoice.rs:27–29`: XSD permits the proforma reference on exactly three kinds. IR:134 and independent optional flag/reference declarations show no XSD choice/restriction encoding that | **High-confidence source attribution defect.** Call the three-kind restriction the crate's domain model, not the XSD's. Keep the model |
| E5h | `invoice.rs:523–525` calls required false flags “absent”; `ops/waybill.rs:1–3` restricts need to delivery-note use despite no such writer restriction; delivery-note template override at `invoice.rs:758–765` | **High-confidence clarity corrections.** Say explicit false defaults; describe waybill use with a compatible layout on supported invoice requests; disclose forced delivery-note template. No writer change |

**Storno resolution (E5a):** the actual worker call site is `crates/restate-szamlazz/src/gateway.rs:1607–1615`: every successful reply failing the helper becomes `NotStornoable` and is logged as a no-op. A positive-gross changed-number reply would therefore be misreported *if* the service emits a genuine negative-original reversal; the agent parser itself returns the numbered reply successfully. The current storno webpage's positive-total sample is a copied invoice-shaped format example with abbreviated PDF, **not proof** that this live case exists.

The best stronger design, if pursued, is **query verification of the returned number**, requiring `tipus=SS` and `hivszamlaszam=original` (the existing worker `FoundDocument::is_storno_of` already expresses that question), with unanswered verification remaining unknown rather than settled no-op. A sign comparison with the original is at most a consistency check; number-change alone is not identity proof. Do not make the sans-I/O helper perform I/O. To promote the conditional runtime bug to confirmed live interoperability failure, obtain a captured negative original and accepted storno, then query both the original's reversal marker and the returned document's type/reference, separating refusal codes 14/221. A missing optional gross also yields false today; it is a further reason false must mean **not established**, not evidence that gross may always be ignored. No new live calls are authorized here.

### E6 — Corpus provenance and verification claims need precise repair

1. **Historical observations are not stale facts to erase.** `fixtures/SOURCES.md:36–39` dates the corpus to July. Statements at `66–67` (“only response example”) and `86–88` (no taxpayer schema block) can remain as **dated acquisition facts**; S3–S5 now offer structured examples/NAV linkage. Add current source observations and new fixtures with their own acquisition dates. Do not retroactively claim the July copy was fetched in September.
2. **Receipt-create provenance is unresolved, not proven fraudulent.** `fixtures/SOURCES.md:92–103` attributes it to a direct download; cached `xmlnyugtacreate.xsd:41–47` has `torloKod`, S9 does not. `git log --oneline -- fixtures/upstream/agent/xsd/xmlnyugtacreate.xsd` shows only initial commit `a3342f0`. That does **not** establish whether July's endpoint differed or the initial acquisition included an undocumented transformation. Record the discrepancy and recover the original fetch/hash or transformation record if available; otherwise say original mechanism unverified. Do not invent a patch history.
3. **No singular canonical current schema.** `fixtures/SOURCES.md:121–135` calls downloads canonical then chooses inline where they disagree. IR:328–350 establishes the invoice header-order conflict; RC:60–82,280–285 adds receipt omissions, TEHK and broken links. Keep snapshots separately, with URL/date/hash and explicit local transformations/precedence per conflict. Do not overwrite invoice group-id/erasure fields or receipt order/erasure fields from downloads. A chosen comparison baseline is not proof of the production server's validation schema.
4. **The outline comparator has a false universal premise.** `tests/upstream.rs:1091–1098,1131,1151–1158` erases empty elements and edge whitespace and says empty/omitted are the same request. S7 explicitly distinguishes absent `emailKuldes` (no send) from a present empty resend block. This is a **concrete counterexample within the reviewed operations**, not a generic concern. Preserve the lossy comparator only as an explicitly scoped example comparison; add a focused presence assertion for the meaningful block distinction. Do not infer protocol equivalence from the erased outline.
5. **Live-test scope is overstated.** `tests/live.rs:11–14` claims rejected kind combinations and empty/omitted handling, while the actual suite is taxpayer lookup, HUF invoice/proforma lifecycles and appearance cases. Its older test rate-limit comment (`:4`) says 100/hour, whereas S8 now says maximum 500/10 minutes. The old lower number is conservative, but not the current vendor limit. Correct these descriptions; no live run is needed to verify what the test code exercises.
6. **Tests need an external expectation, not just more examples.** The named-code round-trip registry cannot discover a missing vendor code (WE:218). A cached request outline cannot discover `simpleItems`; a fabricated all-section response cannot establish every lexical spelling or semantic meaning. Add source-derived catalogue/schema-path coverage at the relevant implementation work, preserving explicit deviations. Use a real NAV 3.0 namespace mix, not merely `/2.0/` replacement. This is verification work, not a demand to count every Cartesian combination as a separate test.

## Complete residual disposition register

**Codes:** **P** promoted above; **M** already owned by a main finding (possibly expanded); **N** no implementation change warranted; **Q** unresolved behavior with precise closure evidence. “Q” is not a live-backed exclusion. Rows map the meaningful residual sections/notes of every raw report; closely related negative results share a row.

### Transport, errors, sessions and optional envelope data

| ID | Origin / residual | Disposition and what the judge should retain |
|---|---|---|
| T01 | WE:76 exact cookie name | **P E1.** Public contract mismatch independently reproduced; no live collision needed for this narrow fix |
| T02 | QR:277; OO:350; WE:120–128 status/body mismatch | **P E2** for prose; **Q** for changing precedence. Need actual non-2xx body-only Agent error with complete HTTP context, not NAV's direct protocol |
| T03 | WE:271 encoded headers; WE:118 URL escaping | **P E2** generic encoding prose. **N** built-in numeric/raw versus textual decoding; do not double-decode body URLs. Exact additional textual escaping needs raw headers if changed |
| T04 | WE:74–75,261 reset/persistence; OO:107 | **P E3** lifecycle guidance. **N** mandatory reset/disk persistence API; fresh construction and custom jar suffice |
| T05 | WE:269–270 session/key invalidation and credentials precedence | **Q.** Native jar sharing is real; account selection and deletion invalidating an existing session need vendor guarantee or two-account/key-rotation evidence. Do not label a wrong-account incident observed |
| T06 | WE:78,274 browser; `lib.rs:48–54`, README:192 | **Q / documentation clarification.** Advertise compile/transport availability separately from direct-service browser feasibility. Default Fetch same-origin means cross-origin session reuse is not enabled by this shell, and passing a different reqwest client does not set the per-request include option. Need browser-origin POST/readability, exposed `szlahu_*` headers, credentialed-cookie behavior and CORS evidence. No blind include patch: no-cookie calls may still authenticate by XML; browser cookie policy/CORS remain external |
| T07 | WE:77–79 timeout, retries, redirects, TLS, diagnostic secrecy | **N.** Default native 60s/no redirects, explicit endpoint/custom-client policy are intentional. No mandated timeout found. Mock HTTP does not establish production TLS or browser behavior; bounded diagnostic body is not a secret scrubber. No new vulnerability inferred |
| T08 | WE:72–73,262,270 credential forms | **N** opaque key/lowercase caller obligation; legacy key-in-both-fields already representable. **Clarify** settings-block prose in `credentials.rs:45` / `wire.rs:353–355` for flat XML/PDF query roots; no credential-placement defect |
| T09 | WE:130–140,203 code 55/56 evidence and ceiling | **P E5f**, **M D3** recovery. Keep numbered 56 success and unnumbered unknown; PHP corroborates but is not live verification. 55's signing failure is not proof of issuance |
| T10 | WE:272; AD:110 mutation recovery; 7/335/338 meanings | **M D3 expansion.** `OutcomeClass` addresses issuance, not every side effect. Credit-entry additive recovery queries current entries; deletion checks disappearance; receipt email cannot be reconciled by invoice external id. 335/338 describe this call, not earlier success; 7 may be missing input on a write. No blanket new class table |
| T11 | WE:263; OO:198,344 create/storno payment-method header | **P E4.** Documented optional capability on the same evidence standard as C2 |
| T12 | QR:295; OO:199,220,344 other auxiliary metadata | **N / explicit scope.** Credit balance's `szlahu_id`, PDF id/method, NAV header/software/success diagnostics, PDF notification flag are not all promised by their operation-specific business result. Keep listed omissions; add only from a stated consumer need and an operation source/capture. C2/C3 retain their stronger evidence |
| T13 | WE:265; IR:226,368 attachment delivery | **N.** Exact five names and conservative 2,000,000-byte bound supported; transport acceptance is not delivery. Invalid-file partial processing and mail-disabled no-op are upstream semantics, no declared per-file Ack/status to expose. Missing attachment-result API requires a real documented/result field, not an invented success boolean |
| T14 | WE:260; OO:335; QR:278 response version 1 | **N.** Deliberate v2-only operations; custom request can select another mode. Unexpected text/HTML is an error, not a missing normal v1 parser. Four writers pin 2; receipts/XML query/deletion/taxpayer have no selector |

### Invoice requests, values, references and live boundaries

| ID | Origin / residual | Disposition and closure |
|---|---|---|
| I01 | IR:328–343 schema divergence/requiredness; RV:166–176 | **P E6; M C1.** Current inline/download `simpleItems`+preview ordering conflict must stay visible. Seek vendor processing-schema confirmation or separately authorized combined-field evidence; neither timestamp nor majority vote settles order |
| I02 | IR:95,134,192,364 small request wording | **P E5g/h.** Exact false/absence, domain-model attribution, layout/waybill distinctions; supported writer unchanged |
| I03 | IR:362–363 minor units and `paid=false` | Minor-unit claims **P E5b / M D4**; **Q** server scale/tolerance. `paid=false` is **Q representational limit**, not proven bug: code emits only true or omission (`invoice.rs:174–178,749–751`). Compare explicit false/omitted with cash/card/transfer and relevant account defaults, then query recorded paid status; only a distinction warrants `Option<bool>` or an override. No XSD default establishes equivalence |
| I04 | IR:356; RC:262–266 foreign-rate gate/MNB | **N** existing general non-HUF gate as chosen boundary supported by current prose; **P E5c** receipt promise. Need foreign receipt bank-only accepted result with returned rate, and separate AAM/proforma/delivery-note omission evidence before relaxing gates |
| I05 | IR:357–359 dates/appearance/proforma references | **N** wire optional date/appearance and three reference-carrying kinds. **Q** live-account/e-invoice backdating and explicit ES/VS proforma references. Current invoice tests and implicit ES consumption do not prove these; capture create and queried date/reference on each claimed shape |
| I06 | IR:360; QR:148,215; OO:336 external ids | **N.** Nonunique/newest-holder, no echo, attach only on actual creation, storno id on SS are recorded facts within account limits. No invented request-layer uniqueness/id length validator. Worker bounds are separate |
| I07 | IR:132,188,361,365 gross-first/final netting | **N.** Raw `LineItem::new` represents gross-first amounts; offer a recipe if needed, not another wire feature. Caller provides negative prepayment line, server does not net. Special VAT codes derive zero by design; `Other("27")` is deliberately not `Percent(27)`, use parser/percent constructor |
| I08 | IR:366,369 obscure settings and free values | **N** plain-data boundary; **Q** aggregator/guardian/logo/payable adjustment/margin-VAT/rendering interactions. Schema presence/type are covered, account semantics not. Need field-specific vendor rules or authorized output evidence, not generalized local validation of all strings/flags |
| I09 | IR:345–350 misleading vendor translations/tokens | **N.** KBAUK meaning follows linked first-party PDF; fulfillment date is not payment date; retain `SzlaNoEnv` token and distinction `Default` versus omission; case is `szamlaSablon`. D1 is the separate actual TAHK rustdoc defect |
| I10 | IR:146,194–226 absent/request-limited fields | **N.** No seller identity request block, no extra FOXPOST/GLS sub-block, MPL weight is schema string, parcel count bound intentional; deprecated download copies are ignored upstream. Dynamic email tags fit existing text. Languages/vendor currency tokens/open rates are representable; D8 owns stale currency count |
| I11 | IR:367, RC:290–296 notifications | **Q.** Test-account delivery routing explains why malformed invoice email may not yield 56; it does not prove production delivery, receipt syntax, or storno emails. Need actual authorized send/delivery observation; no request field added |

### Response shape, content, projections and receipt-specific questions

| ID | Origin / residual | Disposition and closure |
|---|---|---|
| R01 | QR:283–284; OO:243–266; RC:188,255 shared XML boundaries | **M R1 expansion**, explicitly retain **shared** trailing-content/foreign-child-namespace cases. Root expanded-name checking is not whole-document validity. Prior AD already keeps the shared tail problem; do not count again. Fix root completion and structural extraction without imposing every XSD facet |
| R02 | OO:351 blank verdict; WE:128 contradictory success/code | **N** current false/Absent treatment is fail-closed; scratch confirms. Missing verdict already fails. `sikeres=true` plus code, conflicting headers/body/status lack a vendor tie-breaker. If tightening, specify policy and table-test these exact cases; no false-success incident established |
| R03 | QR:288–291; RC:254–255; OO:342–343 | **N** sparse required-content relaxation, empty lists, repeated labels, wider signed ids, future enums, finite Decimal and civil-date domains. Ordinary exponent notation works. **M F1** valid lexical dates, **M R2** string fidelity; these are not excused by “lenient parser.” No NaN/INF/year-zero expansion absent a useful supported-document requirement |
| R04 | QR:280–282; RC:250; OO:196,204 | **N** optional requested PDF can be absent on general document reads/issue; PDF-only query/preview require it. Invalid base64 is rejected ordinarily, softened only for numbered 56. No signature validation is promised. Abbreviated upstream PDF is invalid fixture content, not parser failure; a known numbered write becoming a parse error is not proof nothing landed |
| R05 | QR:292–294; RC:179,252; OO:237 | **M R2** optional-string trim, not every required string. **N** absent queried waybill/attachments/erasure codes/contact input fields/XML outstanding/external id: response schemas have no such elements. Simple NAV addresses are tolerated extra compatibility, not mandatory QueryTaxpayerResponse coverage |
| R06 | QR:296–297 current buyer and financial-item meaning | **Clarify** buyer rustdoc `query_xml.rs:355`: D6 says queried buyer can change after later issue; do not imply immutable snapshot. **Q** `afalevon` unit/range (`:479` “percentage”): unannotated int alone cannot establish or refute percentage. Prefer neutral “VAT deductibility value” pending vendor annotation/example that establishes units; no 0–100 validator |
| R07 | QR:299 legacy tokens/sources | **N** open `JS`, `TEHK`, appearance 2, source 26/28/34 retained. **Q** actual availability on internal XML queries and frequency; shared schema is a superset of operation reach. Need operation-specific query evidence for availability claims, not named variants just to replace `Other` |
| R08 | OO:348–349 direct NAV roots/current version | **N** no `GeneralErrorResponse` parser now: S5 explicitly promises QueryTaxpayerResponse and shows wrapped failures. Need Számla Agent's actual forwarding or vendor statement; NAV direct endpoint errors alone are insufficient. **M C3** business fields; `infoDate` is already in S5, other four forwarding remains open |
| R09 | OO:239 optional validity / sparse taxpayer | **N** `OK` requires explicit validity; absent does not mean false. Valid=false is data, not not-found. Need a legitimate complete `OK` body without validity and its specified meaning before loosening that requirement |
| R10 | RC:268–270 query call id / create-storno scope | **P E5d** unproved creation attribution. **Q** uniqueness scope, retention, cross-operation collision and storno 338: require vendor explanation or separately authorized same-id create/storno/query matrix. Stable create call id and 338 duplicate prevention remain supported; no automatic random id |
| R11 | RC:272–274 email partial defaults | **P E5e** promises; **N** resend via empty present block. Missing whole block does not send. No automatic email field missing from create, and `SendReceipt` returning `()` covers verdict-only response |
| R12 | RC:276–285 duplicate receipt order and example contradictions | **Q** selection among duplicates/after reversal: need two receipts under one order and original/SN query results or vendor rule. **N** alias amount tags, create/query `all` versus prose order, separate current receipt toggle, TEHK retained open. Unbalanced sample totals/reversal-reference on NY are format examples, not accounting evidence |
| R13 | RC:247–249,258; OO:334 | **N** invoice-only receipt fields explicitly refused; arithmetic/tender sum/account prefix rules delegated; HUF minor-unit arithmetic usable, fractional net/VAT also representable. Empty replacing credits intentionally refused; no destructive-clear operation without verification. A repeated receipt call's rejection is not recovered original success |

### Residual live questions and verification limitations

| ID | Origin / residual | Disposition and closure |
|---|---|---|
| V01 | OO:338,352; RV:182; WE:252 storno sign/no-op | **P E5a** guidance; **Q** negative-original positive-storno runtime failure. Query type/reference is the strongest solution; preserve positive-original live evidence without universalizing sign |
| V02 | OO:353,355; IR:398; behavior:174–278 | **Q, retained account-bound live matrix:** zero-total storno; HS/ES/VS and settled ES storno; HS replay; final reissue; sixth-entry server code; SS credit; incoming credit issuer matching; zero-entry replacing credits; receipt operations generally. None inferred from local constructor tests. Need actual request/HTTP reply/queried resulting state for the particular operation; do not exercise intentionally refused requests as part of this review |
| V03 | behavior referenced by IR:354–369, QR:299, OO:353–355 | **Q, no current agent change:** two-day replay window/remaining fingerprint fields; order “last” by id versus date; UI conversion's order/reference; internal whitespace/NFC server handling; external-id limit beyond 110; second D after consumption; seller-id stability/forras variants; cross-account by-number behavior. These remain bounded observations or worker-side protections, not agent conformance evidence. Each stronger guarantee needs its own compare-before/after observation; no blanket “all live-backed” label |
| V04 | IR:390–404; QR:301–317; RC:300–319; OO:357–382; WE:276–293 test claims | **P E6** scope/source checks. **N** no count-inflation finding: RV expressly avoids summing overlapping suites. Request-schema validation ran for six invoice kinds only; receipt/other fresh-schema comparisons were manual. No production/browser/TLS/email behavior inferred from mock or scratch results |
| V05 | QR:298; OO:90–92; RC:82; IR:337 cached source drift | **P E6** dated corpus inventory; seller-text drift and abbreviated PDF not breaking field declarations. Fresh downloads alone cannot prove original acquisition wrong |
| V06 | WE:293 unexecuted follow-up tests | **Retain explicit verification backlog:** cookie reuse/rotation/isolation; status/body controls; non-56 body versus header 56; multipart boundary collision across XML and attachment; `%2B`/`+`, Unicode header decoding and body URLs. These are proposed checks, not demonstrated six new defects. E1/E2 have new scratch evidence; others were not executed by E |

## Methodological challenges to the prior verdict

1. **Same evidence threshold for omissions.** RV:147–150's field coverage is meaningful for the **declared element inventory**. It does not justify excluding documented headers as inherently outside coverage while promoting optional body fields. E4 deserves the same explicit capability-or-chosen-subset decision as C2/C3.
2. **Separate four propositions:** field is representable; selected generated sample is schema-valid; all supported lexical forms parse; server accepts and acts as expected. They are not interchangeable. RV:7/145–150 should be read as inventory/selected-sample results, never “full conformance of all models.” The raw reports and RV:198 already qualify many of these correctly; the judge should preserve those qualifications in the final summary.
3. **Leniency is not one exclusion.** Absence of optional dates/reversal flags, loss of string text, acceptance of an unrelated namespace, and truncated XML accepting success are different mechanisms. R1/R2 survive precisely because no live-backed decision requires those last behaviors. Conversely, rejecting all XSD-incomplete records would break deliberate useful tolerance.
4. **A conservative result can still be inaccurate.** The cookie helper is a real correctness defect despite small impact. HTTP body/status mismatch is real documentation error without proof of service emission. A false storno heuristic is inconclusive; the worker currently turning it into no-op is not validated by positive-original probes. Scope exclusions should identify which of these claims they actually exclude.
5. **Evidence provenance is heterogeneous.** Current XSD, vendor prose, copied format examples, first-party PHP and one-account live observations have different strengths. Current S3's positive storno sample does not prove negative-original behavior; PHP 56 code is stronger than an absent general catalogue row but still not a live capture; July fixture history cannot be reconstructed from September's changed endpoint.
6. **Test coverage is mechanism-specific.** The outline comparator demonstrably erases a meaningful email-block distinction. The local client suite exercises injected transport, not default-client session lifecycle. Existing tests do not certify receipt accounting, full NAV 3.0 structure, browser operation or all account features. RV correctly avoids summing overlapping counts; retain that restraint.

## Recommended adjudication order

1. Keep the original seventeen findings under their owners; attach the shared R1 boundary, broader D3 recovery and D4 currency-evidence qualifications explicitly.
2. Accept **E1/E2** as small concrete fixes, **E3/E5** as precise guidance corrections, and **E6** as source/test-evidence maintenance. These need no live account access.
3. Decide **E4** as an optional result capability on the same basis as C2; preferred resolution is the small optional field rather than forcing callers to replace the built-in projection.
4. Keep Q rows visible and evidence-gated. In particular, do not silently change `paid`, KWD scale, receipt MNB acceptance, storno sign classification, direct NAV roots or browser credential mode on an extrapolation.

The inventory above is the handoff: no unverified note is silently promoted to a production incident, and no concrete public-contract discrepancy is silently discarded merely because it was outside the original seventeen headings.
