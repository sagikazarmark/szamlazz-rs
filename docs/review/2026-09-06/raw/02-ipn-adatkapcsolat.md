# Review 02 — IPN and Adatkapcsolat inbound surfaces

Scope: `crates/szamlazz-ipn`, `crates/szamlazz-adatkapcsolat`, the upstream XSD/example corpus under `fixtures/upstream/adatkapcsolat/`, and `crates/szamlazz-cli/src/commands/listen.rs`. Read-only review; no cargo commands were run. Where I assert quick-xml / http / rust_decimal behaviour I read the vendored crate sources in `~/.cargo/registry` (quick-xml 0.42.0, http 1.5.0, rust_decimal 1.42.1) rather than guessing.

## Summary

The document models are a faithful and complete mapping of the four upstream XSDs (every element of `szamla.xsd`, `szamlabe.xsd`, `banktranz.xsd`, `xmlnyugtaarchiv.xsd` is present, with correct names and case, correct optionality where it is *looser* than the XSD, and open sets for VAT codes, document types and languages), and the four Ack renderers are schema-exact (root, namespace, `alap/id` + `iktatoszam`, `hibakod` enumeration) with the protocol distinction "control code = 200 + `<hibakod>`, failure = non-200" implemented and documented consistently with `CONTEXT.md`. The main risks are not conformance bugs but *posture* decisions: (1) ~30 XSD-`minOccurs=1` elements are re-validated as hard requirements, so a single missing element turns a push into a 400 that szamlazz.hu retries for 72 h and then drops — while the repo's own test admits szamlazz.hu omits XSD-required elements in practice; (2) the multi-tenant `KeyResolver` is sync and infallible, so any transient lookup failure can only be expressed as `KEY_ERR`, which permanently halts bank/receipt delivery; (3) the axum router buffers and fully scans unauthenticated bodies with the body limit disabled by default; (4) the IPN README never tells integrators to confirm an unauthenticated `paid_gross` against the Agent before acting on it; (5) the router silently swallows handler errors although `handler.rs` says the integration layer logs them. Test coverage is good on the synthetic fixture but never parses the official `szamla_example.xml`, and the parse-level tests are gated behind the `axum` feature.

## Findings

### 1. Required-field validation is stricter than what szamlazz.hu demonstrably sends; a missing element loses the push after 72 h and the receiver never sees it

- **Severity:** high
- **Confidence:** medium — the validation code and the fixture annotations are verified; what szamlazz.hu really omits in production is not verifiable from the repo, but the repo's own test (`accepts_receipts_without_issuer_tax_number_seen_in_official_batches`, protocol.rs:229-235) documents one observed case of an XSD-required element being absent.
- **Location:** `crates/szamlazz-adatkapcsolat/src/document.rs:1001-1084` (`InvoiceDocument::validate`), `:1087-1106` (`validate_totals`), `:1445-1485` (`ReceiptBatch::validate`); consumer path `src/axum.rs:277-280`.
- **Evidence:** `validate()` requires, among ~30 others, `alap/fizmodunified`, `alap/katafokonyv`, `alap/kata`, `alap/penzforg`, `vevo/adoszam`, `vevo/lokacio`, `vevo/privatePersonIndicator` (outgoing), `tetel/sztetordering`, `qutet/afalevon`. The official annotated example `fixtures/upstream/adatkapcsolat/szamla_example.xml` marks `<privatePersonIndicator>` as `<!--     boolean -->` (not `REQ`) and `<devizanem>` as `<!--     string  -->` (not `REQ`), while the XSD says `minOccurs="1"` for both — the vendor's own docs and schema disagree. `xmlnyugtaarchiv.xsd` marks `alap/adoszam` `minOccurs="1"`, yet the test at protocol.rs:229 exists because official batches omit it. A `ParseError::Validation` is answered `400` (axum.rs:279) before any `Handler` method runs, so neither business logic nor the `Archiver` ever sees the raw body.
- **Why it matters:** Any non-200 means "retry for 72 hours, then drop" (CONTEXT: *Control code*). A validation failure is deterministic, so all retries fail identically and the document is lost, with no local artifact. The `BankTransaction` doc comment (document.rs:1159-1168) argues this is deliberate for the "contractually-required core", but the invoice validation goes far beyond identity (`kata`, `katafokonyv`, `fizmodunified`, `penzforg`, `sztetordering` were clearly added over time — `szamlabe.xsd:130` still carries a `<!-- NEW -->` marker). Rejecting an otherwise-usable invoice because `katafokonyv` is absent trades a nullable field for total data loss.
- **Recommendation:** Reduce hard validation to true identity (`alap/id`, `alap/szamlaszam`, root structure) and make everything else `Option` as it already is in the struct; or keep the strict check but move it behind an explicit `Document::parse_strict` / builder flag, defaulting to lenient. Independently, give the integration layer a raw-body escape hatch (see finding 3) so that even a rejected push is retained for forensics.

### 2. Closed enum and business-rule checks are additional deterministic-400 traps

- **Severity:** medium
- **Confidence:** high — code verified.
- **Location:** `document.rs:1136-1145` (`TransactionDirection`), `:1124-1129` (`non_negative`), `:1579-1604` (`flexible_bool`/`opt_flexible_bool`), `:1189-1193` (`technical: bool` required).
- **Evidence:** `TransactionDirection` has exactly `BE`/`KI` with no `#[serde(other)]` fallback; any new value is a `DeError` → 400. `non_negative` rejects a negative `afakulcs` (XSD `minInclusive 0`) — a constraint the receiver has no need to enforce. `flexible_bool` rejects anything but `true/false/1/0` (that one is XSD-exact and fine), and `technikai` is required (`bank_transaction_rejects_missing_technical_flag`, document.rs:1709).
- **Why it matters:** The crate's stated philosophy (module doc, document.rs:1-7; `InvoiceAppearance::Unknown`, `ReceiptInfo.kind: Option<String>`, VAT codes as strings) is "open sets so evolution does not break receivers". `TransactionDirection` and `non_negative` contradict it. A new `irany` value or a negative VAT rate on a corrective line would drop bank transactions / invoices after 72 h.
- **Recommendation:** Add `#[serde(other)] Unknown` (or keep the raw string) to `TransactionDirection`; drop `non_negative` (or demote it to a warning field). Consider whether `technikai` really must be fatal or should default to `false` with the raw XML preserved.

### 3. Handler errors, parse errors and Ack-render errors are swallowed silently; `handler.rs` claims the opposite

- **Severity:** medium
- **Confidence:** high — code verified.
- **Location:** `src/axum.rs:249`, `:279`, `:288`, `:296`, `:302`, `:307`, `:312-317`; contradicting doc at `src/handler.rs:57-62`.
- **Evidence:** `handler.rs:59-61`: "Log it yourself for diagnostics; the `Display` bound is what the integration layer uses to do so." The integration layer does `Err(_) => handler_error()` (axum.rs:288) — the error is dropped, never formatted. Parse errors are echoed to szamlazz.hu in the 400 body (axum.rs:249, 279) but not surfaced locally. An `AckError` (bad `iktatoszam`) becomes a bare 500 (axum.rs:315) with no signal to the handler that its registration number was the problem.
- **Why it matters:** A misbehaving deployment produces 500s/400s that szamlazz.hu retries for 72 hours while the operator's logs contain nothing. The doc comment actively misleads implementers into believing the router logs for them. Combined with finding 1, an operator cannot even discover *which* element failed validation without capturing traffic.
- **Recommendation:** Either add an optional `tracing` feature (log at `warn` with the `Display` of the error and the document id/root kind) or add an `on_error`/`on_unparsable(&[u8], &ParseError)` hook to the router builder so a raw-body archiver can be attached. At minimum fix the `handler.rs` sentence.

### 4. `KeyResolver` is synchronous and infallible, so a transient lookup failure can only be voiced as `KEY_ERR` — which permanently stops bank/receipt delivery

- **Severity:** high (for `router_with_resolver` deployments); n/a for the fixed-key router
- **Confidence:** high — trait signature and dispatch verified; the "not resent" semantics for bank/receipt KEY_ERR come from `banktranzvalasz.xsd`/`nyugtavalasz.xsd` comments in the fixture.
- **Location:** `src/axum.rs:39-45` (`fn resolve(&self, presented_key: &str) -> Option<&Self::Handler>`), `:260-275` (None → KEY_ERR ack, 200).
- **Evidence:** `let Some(handler) = receiver.resolver.resolve(presented_key) else { return match root { … Ack::key_error() … } }`. The only non-success value is `None`, which is rendered as the protocol statement "this key is not known here". `banktranzvalasz.xsd`: "KEY_ERR … a banki tranzakció rekordot nem küldi újra"; `nyugtavalasz.xsd` likewise for receipts.
- **Why it matters:** A DB-backed multi-tenant resolver that times out, is rate-limited, or is mid-migration must return `None` (it cannot `await` and cannot return an error) and thereby tells szamlazz.hu to drop the record for good. The crate's own reasoning for answering a *missing header* with 401 instead of KEY_ERR (axum.rs:252-256) applies with equal force here, but the API gives the resolver no way to say "retry later". The sync signature also forces blocking I/O inside an async handler.
- **Recommendation:** Change to `async fn resolve(&self, key: &str) -> Result<Option<&Self::Handler>, Self::Error>` (or a three-state enum `Known(&H) | Unknown | Unavailable`), mapping `Unavailable` to 503 so the 72-hour retry window stays alive. Keep `FixedKey` infallible internally.

### 5. Unauthenticated requests are fully buffered and XML-scanned twice with the body limit disabled by default

- **Severity:** medium
- **Confidence:** high — code and test verified; the "receipt batches are unbounded" premise is not verifiable from the fixtures.
- **Location:** `src/axum.rs:154-157` (`DefaultBodyLimit::disable()` when no limit given), `:213-216` (`body: Bytes` extractor runs before any check), `:247-250` (`Document::preflight` before the key header is read at `:257`); `document.rs:54-60` (`preflight` = `from_utf8` + `root_kind` + full-document `NsReader` scan); locked in by `tests/protocol.rs:531-549`.
- **Evidence:** `router()` and `router_with_resolver()` call `build_router(_, None)` → `router.layer(DefaultBodyLimit::disable())`. In `receive_inner`, the entire body is already in memory and `Document::preflight(&body)` (which walks every element of the document) runs before `headers.get(KEY_HEADER)`.
- **Why it matters:** Anyone who learns the receiver URL can post gigabyte bodies that are buffered in RAM and parsed without presenting a key. The PDF base64 decode is deliberately deferred past authentication (`wrong_key_does_not_decode_embedded_pdf`), which shows the authors considered this — but the far larger cost (buffering + two parser passes) is not deferred. The premise "Számlázz.hu publishes no maximum size" (axum.rs:86-88) is an argument for a *large* default, not for *none*.
- **Recommendation:** Check header *presence* before touching the body (a custom `FromRequest` that reads parts first), run only `root_kind` (which stops at the first start tag) before the key check and the full namespace scan after it, and ship a finite default limit (e.g. 128–256 MiB) with `router_with_body_limit` as the override. Update the >32 MiB test to assert the new default rather than "unbounded".

### 6. An empty configured key is accepted and matches an empty header value

- **Severity:** low
- **Confidence:** high — code verified.
- **Location:** `src/axum.rs:89-101` (`router(key: impl Into<String>, …)`), `:332-345` (`keys_match` returns `true` for two empty slices); `crates/szamlazz-cli/src/commands/listen.rs:30-31` (`env = "SZAMLAZZ_ADATKAPCSOLAT_KEY"`).
- **Evidence:** No validation of `key` at construction; `keys_match("", "")` → lengths equal, `diff == 0` → `true`. A request with `X-Szamlazzhu-Key:` (present, empty) would then reach the handler.
- **Why it matters:** An unset-but-exported environment variable (`SZAMLAZZ_ADATKAPCSOLAT_KEY=`) is a plausible misconfiguration that turns authentication off.
- **Recommendation:** Reject empty (or whitespace-only) keys in `router`/`router_with_body_limit` (panic or return `Result`), and treat an empty header value as missing (401).

### 7. Wrong key → `KEY_ERR` is protocol-correct but a one-character typo permanently halts bank-transaction and receipt streams

- **Severity:** low (design is defensible) / documentation
- **Confidence:** high — behaviour verified; consequences from the fixture XSD comments.
- **Location:** `src/axum.rs:74-76`, `:260-275`; `README.md:46`; `listen.rs:26-28`.
- **Evidence:** "requests whose `X-Szamlazzhu-Key` header does not match `key` are answered `200` + `KEY_ERR` (per protocol)". For invoices szamlazz.hu resends on change; for bank transactions and receipts "nem küldi újra" (never resent).
- **Why it matters:** During onboarding or a key rotation the first mis-keyed push silently and irrecoverably drops bank/receipt records. Neither README nor the CLI help warns that pointing a *live* account at a receiver with the wrong key has permanent consequences.
- **Recommendation:** Document the consequence loudly next to the KEY_ERR behaviour; consider an opt-in `KeyMismatch::Retry` policy (answer 401 for a configurable grace period after startup / for bank and receipt roots only) for rollouts.

### 8. `with_registration_number` is infallible but rendering can fail later, producing a 500 loop for a document the handler already accepted

- **Severity:** low
- **Confidence:** high — code verified.
- **Location:** `src/ack.rs:70-73` (builder), `:118-121` (`to_xml` validates and errors), `src/axum.rs:312-317` (→ `handler_error()`).
- **Evidence:** A handler that durably commits, then returns `InvoiceAck::accept(id).with_registration_number(bad)` gets a 500; szamlazz.hu re-pushes; the handler (if deterministic) supplies the same bad number; repeat for 72 h.
- **Why it matters:** The failure is attributed to the handler ("handler error") though the handler succeeded; no signal reaches it (finding 3).
- **Recommendation:** Make `with_registration_number` return `Result<Self, AckError>` (or add `try_with_registration_number`) so the check happens before the handler commits; alternatively sanitise by stripping forbidden code points and logging.

### 9. IPN README gives no guidance to verify an unauthenticated `paid_gross` against szamlazz.hu before acting on it

- **Severity:** high (security guidance for a payment-relevant, unauthenticated surface)
- **Confidence:** high — README and lib docs verified; szamlazz.hu being the source of truth is stated in CONTEXT (*Order*, ADR 0005).
- **Location:** `crates/szamlazz-ipn/src/lib.rs:17-19`, `README.md:39`; `SOURCE_IPS` at `lib.rs:68-77`.
- **Evidence:** "IPN is unauthenticated by design. Register a URL containing an unguessable path segment, and optionally treat `SOURCE_IPS` as a defense-in-depth signal rather than authentication." Nothing tells the integrator that the body itself must not be trusted for fulfilment decisions, nor that the Agent's query operation (`szamlazz-agent`, `Szamlazz.Agent.query` in the worker) can confirm the invoice's payment status and, incidentally, disambiguate invoice vs proforma (the "absent document-kind discriminator" problem the crate documents at `lib.rs:12-13`, `:115-117`).
- **Why it matters:** An unguessable path is confidentiality of a URL, which leaks through proxies, logs, tunnels (`cloudflared` in `listen.rs`), and error pages. A forged `szlahu_kifizetettbrutto=…` that a receiver acts on (ship goods, unlock access) is a direct fraud vector. The strongest and cheapest mitigation — treat IPN as a *trigger*, read the truth from szamlazz.hu — is the one guidance line the README lacks.
- **Recommendation:** Add a "Trust model" paragraph: never act on `paid_gross` without confirming via the Agent (by `document_number`); use IPN only to schedule that read; note that the Agent query also settles the document kind. Add the X-Forwarded-For caveat to `SOURCE_IPS` (finding 12).

### 10. IPN parsing is strict on `szlahu_kifizdat` and on three "required" parameters, so an unexpected format loses the notification after 10 retries — contradicting the crate's own comma-tolerance rationale

- **Severity:** medium
- **Confidence:** medium — code verified; the real parameter set and formats are not verifiable from the repo (no IPN fixture exists under `fixtures/`).
- **Location:** `crates/szamlazz-ipn/src/lib.rs:201-211` (strict `%Y-%m-%d`), `:219-222` (`Missing` for `bruttovegosszeg`, `kifizetettbrutto`, `fizetesmod`), `:240-244` (comma tolerance comment), `src/axum.rs:8-15`, `:56-64` (→ 400).
- **Evidence:** `parse_decimal` says: "Tolerate a lone comma so an unexpected '1234,56' does not 400 — szamlazz.hu discards a notification after ten failed deliveries." Yet `szlahu_kifizdat=2026-07-04T12:30:00+02:00` is a hard `Invalid` (test `payment_date_requires_the_documented_date_only_format`, lib.rs:385-397), an empty `szlahu_kifizetettbrutto=` is a hard `Invalid` (`"".parse::<Decimal>()` fails), and a missing `szlahu_fizetesmod` is `Missing`.
- **Why it matters:** The crate correctly identifies that a deterministic 400 is a data-loss event for IPN, then applies that insight to one field only. `payment_date` is an *optional* enrichment; failing the whole snapshot on its format is the worst trade. `payment_method` may plausibly be absent for some document kinds (unverifiable).
- **Recommendation:** Degrade `payment_date` to `None` (optionally keep `payment_date_raw: Option<String>`) instead of erroring; make `payment_method` optional or default to empty; treat an empty amount as `Missing` only if it is genuinely absent, else consider `Decimal::ZERO`. Keep `document_number` as the single hard requirement.

### 11. IPN snapshot ordering: retries interleaved with newer snapshots make naive upsert last-writer-wins, and the payload carries no sequence or timestamp

- **Severity:** low
- **Confidence:** high — the payload shape is verified; the interleaving is a consequence of "retried every 3 minutes" stated in CONTEXT.
- **Location:** `crates/szamlazz-ipn/src/lib.rs:5-10`, `:79-90`; `README.md:35`.
- **Evidence:** Guidance is "replace or upsert the stored status … never add `paid_gross`". No field distinguishes an older retried snapshot from a newer one; `PaymentNotification` has no arrival ordering hint.
- **Why it matters:** If delivery #1 (paid 5 000) fails at the receiver and is retried 3 minutes later, while delivery #2 (paid 10 000) succeeded in between, the retry regresses the stored status. Monotonic guards are unsafe because paid amounts legitimately decrease (storno, refunds).
- **Recommendation:** State this explicitly and point to the Agent read (finding 9) as the way to resolve ordering; suggest storing IPN arrivals append-only and deriving state from the Agent.

### 12. `SOURCE_IPS` guidance omits the proxy/`X-Forwarded-For` pitfall and the list's provenance date is stale relative to the fixture corpus

- **Severity:** low
- **Confidence:** medium — comment verified; whether the list is current is not verifiable.
- **Location:** `crates/szamlazz-ipn/src/lib.rs:68-77`.
- **Evidence:** "as of 2025-08-01 … compare directly against a connection's peer address". The fixture corpus was fetched 2026-07-04 (`fixtures/SOURCES.md`); today is 2026-09-06. On Cloudflare Workers (a target the crate advertises) the peer address is Cloudflare's; on any reverse proxy it is the proxy's.
- **Why it matters:** A receiver that compares the TCP peer will reject every legitimate IPN behind a proxy, or accept every forged one if the proxy's IP is whitelisted.
- **Recommendation:** Say "compare the *client* address your platform reports (e.g. `CF-Connecting-IP`, trusted `X-Forwarded-For`)", and refresh/confirm the list date.

### 13. Router-level preflight validates the namespace of every element, and `root_kind` compares the raw (unescaped) `xmlns` attribute value

- **Severity:** info
- **Confidence:** high — code verified against quick-xml 0.42 (`Attribute.value` is the raw value).
- **Location:** `document.rs:94-125` (`validate_element_namespaces`), `:180-186` (raw attribute compare).
- **Evidence:** Every `Start`/`Empty` element must resolve to the root's namespace; `attribute.value.as_ref()` is compared without entity normalisation.
- **Why it matters:** XSD-exact (`elementFormDefault="qualified"`), so conformant. It is stricter than tolerance-to-evolution requires (an extension element from another namespace, or a stray `xmlns=""` on a child, kills the push). The raw-attribute compare would miss a namespace URI written with character references — practically irrelevant. The prefixed-root case is handled correctly; quick-xml's serde deserializer matches on local names (`de/key.rs::from_elem`), so `<s:szamla xmlns:s="…">` documents parse.
- **Recommendation:** Keep, but consider relaxing the per-element check to "root namespace correct, children either in the root namespace or skipped" if evolution tolerance is the goal; document the strictness.

### 14. `Deserialize` impls are wire-shape only; README implies general serde round-tripping

- **Severity:** low
- **Confidence:** high — attributes and README verified.
- **Location:** `README.md:35` ("Serde serialization and deserialization of the parsed invoice, transaction, and receipt types are part of the core crate"); `document.rs` throughout (`rename(deserialize = "…")`, `deserialize_with = "de::…"`); `archive.rs:391-399`.
- **Evidence:** `Serialize` emits English field names (`invoice_number`, `vat_rate`…, tests archive.rs:124-138), while `Deserialize` expects Hungarian element names and wrapped lists. The JSON the `Archiver` writes cannot be read back into `InvoiceDocument`; `Pdf` has no `Deserialize` at all outside the `de::base64_pdf` path.
- **Why it matters:** A consumer reading the README will expect `serde_json::from_slice::<InvoiceDocument>(archived_json)` to work; it will not.
- **Recommendation:** Reword: "`Serialize` for archival/inspection; `Deserialize` targets the Adatkapcsolat XML shape only." Or add `#[serde(alias = "invoice_number")]`-style symmetric names.

### 15. "Required" has two different meanings: text fields accept empty elements, numeric/date/bool fields do not; whitespace-only text flips to absent

- **Severity:** low
- **Confidence:** high — verified in `document.rs` and quick-xml's `deserialize_opt` (`de/mod.rs:3108-3148`: an empty `Text` event → `None`; `<x></x>` → `Some("")`).
- **Location:** `document.rs:1108-1114` (`required`/`required_text`), `:1495-1515` (`empty_string_as_none`, `empty_as_none`), `:1450-1453` (receipt number "required" yet `archive.rs:253-256` handles it being empty).
- **Evidence:** `<nev></nev>` on `vevo` (as in the official example) → `Some("")` → passes `required_text`; `<nev>  </nev>` → quick-xml yields `None` → fails. `<mennyiseg></mennyiseg>` → `empty_as_none` → `None` → fails `required`. `ReceiptInfo.receipt_number` passes validation as `Some("")` and the archiver then falls back to the id.
- **Why it matters:** Inconsistent semantics make finding 1's failure surface hard to reason about, and a pretty-printer that pads empty elements with whitespace would break parsing of otherwise-valid pushes.
- **Recommendation:** Normalise all text fields through `empty_string_as_none` (trim), then decide per field whether *presence* or *non-emptiness* is required — ideally neither (finding 1).

### 16. Fan-out: sound and well documented, with three sharp edges

- **Severity:** low
- **Confidence:** high — code verified.
- **Location:** `src/fanout.rs:192-227` (`dispatch!`), `:229-240` (`escalate`), `:242-266` (`merge_invoice_acks`).
- **Evidence:** (a) Every member but the last receives a full `clone()` of the document, PDF bytes included. (b) Any single member returning `InvoiceAck::disconnect()`/`Ack::disconnect()` severs the whole connection (`KEY_DEL` wins). (c) If two members supply different `iktatoszam` values, the first silently wins.
- **Why it matters:** (a) is a memory multiplier for multi-MB PDFs × N members; (b) hands a business handler the power to end the account's Adatkapcsolat by mistake; (c) can record the wrong registration number at szamlazz.hu without any signal. The sequential, continue-on-failure, 500-if-any-failed model is the right conservative choice and the redelivery requirement is stated clearly. The type-erasure the module exists for is a genuine need (RPITIT `Handler` is not dyn-compatible), so the complexity is warranted — but it is always compiled into a "protocol" crate.
- **Recommendation:** Wrap the document in `Arc` for read-only members or pass `&InvoiceDocument` through the erased trait; document (b) as a foot-gun (or let `Fanout` be configured to refuse control codes from members); make (c) an error or log a conflict.

### 17. Archiver: minor inefficiency and edge cases

- **Severity:** low
- **Confidence:** high — code verified.
- **Location:** `src/archive.rs:391-399` (`document_json`), `:205-220` (three sequential writes), `:349-367` (`write_version_with` loop), `:369-386` (`undated` branch), `:416-430` (`sanitize`).
- **Evidence:** `document_json` serialises the whole invoice — including the PDF as base64 via `impl Serialize for Pdf` (document.rs:309-314) — into a `serde_json::Value`, then removes `"pdf"`. Writes are XML → PDF → JSON; a failure on the third leaves the first two committed and the delivery answered 500 (redelivery then rewrites/duplicates them). The version loop is unbounded on `AlreadyExists` (practically terminates due to nanosecond+sequence stamps). `undated/` is unreachable for invoices and receipts because `issue_date` is validated as required. `sanitize` maps a name of only dots to `"unnamed"`, so such receipts overwrite each other.
- **Why it matters:** Wasteful allocation on large PDFs; otherwise the semantics (create-only versioning via `if_not_exists`, overwrite-latest) are coherent and the README describes them accurately.
- **Recommendation:** `#[serde(skip_serializing)]` on `pdf` (the archiver already stores it separately; the CLI would also stop dumping base64 — finding 20); bound the version loop; drop or document `undated`.

### 18. `401` for a missing key header lacks `WWW-Authenticate`; Ack `Content-Type` is an assumption

- **Severity:** info
- **Confidence:** high for the RFC point; low for what szamlazz.hu expects (not in fixtures).
- **Location:** `src/axum.rs:257-259`, `:319-326`.
- **Evidence:** `(StatusCode::UNAUTHORIZED, "missing X-Szamlazzhu-Key header")` without the header RFC 9110 §15.5.2 requires; acks are `application/xml; charset=utf-8`.
- **Why it matters:** Purely cosmetic for szamlazz.hu (any non-200 retries). Some strict HTTP clients/proxies complain about 401 without a challenge; 403 or 400 would be equally retry-triggering and RFC-clean. The Content-Type choice is reasonable but unverified (the example response in the docs has no declaration and no stated media type).
- **Recommendation:** Use 403 (or 400) for the missing header, or add a `WWW-Authenticate: X-Szamlazzhu-Key` challenge. Note the Content-Type assumption in docs.

### 19. Test coverage gaps on the protocol path

- **Severity:** medium
- **Confidence:** high — test files verified.
- **Location:** `tests/protocol.rs:4` (`#![cfg(feature = "axum")]`), `:19` (`include_bytes!("synthetic/szamla.xml")` only), `:422-427`, `:487-497`; `Cargo.toml:12` (`exclude = ["tests/upstream"]`); no reference to `tests/upstream/` anywhere in the crate.
- **Evidence:** The official `szamla_example.xml` (comments between elements, `xsi:schemaLocation`, empty `vevo/nev`, `vevo/adoszam`, `orszag`, all-empty `postacim`, `<pdf></pdf>`) is never parsed by any test. All parse-level tests live in `protocol.rs`, which is compiled only with `--features axum`, so `cargo test -p szamlazz-adatkapcsolat` exercises only unit tests and `fanout.rs`. `KEY_ERR` through the router is asserted only for `<szamla>` (`:422-427`); the `<banktranzvalasz>`/`<nyugtavalasz>`/`<szamlabevalasz>` KEY_ERR roots are untested at the router level. `KEY_DEL` is tested only via `Fanout` merge and an `ack.rs` unit test, never end-to-end. No test for BOM, leading whitespace, a prefixed root (`<s:szamla xmlns:s=…>`), `xsi:nil`, or an incoming invoice / bank transaction dispatched through the router to the handler. Ack `Content-Type` is never asserted.
- **Why it matters:** The single most valuable conformance test — "the vendor's own example parses" — is missing, despite the fixture being checked in. BOM/prefix robustness is *correct* (verified in quick-xml source: `slice_reader.rs::remove_utf8_bom`, `de/key.rs::from_elem` uses local names) but only by inheritance from the dependency; a quick-xml upgrade could change it unnoticed.
- **Recommendation:** Add a test that parses `tests/upstream/szamla_example.xml` (workspace-only, `#[cfg]`-guarded on the symlink's presence or via `include_bytes!` with `#[ignore]`-free path) and a synthetic incoming-invoice fixture; split parse-level tests into a feature-free `tests/parse.rs`; add router tests for KEY_ERR on all four roots and one KEY_DEL end-to-end; add BOM/prefix/whitespace-declaration cases.

### 20. CLI `listen` dumps full base64 PDFs to stdout and inherits the KEY_ERR footgun without warning

- **Severity:** low
- **Confidence:** high — code verified.
- **Location:** `crates/szamlazz-cli/src/commands/listen.rs:44-48`, `:26-31`, `:104-116`.
- **Evidence:** `print_json(&invoice)` serialises `InvoiceDocument` including `pdf` (base64) — a multi-MB invoice PDF becomes megabytes of terminal output. The `--adatkapcsolat-key` help says a mismatched key is answered `KEY_ERR` "per protocol" but not that this permanently stops bank/receipt pushes if the receiver is pointed at a live account. The IPN route always answers 200 (fine for a dev tool).
- **Why it matters:** Ergonomics and operational safety of a tool explicitly meant to be exposed via `cloudflared` to real traffic.
- **Recommendation:** Strip `pdf` before printing (or print its byte length); add a one-line warning to the CLI help about KEY_ERR consequences; optionally write received raw bodies to a directory.

### 21. `Handler` cannot grow without a breaking change even though `Document` is `#[non_exhaustive]`

- **Severity:** info
- **Confidence:** high.
- **Location:** `src/handler.rs:56-87`, `src/document.rs:22-38`.
- **Evidence:** All four methods are required (deliberately: "adding a stream … cannot silently acknowledge and discard"). A fifth document type would add a required method → semver-major.
- **Why it matters:** The trade-off is stated and reasonable; just be aware the `#[non_exhaustive]` on `Document` buys nothing for `Handler` users.
- **Recommendation:** None required; consider a `fn unknown(&self, root: &str, raw: &[u8]) -> … { Err }` default method as the future extension point (also serves finding 3).

### 22. Constant-time key comparison is best-effort, not guaranteed

- **Severity:** info
- **Confidence:** high — code verified; header-name case-insensitivity verified in `http` (`HdrName::from_bytes` uses the lowercasing `HEADER_CHARS` table).
- **Location:** `src/axum.rs:328-345`.
- **Evidence:** XOR-fold over `zip` with an early length exit (documented). No `black_box`/`subtle`; LLVM may in principle short-circuit the fold.
- **Why it matters:** Adequate for a high-entropy key over a network; a `subtle::ConstantTimeEq` would make the intent verifiable. `headers.get(KEY_HEADER)` with the mixed-case constant works for lowercase wire headers.
- **Recommendation:** Optionally switch to `subtle` or wrap the accumulator in `std::hint::black_box`; keep the doc note.

### 23. wasm32 claims are plausible from the dependency graph but not verifiable here

- **Severity:** info
- **Confidence:** medium — Cargo manifests read; no CI definition in the repo shows a wasm32 build (`.github/workflows/dagger.yaml` delegates to an external Dagger module).
- **Location:** `crates/szamlazz-adatkapcsolat/Cargo.toml:14-43`, `crates/szamlazz-ipn/Cargo.toml:13-27`, workspace `Cargo.toml` (`axum` `default-features = false`, `jiff` `default-features = false, features = ["std","serde"]`, `quick-xml` `serialize` only), `document.rs:283-286` (`save_to` cfg'd off on wasm), `archive.rs:133-136` (jiff `js` caveat).
- **Evidence:** Default features pull only `base64`, `jiff`, `quick-xml`, `rust_decimal`, `serde`, `thiserror` (adatkapcsolat) and `form_urlencoded`, `jiff`, `rust_decimal`, `thiserror` (ipn) — all wasm-clean without `encoding`/tokio features. `send_wrapper` is a target-specific optional dep referenced by the `axum` feature (allowed by Cargo).
- **Why it matters:** The README (`README.md:11`) makes the claim prominently; a `cargo check --target wasm32-unknown-unknown` in CI would make it a guarantee.
- **Recommendation:** Confirm the Dagger `check` includes a wasm32 target check with `--features axum` and `--features axum,opendal` (the latter with `jiff/js` documented as caller-supplied).

## What is done well

- **XSD coverage is complete and exact.** Every element in `szamla.xsd`, `szamlabe.xsd` (including the incoming-only `folyamatostelj`/`elszDatTol`/`elszDatIg`/`dobdel`), `banktranz.xsd` and `xmlnyugtaarchiv.xsd` (camelCase names `hivasAzonosito`, `nettoEgysegar`, `mennyisegiEgyseg`, `fokonyvVevo`, `rendelesSzam`, `stornozottNyugtaszam`; `stornozott` vs the invoice `sztornozott`) is mapped with the right name and case. One `Party`/`Address`/`Totals`/`VatTotal` per shape with optional side-specific fields is a good de-duplication.
- **Open sets where the domain is open.** VAT codes, `tipus`, `nyelv`, `fizmodunified` are strings; `InvoiceAppearance` preserves unknown integers; unknown elements are skipped; `xs:date` timezone suffixes are stripped rather than rejected; booleans accept `1/0`; decimals go through text (rust_decimal's `FromStr` also accepts `xs:double` scientific notation — verified in 1.42.1).
- **Acks are schema-exact** for all four `*valasz.xsd`: correct root, `xmlns="http://www.szamlazz.hu/{root}"`, `alap/id` + optional `iktatoszam`, `hibakod` limited to `KEY_ERR`/`KEY_DEL`, escaping via `BytesText::new`, XML 1.0 character validation, and the pushed id is always echoed regardless of what the handler supplied (`for_document`).
- **The protocol distinction is right and consistently documented**: control codes are 200 + `<hibakod>` via constructors; failures are non-200; the missing-header → 401 reasoning is sound and matches CONTEXT's "errors mean retry".
- **Auth hygiene**: constant-time-style compare, case-insensitive header lookup, key check before base64 PDF decoding, handler never invoked without a resolved key.
- **Dispatch robustness by construction**: BOM, XML declaration, leading whitespace, comments and a prefixed root all work (verified in quick-xml 0.42 sources).
- **IPN semantics are stated precisely**: absolute snapshot, no delta, no kind discriminator, upsert keyed by endpoint + number, sign-aware `is_fully_paid`, `#[non_exhaustive]` + `new()`, comma tolerance with a written rationale.
- **API design**: opaque `XmlError`, `#[non_exhaustive]` errors and enums, `MaybeSend`/`MaybeSync` for wasm, `Infallible` handler error works (`listen.rs`), Fanout's semantics written down, Archiver's create-only versioning.

## Questions I could not resolve

1. The real IPN parameter set and lexical formats (decimal separator, date format, whether `szlahu_fizetesmod` is always present, whether a proforma snapshot carries anything distinguishing). No IPN fixture or docs extract exists under `fixtures/`; the crate's field list is unverifiable from the repo.
2. Whether szamlazz.hu actually sends every XSD-`minOccurs=1` invoice element for legacy, imported (`forras` 26/28/34) or private-person invoices — especially `privatePersonIndicator`, `katafokonyv`, `fizmodunified`, `vevo/adoszam`, `sztetordering`. The docs' example annotations disagree with the XSD on at least two fields.
3. Whether szamlazz.hu probes the receiver URL (GET/HEAD) at registration; the router answers 405 to anything but POST.
4. What response `Content-Type` (if any) szamlazz.hu requires, and whether the XML declaration in the Ack matters.
5. Maximum push size in practice (largest PDF, largest daily receipt batch) — needed to pick a sane default body limit.
6. Whether an outgoing invoice is re-pushed when it changes (payment recorded, `sztornozott` flips) — the archiver's "overwrite = latest state" design assumes yes.
7. Whether the Dagger CI `check` builds `wasm32-unknown-unknown` and runs tests with `--all-features`; `tests/protocol.rs` is invisible without `axum`.
8. Current szamlazz.hu IPN source IPs (`SOURCE_IPS` says "as of 2025-08-01").
