# Judge A — protocol surfaces (szamlazz-agent, szamlazz-ipn, szamlazz-adatkapcsolat)

Reports judged: `01-agent-protocol.md` (20 findings), `02-ipn-adatkapcsolat.md` (23 findings).
Method: every medium+ finding re-read at the cited lines with surrounding code; XSDs, upstream
fixtures, `docs/szamlazz-hu-behaviour.md` and vendored crate sources (`quick-xml 0.42.0`,
`clap_builder 4.6.6`, `rust_decimal 1.42.1`, `base64 0.22.1`) consulted where a claim depended on
them. No cargo commands run; no repo files modified. IDs: `A-nn` = report 01 finding nn,
`B-nn` = report 02 finding nn, `J-nn` = new finding from this judge.

## (a) Summary

**Verdict counts (43 reviewer findings, B-20's KEY_ERR half merged into B-07):**

| Verdict | Count |
|---|---|
| CONFIRMED | 33 |
| CONFIRMED-WITH-CORRECTION | 4 (A-03, A-08, B-01, B-19) |
| DOWNGRADED | 3 (A-07, B-03, B-09) |
| UPGRADED | 0 |
| REFUTED | 0 |
| UNVERIFIABLE | 3 (A-19, A-20, B-23) |
| Additional (judge) | 1 (J-01, medium) |

Every factual code claim I checked in both reports was true at the cited lines. No finding is
refuted outright; the corrections are to consequences, evidence weight and severity, not to facts.

**Five most important confirmed findings in this domain**

1. **B-01 (high)** — `szamlazz-adatkapcsolat` re-validates ~30 XSD-`minOccurs=1` elements as hard
   requirements (`document.rs:1001-1084`, `:1445-1485`); a `ParseError::Validation` is a
   deterministic 400 (`axum.rs:277-280`) that szamlazz.hu retries for 72 h and then drops, with no
   local artifact. The repo's own test (`tests/protocol.rs:229-235`) records that official receipt
   batches omit an XSD-required element, so the risk is not hypothetical.
2. **B-04 (high, resolver deployments)** — `KeyResolver::resolve` is sync and infallible
   (`axum.rs:39-45`); its only non-success value, `None`, is rendered as `KEY_ERR` (`:260-275`),
   which `banktranzvalasz.xsd`/`nyugtavalasz.xsd` define as "record not resent". A transient
   lookup failure permanently drops bank/receipt records.
3. **A-01 (medium)** — `ErrorCode::is_retryable()` returns `true` for 1 and 55 regardless of
   operation (`error.rs:192-201`), with a doc comment implying a server-reported code is safe to
   re-send; `CONTEXT.md` (*Unconfirmed*) classifies exactly these codes on a create as
   "outcome not known". A caller looping on the flag can issue a duplicate legal document.
4. **B-10 (medium)** — IPN parsing hard-fails on a non-`%Y-%m-%d` `szlahu_kifizdat`, an empty
   amount, or a missing `szlahu_fizetesmod` (`szamlazz-ipn/src/lib.rs:201-222`) while the same
   file's comma-tolerance rationale (`:240-244`) states that a deterministic 400 loses the
   notification after ten deliveries.
5. **A-02 (medium)** — `InvoiceKind` cannot carry `dijbekeroSzamlaszam` on a prepayment or final
   invoice (`ops/invoice.rs:22-51`, writer `:829-850`) although `xmlszamla.xsd:121-124` defines
   the elements independently; the worker drops `refs.proforma` for prepayments
   (`restate-szamlazz/src/gateway/build.rs:109`) and relies on server auto-linking by order number.

Also notable: **A-13** — the crate (`InvoiceAppearance::Paper = 1`, per the vendor's own
annotation in `szamla_example.xml:32`) and `docs/szamlazz-hu-behaviour.md:5,137` ("e-invoicing
enabled (`<eszamla>1</eszamla>`)") contradict each other; one of them is wrong and the worker's
storno `e_invoice` derivation (`gateway.rs:371-377`) depends on the answer.

**Most consequential corrections (no outright refutations)**

- **B-03** downgraded medium → low: `handler.rs:59` and `axum.rs:349-350` both tell handlers to
  log their own errors; only the trailing clause "the `Display` bound is what the integration
  layer uses to do so" is stale. The reviewer's "actively misleads implementers into believing the
  router logs for them" overstates it.
- **A-07** downgraded low–medium → low: the only in-repo evidence (`taxpayer_error.xml`) shows
  szamlazz.hu relaying its *own* code 57 *inside* the NAV envelope, which the parser handles; the
  `xmlszamlavalasz`-body scenario is speculative, and the worker's read policy bounds the retry to
  three attempts, not an open-ended "until exhaustion".
- **B-09** downgraded high → medium: a README omission, not a defect; the crate already states
  IPN is unauthenticated by design.
- **B-01** evidence correction: the official example's non-`REQ` annotations are weaker evidence
  than stated, because empty elements deserialize as `Some("")` and pass `required_text`
  (verified in quick-xml 0.42 `de/mod.rs:3128-3129`, `de/map.rs:561-576`); the example itself
  would pass validation. The receipt-batch test is the real evidence.

## (b) Verdict table

| ID | Source | Title | Reviewer sev/conf | VERDICT | Judge sev/conf | Justification |
|---|---|---|---|---|---|---|
| A-01 | R01 #1 | `is_retryable()` marks 1/55 retryable for creates | medium/high | CONFIRMED | medium/high | `error.rs:199-201` + doc `:196-197`; CONTEXT *Unconfirmed* lists 1/55 as open on a create; no workspace caller uses the flag (grep), so a public-API footgun rather than a live bug |
| A-02 | R01 #2 | `InvoiceKind` cannot express proforma ref on prepayment/final | medium/high | CONFIRMED | medium/high | `ops/invoice.rs:22-51,829-850`; `xmlszamla.xsd:121-124` lists the elements independently; worker drops `refs.proforma` for `Prepayment` at `gateway/build.rs:109`, relying on order-number auto-link (behaviour doc C1-3) |
| A-03 | R01 #3 | `calculated_for_currency` does no rounding for non-HUF | medium/medium | CONFIRMED-WITH-CORRECTION | medium/medium | `item.rs:139-146` verified; but the fn doc `:125-128` states "exact decimal arithmetic for other currencies", so it is a documented (arguable) design, not a hidden trap; reaches production via `contract/document.rs:336-348` |
| A-04 | R01 #4 | `VatRate::Percent` wire token not normalised | low–medium/medium | CONFIRMED | low/medium | `types.rs:243` `rate.to_string()` keeps scale; worker passes caller `vat_rate: String` through `VatRate::from` (`contract/document.rs:329-331`); all fixtures show integer tokens; acceptance of `27.00` UNVERIFIABLE; a rejection would be `rejected`, not a duplicate |
| A-05 | R01 #5 | HUF net rounded before server's `net = price×qty` check | low/low–medium | CONFIRMED | low/low | `item.rs:137-138,173-176` verified; `tests/live.rs:82-84` is `#[ignore]`d; tolerance UNVERIFIABLE in repo |
| A-06 | R01 #6 | Delivery note forces `SzlaFuvarlevelesAlap` template | low/low–medium | CONFIRMED | low/low | `ops/invoice.rs:865-868` overrides `header.template` silently; XSD (`xmlszamla.xsd:135`) only lists the token; necessity UNVERIFIABLE |
| A-07 | R01 #7 | Taxpayer parser ignores `xmlszamlavalasz` envelope | low–medium/low | DOWNGRADED | low/low | `taxpayer.rs:182-220`: `check()` handles header errors; fixture `taxpayer_error.xml` shows szamlazz.hu code 57 relayed *inside* the NAV envelope, which `into_info` handles; read-policy exhaustion is 3 attempts, not open-ended |
| A-08 | R01 #8 | Untyped 339 and `TEHK` | low/high(untyped) | CONFIRMED-WITH-CORRECTION | low/high | 339 doc at `error.rs:31-32` verified; `TEHK` appears only in the *response* `afatipusTipus` (`szamla.xsd:35`, `xmlnyugtavalasz.xsd:15`), not in any request `afakulcs` list — request acceptance unverified |
| A-09 | R01 #9 | Final-invoice docs omit negative prepayment line | low/medium | CONFIRMED | low/high | behaviour doc C6-2 explicit; `ops/invoice.rs:38-44` silent; README `:62` silent |
| A-10 | R01 #10 | Empty `szlahu_error_code` treated as error | low/low | CONFIRMED | low/low | `wire.rs:205-211`: no empty filter (contrast `check_available` `:224-226`); never observed on success (behaviour doc) |
| A-11 | R01 #11 | `RawResponse` carries no HTTP status | low/high | CONFIRMED | low/high | `wire.rs:141-145`; `client.rs:191-203` discards `response.status()` |
| A-12 | R01 #12 | Zero entries + `additive=false` wipes payments | low/high | CONFIRMED | low/high | `credit_entry.rs:136-144` defaults; no `validate()`; `xmlszamlakifiz.xsd:28` `minOccurs=0`; behaviour doc D7 replace semantics |
| A-13 | R01 #13 | `eszamla`: crate says 1 = paper, behaviour doc reads 1 as e-invoice | low(crate)/medium(docs), medium | CONFIRMED | medium/medium | `query_xml.rs:121-163` matches vendor annotation `szamla_example.xml:32`; behaviour doc `:5,137` contradicts; worker `gateway.rs:371-377` + `service/storno.rs:49-51` derive storno `e_invoice` from it; which side is right is UNVERIFIABLE here |
| A-14 | R01 #14 | Derived `Debug` on `RawResponse` prints `Set-Cookie` | low/high | CONFIRMED | low/high | `wire.rs:141`; `client.rs:191-200` copies every header incl. `set-cookie`; `session_cookie()` `:236-246` proves the value is meaningful |
| A-15 | R01 #15 | `MinimalInvoiceResponse.hibakod` lacks `empty_as_none`; fabricated `"0"` | info/high | CONFIRMED | info/high | `ops/invoice.rs:1268-1297`, `:1316-1321`; `proforma.rs:83-89` same pattern |
| A-16 | R01 #16 | Exchange-rate check stricter than schema; `is_huf` case-sensitive | low/medium | CONFIRMED | low/medium | `ops/invoice.rs:776-787`; `types.rs:379-381`; `xmlszamla.xsd:118-119` optional |
| A-17 | R01 #17 | CLI `invoice storno` skips `CreatedInvoice::reverses()` | low/high | CONFIRMED | low/high | `szamlazz-cli/src/commands/invoice.rs:209-222`; crate instruction `ops/storno.rs:20-24` |
| A-18 | R01 #18 | `+` → space in `szlahu_*` header decode | info/medium | CONFIRMED | info/medium | `wire.rs:250-257`; test `:406` shows the `+`-encoded shape; URL case UNVERIFIABLE |
| A-19 | R01 #19 | `TÉTELÁFA` unrepresented | info/low | UNVERIFIABLE | info/low | no fixture/XSD/doc in repo mentions it (grep); settle by reading the szamlazz.hu `afakulcs` docs |
| A-20 | R01 #20 | `sendEmail` default asserted but unverified | info/low | UNVERIFIABLE | info/low | `ops/invoice.rs:390-398` doc claim; behaviour doc D6 always set `sendEmail=true` |
| B-01 | R02 #1 | Strict required-field validation → deterministic 400 → 72 h loss | high/medium | CONFIRMED-WITH-CORRECTION | high/medium | `document.rs:1001-1084`; `axum.rs:277-280`; `tests/protocol.rs:229-235` precedent; correction: official example passes (empty elements are `Some("")`); same `szamla.xsd` (byte-identical to the agent's, verified) is parsed leniently by `szamlazz-agent` (`query_xml.rs:724-781`) |
| B-02 | R02 #2 | Closed `TransactionDirection`, `non_negative`, required `technikai` | medium/high | CONFIRMED | medium/high | `document.rs:1136-1145,1124-1129,1189-1193`; contradicts module doc `:1-3` |
| B-03 | R02 #3 | Handler/parse/ack errors swallowed; `handler.rs` claims otherwise | medium/high | DOWNGRADED | low/high | `axum.rs:288-317` swallow verified; but `handler.rs:59` says "Log it yourself" and `axum.rs:349-350` repeats it — only the `Display` clause is stale; parse errors are in the 400 body, capturable by a tower layer |
| B-04 | R02 #4 | `KeyResolver` sync/infallible → transient failure = `KEY_ERR` | high/high | CONFIRMED | high/high | `axum.rs:39-45,260-275`; `banktranzvalasz.xsd`/`nyugtavalasz.xsd` "nem küldi újra"; the crate's own 401-for-missing-header reasoning (`:252-256`) is not available to resolvers |
| B-05 | R02 #5 | Unauthenticated bodies buffered + fully scanned; no default limit | medium/high | CONFIRMED | medium/high | `axum.rs:154-157` disable; `:213-216` `Bytes` extractor; `:247-257` `preflight` (full `NsReader` walk, `document.rs:94-125`) before key read; test `protocol.rs:531-549` |
| B-06 | R02 #6 | Empty configured key matches empty header | low/high | CONFIRMED | low/high | `axum.rs:89-101,332-345`; clap passes an empty env through verbatim (`clap_builder-4.6.6/src/builder/arg.rs:2207` `env::var_os`), so `listen.rs:30` is a real path |
| B-07 | R02 #7 (+#20 KEY_ERR half) | Wrong key → `KEY_ERR` permanently halts bank/receipt streams; docs don't warn | low/high | CONFIRMED | low/high | `axum.rs:74-76,260-275`; README `:46` and `listen.rs:26-28` omit the "never resent" consequence; `ack.rs:18-21` partially states it |
| B-08 | R02 #8 | `with_registration_number` infallible, `to_xml` fails later → 500 loop | low/high | CONFIRMED | low/high | `ack.rs:70-73,118-121`; `axum.rs:312-317` |
| B-09 | R02 #9 | IPN README lacks "confirm `paid_gross` via Agent" guidance | high/high | DOWNGRADED | medium/high | `lib.rs:17-19`, README `:39` verified; doc-only gap on a surface already labelled unauthenticated; `is_fully_paid()` `:151-157` invites acting on the body |
| B-10 | R02 #10 | IPN strict on `szlahu_kifizdat`, empty amount, `fizetesmod` | medium/medium | CONFIRMED | medium/medium | `lib.rs:201-222`; `"".parse::<Decimal>()` fails (`rust_decimal-1.42.1/src/str.rs:186`); test `:385-397` locks it in; real parameter set UNVERIFIABLE |
| B-11 | R02 #11 | IPN retries interleave; no sequence/timestamp | low/high | CONFIRMED | low/high | `lib.rs:79-90,95-123`: no ordering field |
| B-12 | R02 #12 | `SOURCE_IPS` proxy pitfall; list date | low/medium | CONFIRMED | low/medium | `lib.rs:68-77` "as of 2025-08-01", "peer address"; list currency UNVERIFIABLE |
| B-13 | R02 #13 | Per-element namespace check; raw `xmlns` compare | info/high | CONFIRMED | info/high | `document.rs:94-125,180-186` |
| B-14 | R02 #14 | `Deserialize` is wire-shape only; README implies round-trip | low/high | CONFIRMED | low/high | README `:35`; `rename(deserialize=…)` throughout; `archive.rs:391-399` |
| B-15 | R02 #15 | "Required" has two meanings (text vs typed) | low/high | CONFIRMED | low/high | quick-xml 0.42 `de/mod.rs:3128-3129` (empty Text → None), `de/map.rs:561-576`; trailing-space trim at `de/mod.rs:2328-2330` |
| B-16 | R02 #16 | Fan-out: clone per member, `KEY_DEL` wins, first `iktatoszam` wins | low/high | CONFIRMED | low/high | `fanout.rs:192-227,229-240,242-266` |
| B-17 | R02 #17 | Archiver JSON serialises PDF then strips it; ordering edge cases | low/high | CONFIRMED | low/high | `archive.rs:391-399` + `Pdf: Serialize` base64 `document.rs:309-314`; other sub-claims not independently verified |
| B-18 | R02 #18 | 401 without `WWW-Authenticate`; Content-Type assumption | info/high | CONFIRMED | info/high | `axum.rs:257-259,319-326` |
| B-19 | R02 #19 | Test gaps: official example never parsed; parse tests behind `axum` | medium/high | CONFIRMED-WITH-CORRECTION | medium/high | `tests/protocol.rs:4` `#![cfg(feature = "axum")]`; `tests/upstream` symlink unused; correction: `Cargo.toml:12` excludes `tests/upstream` for licensing (`fixtures/SOURCES.md`), so an `include_bytes!` test would break `cargo package` — the fix must be runtime-gated |
| B-20 | R02 #20 | CLI `listen` dumps base64 PDFs | low/high | CONFIRMED | low/high | `listen.rs:44-48` `print_json(&invoice)`; `Pdf: Serialize` emits base64; KEY_ERR half merged into B-07 |
| B-21 | R02 #21 | `Handler` cannot grow non-breaking | info/high | CONFIRMED | info/high | `handler.rs:56-87` |
| B-22 | R02 #22 | Constant-time compare best-effort | info/high | CONFIRMED | info/high | `axum.rs:332-345` |
| B-23 | R02 #23 | wasm32 claims plausible, unverified | info/medium | UNVERIFIABLE | info/low | not independently verified (pass-through); would need `cargo check --target wasm32-unknown-unknown` in CI |
| J-01 | judge | Strict base64 PDF decode is a deterministic 400 for the whole invoice | — | NEW | medium/medium | `document.rs:1615-1636` uses `base64::STANDARD` = `RequireCanonical` padding, no trailing bits (`base64-0.22.1/src/engine/general_purpose/mod.rs:257-263`); a PDF encoding anomaly loses a legal document |

## (c) Detailed notes

### A-03 — CONFIRMED-WITH-CORRECTION (currency rounding)
The code does exactly what the reviewer says (`item.rs:139-146`; the test at `:254-266` asserts
`27.00135`). The correction is to framing: the fn doc at `item.rs:125-128` states plainly "whole
forints for HUF (`HUF`/`Ft`), exact decimal arithmetic for other currencies. Use `LineItem::new`
when a foreign-currency business rule requires an explicit rounding scale", so this is a documented
design decision, not a hidden inversion. The substantive point stands: the worker's
`LineItemInput::to_line_item` (`restate-szamlazz/src/contract/document.rs:336-348`) is the only
path to the wire and it uses this helper, so sub-minor-unit EUR values reach szamlazz.hu in
production. The server's 259–261 checks pass trivially on exact arithmetic; what szamlazz.hu does
with `100.005` (round for print/NAV, or reject) is not settled by any fixture — a live probe
(one EUR line with a 3-decimal net) would settle it. Severity stays medium because the asymmetry
with `calculated()` and the NAV 2-decimal reporting context make silent rounding drift plausible.

### A-07 — DOWNGRADED (taxpayer error envelope)
Facts verified: `QueryTaxpayer::parse` (`taxpayer.rs:182-185`) calls `response.check()` — so
header-form errors (3/135/136/164 with `szlahu_error_code`) are already handled — and then
`TaxpayerResponse::from_body` accepts only `QueryTaxpayerResponse` in the OSA 2.0/3.0 namespaces
(`:207-220`). The reviewer's scenario is an `xmlszamlavalasz` body for `xmltaxpayer`. The only
in-repo evidence points the other way: `fixtures/upstream/agent/responses/taxpayer_error.xml` shows
szamlazz.hu's *own* code 57 (malformed request XML — a szamlazz.hu-side validation, not NAV's)
delivered *inside* the NAV envelope as `<result><funcCode>ERROR</funcCode><errorCode>57`, which
`into_info` maps to an `ApiError`. So the crate's dual-path handling (header for szamlazz.hu-side
codes, envelope for relayed codes) is consistent with what the vendor documents. The consequence is
also overstated: a parse error is *Unanswered* in the worker and the *Read policy* defaults are
3 attempts / 2 min max — bounded, then `unavailable`. Low, low confidence; a live probe with a
wrong agent key on `xmltaxpayer` would settle the header-vs-body question.

### A-08 — CONFIRMED-WITH-CORRECTION (TEHK / 339)
339 is documented as untyped at `error.rs:31-32` — confirmed. `TEHK` is confirmed in the XSDs, but
only in `afatipusTipus` (`fixtures/upstream/agent/xsd/szamla.xsd:35`, `xmlnyugtavalasz.xsd:15`),
i.e. the *response-side* `afatipus` element of the queried invoice/receipt. The request `afakulcs`
element is a plain `xs:string` (`xmlszamla.xsd:75`) and the only documented value list in the repo
(`requests/xmlnyugtacreate.xml:26`) does not include `TEHK` (nor `TAHK`). Adding
`VatRate::Tehk` is still sensible because `DocumentItem::vat_rate()` (`query_xml.rs:398-400`) maps
`afatipus` into `VatRate` and currently yields `Other("TEHK")`, but the "request token" framing is
unverified.

### A-13 — CONFIRMED (contradiction), resolution UNVERIFIABLE
`InvoiceAppearance` (`query_xml.rs:121-163`) follows the vendor's own annotation in
`fixtures/upstream/adatkapcsolat/szamla_example.xml:32` ("0: not an invoice, 1: paper invoice,
2: e-invoice, 3: e-invoice"). `docs/szamlazz-hu-behaviour.md:5` and `:137` state the probe account
had "e-invoicing enabled (`<eszamla>1</eszamla>`)". These cannot both be right. Downstream, the
worker's `InvoiceDocumentExt::e_invoice()` (`restate-szamlazz/src/gateway.rs:371-377`) maps
`Paper → Some(false)` and `service/storno.rs:49-51` uses that to set the storno request's `eszamla`;
the worker's own test fixtures use `eszamla=2` for e-invoices (`gateway.rs:1731`,
`tests/gateway.rs:110`). Given the worker's `Defaults::e_invoice` is `false` (`config.rs:414`) and
the vendor text is explicit, the more likely resolution is that the probe documents were *paper*
invoices and the behaviour doc's premise (and its "352 may be an e-invoice rule" caveat, echoed in
`ops/storno.rs:89-92`) is wrong — which would mean the storno `e_invoice` path has never been
exercised against a real e-invoice. Medium because either outcome requires a fix (doc or worker).
Settle on the go-live account: create one document with `<eszamla>true</eszamla>` and one with
`false`, query both, record `<eszamla>`.

### A-19, A-20 — UNVERIFIABLE
No fixture, XSD or doc in the repo mentions `TÉTELÁFA` (grep of `fixtures/` and `crates/`); the
`sendEmail`-omitted default is asserted only in the field doc (`ops/invoice.rs:390-398`) and every
behaviour-doc probe set it explicitly (D6). Both need the szamlazz.hu docs page or a live probe.

### B-01 — CONFIRMED-WITH-CORRECTION (strict validation)
Code verified at `document.rs:1001-1084` (invoice), `:1087-1106` (totals), `:1445-1485`
(receipts); the 400 path at `axum.rs:277-280` runs before any handler and before the `Archiver`.
CONTEXT (*Control code*) confirms non-200 = "retry within 72 hours". The precedent the reviewer
cites is real: `tests/protocol.rs:229-235` ("seen_in_official_batches") strips `alap/adoszam`,
which `xmlnyugtaarchiv.xsd:55` marks `minOccurs="1"`. Two corrections. (1) The reviewer's second
evidence line — the official example's non-`REQ` annotations on `privatePersonIndicator` and
`devizanem` — is weaker than stated: in `szamla_example.xml` those elements are *present*, and
elements that are present-but-empty (`<nev></nev>`, `<adoszam></adoszam>`) deserialize as
`Some("")` and pass `required_text` (quick-xml 0.42 `de/map.rs:561-576` → `de/mod.rs:3128-3134`:
an `End` event after the start yields `visit_some`). The example itself would pass `validate()`.
(2) An observation both reviewers missed that strengthens the finding: the Adatkapcsolat
`szamla.xsd` and the Agent's `szamla.xsd` are byte-identical (verified with `diff`), yet the
`szamlazz-agent` crate parses the same document with only `id`, `szamlaszam`, `tipus`, `eszamla`
required and everything else `#[serde(default)]` (`ops/query_xml.rs:724-781`), while the receiver
crate requires ~30 elements. The workspace already contains the lenient posture the reviewer
recommends. Severity stays high, confidence medium (what production pushes omit is unknowable
here; a week of archived raw pushes from a live account would settle it).

### B-03 — DOWNGRADED (swallowed errors)
The swallowing is real: `axum.rs:288,296,302,307` do `Err(_) => handler_error()` and `:312-317`
turns an `AckError` into a bare 500. But the documentation claim is only partly right.
`handler.rs:57-62` reads "Log it yourself for diagnostics; the `Display` bound is what the
integration layer uses to do so" — the operative instruction is "log it yourself", and
`axum.rs:347-350` repeats "Handlers should log their own errors for diagnostics". Only the
`Display`-bound clause is stale (nothing formats the error). Parse-validation errors are written
into the 400 body (`:249,:279`), so a standard tower/axum response-logging layer captures them.
The missing raw-body hook for rejected pushes is a real gap (and the right complement to B-01),
but on its own this is a doc fix plus a nice-to-have: low.

### B-09 — DOWNGRADED (IPN trust model)
Verified: `lib.rs:17-19` and README `:39` say only "unauthenticated by design … unguessable path …
`SOURCE_IPS` as defence in depth"; nothing says "confirm via the Agent before acting". The
recommendation is correct and cheap, and `is_fully_paid()` (`lib.rs:151-157`) does make acting on
the body the path of least resistance. But this is a README omission on a surface the crate already
labels unauthenticated in three places; there is no code defect, and the crate has no way to
enforce integrator behaviour. High is reserved here for defects with a direct failure path; medium.

### B-19 — CONFIRMED-WITH-CORRECTION (test gaps)
`tests/protocol.rs:4` is `#![cfg(feature = "axum")]` — confirmed; the `tests/upstream` symlink
exists and nothing references it — confirmed (the same is true of `szamlazz-agent/tests/upstream`).
Correction: `Cargo.toml:12` (`exclude = ["tests/upstream"]`) and `fixtures/SOURCES.md` explain
why — the official corpus has no redistribution licence and must not enter the published package,
so an `include_bytes!("upstream/szamla_example.xml")` test would fail `cargo package`'s
verification build. The recommendation must be runtime-gated (`std::fs::read` with a skip when the
symlink is absent, or an env-gated test) rather than the `include_bytes!` the reviewer suggests
as one option. The value of the finding is unchanged: the vendor's own example is the single best
conformance fixture and it is never exercised. Medium.

### B-23 — UNVERIFIABLE
Manifests read by the reviewer; I did not independently verify the wasm32 dependency graph and no
wasm build exists in the repo's CI definition. Pass-through, not independently verified.

## (d) Additional finding

### J-01. Strict base64 PDF decoding turns a PDF encoding anomaly into a deterministic 400 for the whole invoice push

- **Severity:** medium
- **Confidence:** medium — code and library defaults verified; whether szamlazz.hu ever emits
  non-canonical base64 is not verifiable from the repo (the official example has `<pdf></pdf>`).
- **Location:** `crates/szamlazz-adatkapcsolat/src/document.rs:1615-1636` (`de::base64_pdf`),
  applied at `:981` (`#[serde(default, deserialize_with = "de::base64_pdf")]`); consumer path
  `src/axum.rs:277-280`; test `tests/protocol.rs:430-441` (`wrong_key_does_not_decode_embedded_pdf`
  asserts that `<pdf>not base64!</pdf>` with the right key is a 400).
- **Evidence:** `base64::engine::general_purpose::STANDARD` is built with `PAD` =
  `GeneralPurposeConfig::new()`, i.e. `decode_allow_trailing_bits: false` and
  `decode_padding_mode: DecodePaddingMode::RequireCanonical`
  (`~/.cargo/registry/src/*/base64-0.22.1/src/engine/general_purpose/mod.rs:257-263,332,347`).
  Whitespace is stripped (`split_whitespace`), but an unpadded stream (Java
  `Base64.getEncoder().withoutPadding()`), a URL-safe alphabet, or non-zero trailing bits from a
  lenient encoder all fail, and the failure is a `DeError` → `ParseError` → 400 before the handler
  or `Archiver` runs.
- **Why it matters:** Same failure class as B-01/B-02 but for an *auxiliary* field: the legal
  document (ids, totals, buyer, items) is complete and parseable, yet the receiver refuses it
  deterministically, szamlazz.hu retries for 72 h and drops it, and nothing is archived. The crate
  already treats `<pdf></pdf>` as `None`; a malformed PDF payload deserves the same degradation, not
  loss of the invoice.
- **Recommendation:** Decode with a lenient engine (`GeneralPurposeConfig::new()
  .with_decode_padding_mode(DecodePaddingMode::Indifferent).with_decode_allow_trailing_bits(true)`)
  and, on failure, yield `pdf: None` while keeping `raw_xml` (already retained at `:70`) so the
  handler/archiver can recover the bytes; optionally surface a `pdf_error: Option<String>` on
  `InvoiceDocument`. Update the `wrong_key_does_not_decode_embedded_pdf` test's second half
  accordingly.

(No further medium+ findings. Two low/info observations for completeness, not counted: the Agent's
`AlapXml.teszt` uses `#[serde(default)]` + `flexible_bool`, which maps an absent or empty `<teszt>`
to `false` = "live" (`ops/query_xml.rs:777-778`, `xml.rs:184-195`), so the worker's mode pin would
pass a `teszt`-less test document as live — `szamla.xsd:149` says `minOccurs="1"` and every
observed document carried it, so low; and `InvoiceInfo.id: i32` in the receiver
(`document.rs:455`) is XSD-exact (`int`) while the Agent uses `u64` — observed ids are ~0.9 × 10⁹,
so not imminent.)

## (e) Reviewer quality ratings

**Report 01 (agent protocol)** — Accuracy 20/20 on code facts; 17 confirmed as rated, 2 confirmed
with a correction to evidence/framing (A-03 documented design, A-08 `TEHK` is a response-side
enum), 1 downgraded for an over-drawn consequence (A-07), 2 correctly self-labelled unverifiable.
Depth is excellent: I spot-checked the XSD sequence-order claims for `xmlszamla`, `xmlszamlast`,
`xmlszamlaxml` and `xmlszamlakifiz` and they hold, and the report correctly separates what the
fixtures/behaviour doc prove from what needs a live probe. Actionability is high — every finding
has a concrete, minimal fix; the one weakness is a tendency to rate documented-but-debatable
design choices (A-03) as traps.

**Report 02 (IPN + Adatkapcsolat)** — Accuracy 22/22 on code facts for the findings I verified
(B-23 passed through); 18 confirmed as rated, 2 confirmed with corrections (B-01 evidence weight,
B-19 packaging constraint), 2 downgraded for severity (B-03 misread a doc sentence's emphasis;
B-09 rated a README gap as high). Depth is very good — the reviewer read vendored quick-xml, http
and rust_decimal sources and every such claim I re-checked (empty-element `Some("")`, whitespace
trim, scientific-notation parse) was correct; it missed that the identical `szamla.xsd` is parsed
leniently next door and that the base64 PDF path is another deterministic-400 trap (J-01).
Actionability is high, with the caveat that the B-19 recommendation as written would break
`cargo package`.
