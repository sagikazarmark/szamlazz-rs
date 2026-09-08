# Reviewer 01: architecture (research-only)

Scope: read-only pass over all six crates, `CONTEXT.md`, `docs/design/restate-szamlazz.md`, and the 2026-09-06 FINAL-REVIEW executive summary/top-10. Prior findings that have since landed (B-04 fallible async `KeyResolver`, B-05 body limit, B-01 lenient parse, J36 e2e now runs in CI via `.dagger/modules/ci/main.dang`, J-07-13 execution span, J2/J3 issue-delay floor, A1 exclusivity rows, J8 checked arithmetic, J5/J11 `OrderKey` alphabet, A2 external-id bound) are not repeated.

---

## (a) Verdict

The workspace is well architected where it matters most and shows its seams in the connective tissue. The layering in `restate-szamlazz` is genuinely deep in the right places: `Gateway` (`gateway.rs`) hides every szamlazz.hu quirk behind eleven async fns whose *only* `Err`s are the two retry-classes (`Unanswered`, `Unconfirmed`), the handler surface (`service/handlers.rs`) is 470 lines of pure attribute + delegation, and every `ctx.run` closure is a one-line `gateway.x(...)` call, so no decision lives inside a durable step. Invariants are overwhelmingly encoded in types (`OrderKey`, `ExternalId` with a compile-time length proof, `InvoiceNumber`, `CorrectionId`, `Namespace`, `Endpoint`, `Body<T>`, un-serialisable `Credentials`). The weaknesses are structural repetition rather than wrong boundaries: the same `{code, message}` → `Fault` mapping is written ~23 times across the service layer over ten outcome enums that each re-declare the same two variants; the handler protocols in `service/create.rs` and `service/storno.rs` interleave pure decisions with `ctx`-bound reads inside single async fns, so the sequencing logic (which is where the exactly-once argument lives) can only be exercised under a real Restate server; the `xmlszamlavalasz` envelope parser lives inside `ops/invoice.rs` and is imported by four sibling ops; and the `tipus` code is a bare `String` matched against literals in six places. None of these are correctness risks; all of them raise the cost of the next change and keep meaningful logic behind heavy infrastructure.

---

## (b) Strengths

1. **Outcome-as-data discipline is uniform across the gateway.** All read fns return `Result<XOutcome, Unanswered>` and both write fns `Result<XOutcome, Unconfirmed>` (`gateway.rs:903`, `:1005`, `:1281`, `:1295`, `:1306`, `:1323`, `:1363`, `:1400`, `:1459`); the two exceptions (`delete_proforma`, `set_payments`, `:1627`, `:1648`) return the outcome directly with a `Transport` variant because they run under `run_once`. The split between "answered" and "unanswered" is made in exactly one place, `QueryError::answered` (`gateway.rs:620-630`), and every read fn goes through it.

2. **Nothing decides inside a `ctx.run`.** Every closure is `move || async move { gateway.<fn>(…).await }` (`create.rs:674-683`, `:709-719`; `support.rs:758-760`, `:773-775`, `:812`, `:845-855`, `:879`, `:910`; `agent.rs:168-170`, `:193-195`, `:215-217`, `:242`; `storno.rs:176`). Decisions are made on the journaled result outside the closure. This is the property ADR 0005 depends on and it holds everywhere.

3. **The handler layer is thin to the point of being generated.** `handlers.rs:43-291` and `:299-471` contain only `#[handler]` attributes, `into_request()?`, `order_key()?`, and `self.execute(ctx, |execution| …)`. The three-step entry (decode body → parse key → prologue) is identical on every handler and the order is enforced by structure, not convention.

4. **Types carry the invariants, with proofs.** `identity.rs:288-314` is a `const _: () = { assert!(…) }` block proving every external-id shape fits `ExternalId::MAX_LEN` from the parts' `MAX_LEN` constants; `CorrectionId::validate` refuses `ExternalId::TOKENS` (`contract.rs:104`) so no composition reads as another; `Endpoint::parse` refuses userinfo (`account.rs:200-205`); `Body<T>`'s SDK `Deserialize` is `Infallible` so decode failures are the handler's structured fault (`service/body.rs:51-60`); `Secret` and `AgentKey` have redacting `Debug` (`config.rs:340-344`, `credentials.rs:27`).

5. **Journal contract is enforced, not described.** The `Journaled` marker trait is the bound of every run helper (`support.rs:33-45`, `:648`, `:678`, `:710`) and links each `ctx.run` site to `tests/journal/<type>/` fixtures; the `Run-name pin` (`tests/e2e/harness/run_names.rs`) pins step order against `sys_journal`.

6. **Sans-IO core in `szamlazz-agent` is clean.** `AgentRequest` (`wire.rs:346-396`) is `to_wire(&Credentials) -> WireRequest` + `parse(&RawResponse)`; `WireRequest` carries no URL (`wire.rs:10-13`); `client.rs` is 246 lines and only adds transport (`client.rs:218-245`). `restate-szamlazz` depends only on public `ops::*`, `wire::{AgentRequest, RawResponse}` (tests), `client::BuildError`, and re-exported types; no `pub(crate)` reach-in.

7. **Endpoint/library config duplication is pinned.** The endpoint's hand-maintained key tree (`endpoint/src/config/schema.rs:48-112`) is compared field-for-field against the library types' `Serialize` output (`schema.rs:351-359`), so `Defaults`/`SellerConfig`/policy drift fails a unit test.

8. **Pure decision fns already exist where extracted.** `Identity::respond_to` (`create.rs:96-166`), `Lookup::classify` (`support.rs:492-513`), `verified_document` (`:304-319`), `storno_response` (`:383-417`), `storno_number_from_hint/lookup` (`:429-466`), `StornoIntent::from_verified` (`:353-371`), `prologue::{resolution, account_of, fetch_fault}` (`prologue.rs:184-208`, `:298-317`), `agent::{query_response, taxpayer_response, set_payments_response}` (`agent.rs:85-150`) are all tested without infrastructure. The pattern is established; the gap is that it is applied to roughly half the decisions.

---

## (c) Findings

### F-01 · Handler protocol sequencing is fused with `ctx`-bound reads, so the create/storno decision trees are testable only under Restate
**Severity: medium · Effort: M**
- `create.rs:247-305` (`issue_kind`), `:603-657` (`issue`), `:429-456` (`exclusivity`), `:459-491` (`prepayment_for_final`), `:506-596` (`proforma_link`); `storno.rs:24-96` (`storno`), `:102-134` (`verify_for_storno`), `:139-196` (`delete`), `:206-245` (`status`).
- **Problem.** Each fn takes `&ObjectContext`, performs a read via `object::lookup/verify`, and then makes a 5–10-arm pure decision on the journaled result *in the same body*. E.g. `issue()`'s match on `LookupOutcome` (`create.rs:614-648`) decides reissue-vs-conflict-vs-reversed, the core of ADR 0003, and is reachable only by driving a Restate server through the e2e (`tests/e2e/main.rs:88-157`, one `#[tokio::test]`, ~60 scenarios, `#[ignore]`). `verify_for_storno`'s four `Break` arms (`storno.rs:115-132`) and `proforma_link`'s `Number` branch (`create.rs:561-593`) likewise. The design doc acknowledges this (`docs/design/restate-szamlazz.md:849-851`: "handler decisions are tested end to end or as the pure functions they are extracted into") but the extraction is partial.
- **Recommendation.** See Seam S-1 below: split each into `async fn read…` + `fn decide…(outcome, &Identity/…) -> Result<ControlFlow<Response, Carry>, Fault>`. The async shell becomes a 3–5-line "read, decide, read, decide" sequence.

### F-02 · The same `{code, message}` variant pair is declared in ten outcome enums and mapped to the same two `Fault`s ~23 times
**Severity: medium · Effort: M**
- Variant declarations: `CredentialsRejected { code, message }` and `Api { code, message }` in `LookupOutcome` (`gateway.rs:166-181`), `CreateOutcome` (`:271-289`), `QueryOutcome` (`:494-507`), `ProbeOutcome` (`:528`), `TaxpayerOutcome` (`:554-567`), `QueryError` (`:582-595`), `StornoLookupOutcome` (`:651-664`), `StornoOutcome` (`:719-733`), `DeleteOutcome` (`:760`), `SetPaymentsOutcome` (`:811`), plus the private `Answer` and `Failure`.
- Mapping sites: `Fault::credentials_rejected(...)` × 14 (`agent.rs:92,109,146,318`; `create.rs:154,565,619`; `storno.rs:73,186`; `support.rs:316,408,440,463,502`) and `Fault::inconclusive_answer(...)` × 9 (`agent.rs:322`; `create.rs:160,562,616`; `storno.rs:76`; `support.rs:314,411,500`).
- **Problem.** This is the J-05-04 observation from the prior review, still present. The journaled-type rule (additive-only) makes the enum shapes hard to consolidate retroactively, but the *mapping* is not journaled and can be. Every new handler re-types the same two arms; a change to the fault text (e.g. #63's `szlahu_down` wording) had to be applied at each site.
- **Recommendation.** Introduce one non-journaled `enum AnsweredCode { Credentials{code,message}, Other{code,message} }` and a trait `impl AsAnsweredCode for <each outcome>` with `fn answered_code(&self) -> Option<AnsweredCode>`; then a single `fn fault_for(code: AnsweredCode, namespace) -> Fault`. Each handler match collapses two arms into `x if let Some(c) = x.answered_code() => return Err(about(fault_for(c, ns)))`. The journaled enums stay byte-identical. Alternatively, since every write outcome is *built* in `gateway.rs`, add `impl From<Answer> for LookupOutcome` etc. so the construction side is also single-sourced.

### F-03 · The `xmlszamlavalasz` envelope parser lives inside `ops/invoice.rs` and is imported by four sibling ops
**Severity: medium · Effort: S**
- `ops/invoice.rs:1110-1198` (`parse_creation_result`), `:1283-1310` (`decimal_body_or_header`, `parse_issued`), `:1312-1403` (`InvoiceResponse`, `MinimalInvoiceResponse`); consumers: `ops/credit_entry.rs:9`, `ops/query_pdf.rs:95`, `ops/query_xml.rs:557`, `ops/storno.rs:8`.
- **Problem.** `ops/invoice.rs` is 2238 lines because it hosts three things: ~790 lines of request types (incl. carrier/waybill types `:180-290` that only the delivery-note case uses), the `AgentRequest` impl (`:788-1108`), and the *shared response envelope* (`:1110-1403`), followed by ~830 lines of tests. The envelope is the one piece every issuing/registering op depends on; having `storno.rs` reach into `invoice::parse_issued` inverts the natural dependency direction (sibling ops depend on one op's internals).
- **Recommendation.** Move `:1110-1403` to `ops/response.rs` (or `xml::valasz`) as `pub(crate)`; move `BuyerLedger/TransOFlex/PickPackPoint/Sprinter/Mpl/Waybill` (`:180-290`) to `ops/invoice/waybill.rs`. `invoice.rs` drops to ~1200 lines with tests and reads as one op. No public API change (everything moved is `pub(crate)` or re-exportable).

### F-04 · `tipus` is a bare `String` compared against literals in six places
**Severity: low · Effort: S**
- `szamlazz-agent/src/ops/query_xml.rs:209` (`pub document_type: String`); worker literals: `gateway.rs:453` (`== "SS"`), `:1732-1740` (`document_type_of`), `:1745-1754` (`issued_kind_of`), `:1761` (`is_invoice_family`), `service/create.rs:582` (`!= "D"`), `service/storno.rs:130` (`matches!(…, "SZ" | "ES" | "VS" | "HS")`).
- **Problem.** The worker already has two half-typed conversions (`document_type_of`, `issued_kind_of`) but `storno.rs:130` and `create.rs:582` bypass them with fresh literal sets. The "stornoable" set (`SZ|ES|VS|HS`) and the "invoice family" set (`SZ|ES|VS`) are two different tables with no type tying them together. A new `tipus` code from szamlazz.hu (or a typo) is invisible to the compiler.
- **Recommendation.** Add an open enum `DocumentType { Invoice, Proforma, Prepayment, Final, Corrective, Storno, DeliveryNote, Other(String) }` in `szamlazz-agent::ops::query_xml` (same pattern as `VatRate`/`PaymentMethod::Other`; additive to the response type; check the journal fixtures still decode since `InvoiceDocument` is journaled; a `String`→open-enum retype with `#[serde(from = "String")]` keeps the JSON identical). Then `InvoiceDocumentExt::is_stornoable()` and `is_invoice_family()` become matches on variants and `issued_kind_of` goes away.

### F-05 · `ExternalId::for_storno` / `for_unmanaged_storno` take `&str`, disconnecting the compile-time length proof from the API
**Severity: low · Effort: S**
- `identity.rs:249` (`original_number: &str`), `:259` (`number: &str`); proof assumes `NUMBER = InvoiceNumber::MAX_LEN` at `:293`, `:309`, `:311`. Callers convert the validated `InvoiceNumber` to `String` at entry: `storno.rs:34`, `agent.rs:264`, `create.rs:319`.
- **Problem.** The proof at `:288-314` holds today because the only callers happen to start from a validated `InvoiceNumber`, but nothing in the signature says so; `ExternalId::new(impl Into<String>)` (`:226-228`) is also a public unchecked constructor on a type whose docs promise `MAX_LEN`. The placeholder `ExternalId::new("-")` at `create.rs:397` is the one legitimate internal use.
- **Recommendation.** Change the two constructors to take `&InvoiceNumber` (contract type) and keep the number as `InvoiceNumber` through `StornoIntent` (`support.rs:327`) and `StornoStepRequest` (`gateway.rs:671`), converting to `&str` only at the wire call. Make `ExternalId::new` `pub(crate)` or `#[doc(hidden)]`.

### F-06 · `validate_document` validates by building the request with placeholder refs, then the request is built again
**Severity: low · Effort: S**
- `create.rs:384-405` (`validate_document` → `self.build(kind, document, order, &ExternalId::new("-"), DocumentRefs { proforma: None, prepayment: Some("-"), corrected: Some("-") })`), then the real build at `:287-297` and `:340-349`.
- **Problem.** The validation is a side effect of `Gateway::build_create` (`gateway/build.rs:83-189`) rather than a function of its own, so it needs fake identity inputs to run. It works, but the intent ("does this `DocumentInput` project?") is expressed as "build a throwaway `CreateInvoice`", and a future reference-dependent validation in `build_create` would be silently satisfied by the `"-"` placeholders.
- **Recommendation.** Split `build_create` into `fn validate_input(&Defaults, &DocumentInput) -> Result<Validated, InputError>` (currency, language, exchange rate, items → `Vec<LineItem>`) and `fn assemble(Validated, kind, order, external_id, refs) -> CreateInvoice` (infallible except `MissingReference`). `prepare()` calls the first; `issue_kind` passes `Validated` to the second. Removes the double build and the placeholders.

### F-07 · The three run-policy configs are three field-identical structs with three identical `run_retry_policy` bodies
**Severity: low · Effort: S**
- `config.rs:531-546` (`IssueConfig`), `:626-640` (`ReadConfig`), `:686-698` (`ResolveConfig`, minus `max_attempts`); `run_retry_policy` at `:587-594`, `:659-666`, `:715-721`; `WorkerConfig::validate` iterates them as tuples at `:106-150`.
- **Problem.** The difference between them is semantic (floor on issue, no attempt cap on resolve), not structural, but the structure is copied. `validate` already has to destructure all three into a homogeneous tuple to iterate, which is the tell. (Already tracked as J-05-03 in #68.)
- **Recommendation.** One `struct RunPolicy { max_attempts: Option<u32>, initial_delay, factor, max_delay, max_duration }` with `impl RunPolicy { fn run_retry_policy(&self) }`, and `WorkerConfig { issue: RunPolicy, read: RunPolicy, resolve: RunPolicy }` with the floor check keyed on `Policy::Issue`. Not journaled (`WorkerConfig` is not a `Journaled` type; only `Namespace` is pinned), so the change is free of replay concerns. If the `[resolve]` table must keep refusing `max_attempts`, keep `ResolveConfig` as a newtype over `RunPolicy` with `deny_unknown_fields`; the schema tree at `endpoint/config/schema.rs:69-74` already distinguishes it.

### F-08 · `journal_helpers!` stamps 13 fns × 3 context types; `Execution` methods are then written once per context type anyway
**Severity: low (structural cost, documented) · Effort: L (blocked on SDK)**
- `support.rs:521-925` (macro + three invocations), justified at `:516-520` (rust-lang/rust#100013). Consumers pick a module: `object::` in `create.rs:22`/`storno.rs:12`, `shared::` in `storno.rs:12`, `service::` in `agent.rs:20`.
- **Problem.** Not fixable in the crate today; noted because it is the reason there is no `trait Durable` seam (see S-2) and why `status` (`storno.rs:206`) had to be written against `SharedObjectContext` while every other `Order` handler is against `ObjectContext`. The `#[allow(dead_code)]` at `:523-527` hides which of the 39 copies are actually used.
- **Recommendation.** Leave as is; revisit when the SDK exposes a non-sealed run trait or when `async fn` in traits with `Send` bounds is fixed. Consider a per-module `pub(in crate::service) use` list instead of `dead_code` so unused copies surface.

### F-09 · Rustdoc drift after the account-pin amendment (ADR 0006)
**Severity: low · Effort: S**
- `gateway.rs:17-19` ("its ownership-validation pins"), `:114-115` and `:187-188` ("validated against the gateway's own `Account`"), `Gateway::is_ours` at `:879-881` ignores `self`; `support.rs:3-4` ("the account check of documents found by number"); `docs/design/restate-szamlazz.md:872-873` ("the gateway validates found documents against the account it was opened for").
- `lib.rs:172-175` and `contract.rs:4-7` still say a `TerminalError` "always means 'outcome unknown, retry with a new Idempotency-Key'", contradicting `TerminalCode`'s own docs at `contract.rs:485-492` and `CONTEXT.md` *Outcome* (only three of seven codes mean that). This is the C-A finding, fixed in the READMEs and glossary but not in the crate-root rustdoc.
- **Recommendation.** Five one-line edits. Consider a doctest that asserts `TerminalCode::ALL.iter().filter(|c| c.is_outcome_unknown()).count() == 3` and link to it from the module docs so the claim can't drift again.

### F-10 · `identity.rs` hosts `normalize_buyer_name`
**Severity: low · Effort: S**
- `identity.rs:334-343`, used by `gateway/build.rs:165` and `create.rs:390`.
- **Problem.** Buyer-name NFC normalisation is a wire-hygiene concern of the create projection, not of order/document identity. Minor cohesion smell in an otherwise tightly scoped module.
- **Recommendation.** Move to `gateway/build.rs` (its primary consumer) or `contract/document.rs` as `BuyerInput::normalized_name()`.

### Verified non-findings (for the record)
- `gateway.rs` at 2050 lines: ~850 lines are outcome-type definitions with substantial rustdoc (`:110-848`), ~860 lines are `impl Gateway` (`:850-1711`), ~170 lines are tests. Splitting into `gateway/outcomes.rs` + `gateway/mod.rs` would be navigational only; the impl is cohesive (one struct, one concern). Not recommended as a priority.
- `config.rs` at 1381 lines: ~540 lines of tests. The non-test part is four unrelated things (`WorkerConfig`, `Namespace`, `Secret`, `Defaults/SellerConfig`, three policies, duration parsing). `Secret` is used only by `static_resolver.rs:90` and `Defaults/SellerConfig` are account-shaped; moving those to `account.rs` would tighten both modules but is optional.
- `contract.rs` at 1177 lines: ~570 lines of tests; the two bounded newtypes (`CorrectionId`, `InvoiceNumber`) are ~300 lines of near-identical boilerplate (`:74-211`, `:230-361`); a `bounded_string!` macro would halve it but the duplication is mechanical, not logical.
- `document.rs` (adatkapcsolat) at 1944 lines: lenient parse + opt-in validate + `de` helpers + tests for four document kinds. Cohesive; the earlier B-01 fix is in (`:1-25` module doc). The J-05-01 duplicate `<szamla>` model vs `query_xml.rs` remains a known, scheduled item.
- Cross-crate coupling: `restate-szamlazz` → `szamlazz-agent` uses only public items; the endpoint → library boundary is clean (`EndpointConfig::from(Layout)` assembles `WorkerConfig`/`StaticConfig`, `config.rs:140-162`). No inverted dependencies.

---

## (d) Top seam-extraction opportunities

Ordered by leverage (branches unlocked ÷ effort).

### S-1 · Split the `Execution` protocol fns into `read` + pure `decide` (create.rs, storno.rs)
**Where:** `create.rs:614-648` (lookup decision), `:447-455` (exclusivity), `:477-490` (prepayment-for-final), `:527-546` and `:561-593` (proforma link, both branches); `storno.rs:115-132` (verify-for-storno), `:157-167` + `:180-195` (delete), `:223-243` (status fold).
**Seam:** for each, a `fn decide_*(outcome: <Journaled outcome>, ctx-free inputs…) -> Result<ControlFlow<Response, Carry>, Fault>` in the same module, next to the existing `Identity::respond_to` (`create.rs:96`). Concretely:
```rust
fn decide_lookup(outcome: LookupOutcome, reissue: bool, identity: &Identity, ns: &Namespace)
    -> Result<ControlFlow<CreateResponse, Option<String>>, Fault>;
fn decide_exclusivity(found: Lookup, reason: ConflictReason, identity: &Identity) -> Option<CreateResponse>;
fn decide_prepayment_for_final(found: Lookup, identity: &Identity, refs: &mut Refs) -> Option<CreateResponse>;
fn decide_proforma_by_number(outcome: QueryOutcome, number: &str, order: &OrderKey, identity: &Identity, ns: &Namespace, refs: &mut Refs)
    -> Result<Option<CreateResponse>, Fault>;
fn decide_verify_for_storno(found: Box<InvoiceDocument>, order: &OrderKey, number: &str)
    -> ControlFlow<StornoDecision, Box<InvoiceDocument>>;   // where StornoDecision::AlreadyReversed carries "needs hint"
fn decide_delete(found: Lookup, force: bool) -> ControlFlow<DeleteProformaResponse, Box<InvoiceDocument>>;
fn fold_status(found: [(DocumentKind, Lookup); 4]) -> OrderStatus;
```
The async fns become: read → `decide` → `?`/`return`. `test_support::Doc` (`test_support.rs:132-245`) already builds `InvoiceDocument` fixtures, so tests are cheap.
**Tests enabled (no server, no wiremock):** reissue-on-live → `conflict{live}`; reversed-without-reissue → `reversed{storno_number}`; reversed-with-reissue → proceed carrying the number; foreign → `conflict{foreign}`; exclusivity on a *reversed* other-kind → proceed (the A1 fix's negative case); `prepayment_for_final` on `Absent`/`Collision`/reversed/live; `proforma_link` `None` on live → `conflict{proforma_live}`, `{number}` on another order's proforma → `conflict{not_managed}`, on a non-`D` → `invalid_input`; every `verify_for_storno` `Break` arm; `delete` on paid without force; `status`'s consumed-proforma derivation (`storno.rs:230-243`) on all four `invoice`/`prepayment` × `referenced_proforma` combinations. That is ~25 branches currently reachable only through `tests/e2e`.

### S-2 · A `Steps` seam so `Execution` protocols can be driven by a scripted journal
**Where:** `support.rs:640-716` (`run_once`, `run_retrying`, `run_reading`, `run_best_effort`), the only touch points between `Execution` and the SDK.
**Seam:** an object-safe trait the crate owns, implemented for the three SDK contexts by the existing macro and by an in-memory `ScriptedSteps` in tests:
```rust
pub(super) trait Steps: Send + Sync {
    fn run_json<'a>(&'a self, name: String, policy: RunRetryPolicy,
                    f: Box<dyn FnOnce() -> BoxFuture<'a, Result<serde_json::Value, BoxError>> + Send + 'a>)
        -> BoxFuture<'a, Result<serde_json::Value, TerminalError>>;
    fn scope(&self) -> Option<&str>;
}
```
with the typed `run_reading<T: Journaled>` becoming a thin generic wrapper that serialises through `Value`. `Execution` methods take `&dyn Steps` instead of `&ObjectContext`. Boxing sidesteps rust#100013 because there is no generic `async fn` over the sealed traits, only a concrete impl per context type.
**Caveat:** the `Journaled` bound is preserved (the wrapper requires it), so the fixture link is intact; cost is one `serde_json::Value` round-trip per step, negligible against a 60 s HTTP call.
**Tests enabled:** full-protocol tests of `issue_kind`/`correct`/`storno`/`delete`/`status` with a `ScriptedSteps` that returns pre-recorded outcomes per step name and records the step-name sequence, which would make `RUN_NAMES` (`tests/e2e/harness/run_names.rs`) assertable per handler without a server, and would let "the create step is never reached when exclusivity refuses" be a unit assertion. Combined with S-3 this removes the last reason the protocol layer needs Restate for anything but the SDK-level behaviours (replay, cancel, retention).

### S-3 · A `trait SzamlazzGateway` (or `Gateway` as a generic parameter) between `Execution` and the HTTP client
**Where:** `prologue.rs:32-35` (`Execution { gateway: Arc<Gateway>, … }`), `gateway.rs:104-108`.
**Seam:** `Execution<G: GatewayOps = Gateway>` where `GatewayOps` has the eleven read/write fns (`lookup`, `create`, `verify`, `query`, `hint`, `lookup_storno`, `storno`, `delete_proforma`, `set_payments`, `query_taxpayer`, `probe`) plus `account()`, `build_create()`. Generic rather than `dyn` since the run closures need `'static + Send` futures; `Arc<G>` clones into the closures as `Arc<Gateway>` does today (`create.rs:669`, `:700`).
**Tests enabled:** the same protocol tests as S-2 but scripted at the szamlazz.hu-answer level rather than the journal level (e.g. "lookup answers `Reversed`, create answers `Issued`" → `outcome: issued` with the reversed number carried through `CreateStepRequest.reversed`). Also lets `tests/gateway.rs` stay as the sole wiremock suite while the ~40 protocol scenarios in `tests/e2e/create_*.rs`, `storno.rs`, `delete_proforma.rs`, `get.rs` gain fast unit twins. Without S-2 this seam alone still needs a ctx, so pair with S-1 or S-2.

### S-4 · Single-source the answered-code → `Fault` mapping (see F-02)
**Where:** the 23 sites listed in F-02.
**Seam:** `trait AnsweredCode { fn answered(&self) -> Option<Answer> }` on every outcome enum + `fn fault_of(Answer, &Namespace) -> Fault` in `support.rs`.
**Tests enabled:** one table test asserting that every outcome enum's `CredentialsRejected` → `credentials_rejected`/503 and `Api` → `unavailable`/503 (or `szamlazz_error`/422 where the handler chooses pass-through), replacing the per-handler tests at `create.rs:974-1016`, `agent.rs:401-450`, `tests.rs:899-954` with one exhaustive matrix; and, more importantly, making "a new outcome enum forgot the credentials arm" a compile error rather than a missing e2e scenario.

### S-5 · Separate `validate_input` from `assemble` in `Gateway::build_create` (see F-06)
**Where:** `gateway/build.rs:83-189`, `create.rs:384-405`.
**Seam:** `fn validate_input(defaults: &Defaults, document: &DocumentInput) -> Result<ValidatedDocument, InputError>` (pure, needs no `Gateway`), `fn assemble(&self, validated: ValidatedDocument, kind, order, external_id, refs) -> Result<CreateInvoice, InputError>`.
**Tests enabled:** the existing `build.rs` tests (`:255-616`) split into input-validation tests that need no `Gateway::open` (currently every one builds a `Gateway` with a fake `Credentials` and a `127.0.0.1:1` endpoint, `build.rs:236-245`) and assembly tests; `prepare()`'s tests in `create.rs:816-900` likewise drop the `Execution` fixture (`:747-756`). Removes the last reason contract-level validation touches an HTTP client type.

---

**Priority suggestion:** S-1 first (small, no new abstractions, unlocks the most branches); S-4 second (removes the largest repetition and adds a compile-time guard); S-5 third; S-2/S-3 as a pair only if the team wants the protocol layer unit-tested end to end, since the e2e now runs in CI and covers it today.
