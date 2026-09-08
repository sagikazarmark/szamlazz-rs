# Architecture review, szamlazz-rs workspace (v0.3.0, restate-* unreleased)

Reviewer: architecture / module-boundary axis. Read-only; no cargo run. All line numbers are as of HEAD `0e4238c`.

## Summary

This is an unusually well-reasoned workspace for its age. The two hard problems: a remote API that is hostile to idempotency (no idempotency key on document creation, non-unique external ids, a duplicate-order-number refusal that can contradict the query surface, a storno that echoes success on a no-op) and a durable-execution runtime whose journal outlives deployments; are met with a small number of deep modules: `szamlazz_agent::wire` (one trait, one envelope, every op), `restate_szamlazz::gateway` (one plain async fn per durable step, every expected outcome as data), `restate_szamlazz::identity` (deterministic external ids) and the two-step lookup/create protocol in `service/create.rs`. The layering `Szamlazz.Order → gateway ← Szamlazz.Agent` is a compile-time fact as ADR 0001 claims, no Restate service calls another, the glossary vocabulary is followed almost everywhere in code and docs, and every crate's `lib.rs` module doc is accurate. The complexity that exists is mostly *earned* by the domain.

Where the workspace is weaker is in **duplicated modelling** rather than in structure: the same `<szamla>` XML document is modelled twice (agent `query_xml.rs` vs adatkapcsolat `document.rs`, ~1,000 lines each, divergent field types and names), the lenient serde helpers are copied between those two crates, three field-identical retry-policy config types exist where one would do, `CredentialsRejected { code, message }` is repeated as a variant in 11 enums, and the agent's blanket `#[non_exhaustive]` on request types (77 attributes across 96 public types) pushes every consumer (including this workspace's own restate crate) into `new()` + ten lines of field assignment. Of the three released protocol crates, only the `<szamla>` duplication is expensive to fix; everything in `restate-*` is unreleased and cheap to change now.

Overall: **sound architecture, deep where it matters, with a handful of medium-severity duplication and one high-severity API-ergonomics issue in the released agent crate.** No critical findings.

---

## Findings

### 1. The `<szamla>` invoice document is modelled twice, divergently

- **Severity:** high
- **Confidence:** high, both parsers target root `szamla` in namespace `http://www.szamlazz.hu/szamla` (`crates/szamlazz-agent/src/ops/query_xml.rs:599`, `crates/szamlazz-adatkapcsolat/src/document.rs:147`).
- **Location:** `crates/szamlazz-agent/src/ops/query_xml.rs` (1,630 lines, 17 pub types, 2-layer wire→public design) vs `crates/szamlazz-adatkapcsolat/src/document.rs` (1,782 lines, 25 pub types, direct-serde design).
- **Evidence:** The two `InvoiceInfo` structs each have ~28 fields with identical wire names (`gazdEsemAzon`, `hivszamlaszam`, `fizmodunified`, `keszpenz`, `rendelesszam`, …) but different Rust shapes: agent `id: u64` / adatkapcsolat `id: i32`; `document_type: String` / `kind: Option<String>`; `cash_payment: bool` / `cash: Option<bool>`; `unified_payment_method` / `payment_method_unified`; `InvoiceNumber` newtype / `String`; `e_invoice: InvoiceAppearance` / `Option<InvoiceAppearance>`, two distinct `InvoiceAppearance` enums (`query_xml.rs:124`, `document.rs:209`), two `Pdf` newtypes (`types.rs:92`, `document.rs:269`), two `VatRate` types (owned enum vs `VatRate<'a>`), two `Address`/`Party`/`Supplier` pairs. The lenient serde helpers are also duplicated: `empty_as_none`, `flexible_bool`, `optional_flexible_bool` in `szamlazz-agent/src/xml.rs:164-228` vs `empty_as_none`, `flexible_bool`, `opt_flexible_bool` in `document.rs:1490-1666`, and only the adatkapcsolat copy handles `xs:date` timezone suffixes (`opt_xs_date`), so the two parsers differ in robustness on the same wire format.
- **Simpler alternative:** One shared no-IO crate (`szamlazz-xml` or `szamlazz-types`) holding the `szamla` document model, `Pdf`, `VatRate`, `InvoiceAppearance`, and the `de` helpers; both crates depend on it, the agent's `QueryInvoiceXml::Response` and adatkapcsolat's `Document::OutgoingInvoice` become the same type. A receiver could then store what the agent queries without a mapping layer.
- **Worth the churn?** **Later, before 1.0.** It is a breaking change to two released crates and touches ~2,000 lines, so not for a patch; but the longer it waits the more consumers pin one shape or the other. If the owner does not want a fourth crate, at minimum unify the `de` helpers (agent could adopt the `xs:date`-tolerant parser today with no API change).

### 2. Blanket `#[non_exhaustive]` on request/input structs makes the agent API verbose for every consumer

- **Severity:** high (materially hurts users of the released crate)
- **Confidence:** high, counted and observed in this workspace's own consumer.
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs` (`Buyer` :379, `InvoiceHeader` :254, `CreateInvoice` :572, `Seller` :335, `Waybill` :231 …), `ops/storno.rs:52`, `ops/receipt.rs`, `item.rs:39`.
- **Evidence:** 77 `#[non_exhaustive]` attributes across the agent's 96 public types, applied to *input* structs with all-`pub` fields. From another crate this forbids both struct literals and `..Default::default()` update syntax, so the only path is `X::new(required…)` followed by one assignment per optional field. `Buyer` has 17 fields, `InvoiceHeader` 17, `CreateInvoice` 14. The restate crate pays this tax at `contract/document.rs:202-217` (`Buyer::from(BuyerInput)`: `new()` + 10 assignments), `contract/document.rs:236-246`, `gateway/build.rs:128-167` (`new()` + 8 header assignments, `new()` + 6 create assignments), `config.rs:446-479`. The CLI sidesteps it entirely by deserialising `CreateInvoice` from JSON (`szamlazz-cli/src/commands/invoice.rs:136`), which is telling.
- **Simpler alternative:** Keep `#[non_exhaustive]` on *response* types (the server can add fields) and drop it on request/input structs, accepting that adding a request field is a 0.x minor bump (it already is under cargo's 0.x semver). Alternatively keep it and ship a real builder (`Buyer::builder().name(..).zip(..).email(..).build()`), but that is more code for less benefit than plain struct literals.
- **Worth the churn?** **Yes, now.** Removing an attribute is not itself a breaking change for existing callers; it only widens what they may write. Do it before 1.0, when the decision becomes permanent.

### 3. Three field-identical retry-policy config types

- **Severity:** medium
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/config.rs:491-654`.
- **Evidence:** `IssueConfig` (:493) and `ReadConfig` (:560) have the same five fields and byte-identical `run_retry_policy()` bodies (:529-536 vs :593-600); `ResolveConfig` (:618) is the same minus `max_attempts`. Together with three `Default` impls and the `Policy` enum used by `WorkerConfig::validate` (:98-139, which iterates the three by hand) this is ~170 lines expressing one concept. The endpoint's schema tree then mirrors the split again (`CAPPED_POLICY` / `RESOLVE_POLICY`, `schema.rs:60-74`).
- **Simpler alternative:** One `RetryPolicyConfig { max_attempts: Option<u32>, initial_delay, factor, max_delay, max_duration }` with three `const fn` default constructors (`issue_defaults()`, `read_defaults()`, `resolve_defaults()`) used via `#[serde(default = "…")]`; `validate` becomes a loop over `[(Policy::Issue, &self.issue), …]`. The type-level distinction buys nothing: nothing dispatches on the type, only the defaults differ.
- **Worth the churn?** **Yes.** Unreleased crate; ~100 lines removed; the journal is unaffected (policies shape no journal entry, config.rs:487).

### 4. `CredentialsRejected { code, message }` repeated as a structural variant in eleven enums

- **Severity:** medium
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/gateway.rs:146, 251, 420, 453, 479, 507, 536, 576, 639, 664, 714`; consumers in `service/*.rs`.
- **Evidence:** 52 occurrences of `CredentialsRejected` in `gateway.rs` alone; `Fault::credentials_rejected(&namespace, code, message)` is called 20 times across `service/`. The `QueryOutcome` four-arm match (`Api → inconclusive_answer`, `CredentialsRejected → credentials_rejected`, `NotFound → …`, `Found → …`) is written out 6–7 times (`service/create.rs:299-316`, `service/agent.rs:250-267`, `service/storno.rs`, `support.rs:669-681` …). `storno_response` (`support.rs:264-289`) returns `Result<StornoResponse, (String, String)>`, a tuple standing in for the missing struct.
- **Simpler alternative:** (a) A shared `pub struct RejectedCredentials { code: String, message: String }` nested in each enum, same journal JSON shape (serde flattens a struct variant identically if you keep the field names), one place to document the codes 3/135/136/164. (b) A `QueryOutcome::into_found(&Namespace) -> Result<Option<Box<InvoiceDocument>>, Fault>` helper that owns the `Api`/`CredentialsRejected` arms, so handler bodies match only `Some`/`None`. I would *not* lift credential rejection into a `TerminalError` raised inside the closure: the probe needs it as data (`ProbeOutcome`, `check_account`), and data-not-error is the module's stated invariant.
- **Worth the churn?** **Yes** for (b), pure deletion of repeated arms, no journal change. **Yes, but check fixtures** for (a), the journal fixtures under `tests/journal/*/credentials-rejected.json` must re-encode byte-identically; with `#[serde(flatten)]` or matching field names they will.

### 5. `service/support.rs` is a grab-bag; the `journal_helpers!` macro triplicates 340 lines

- **Severity:** medium
- **Confidence:** high on the grab-bag; medium on the macro (the rust-lang/rust#100013 justification at `support.rs:343-345` is plausible and I could not compile to disprove it).
- **Location:** `crates/restate-szamlazz/src/service/support.rs` (690 lines).
- **Evidence:** Six unrelated concerns in one file: `Journaled` marker + 11 impls (:22-44), `Fault` and the fault→`TerminalError` mapping (:46-191), `order_key` parsing (:193-211), `check_pins` (:213-245), `StornoIntent`/`storno_response` (:247-289), `Lookup` classification (:291-339), then a 340-line `macro_rules!` (:346-686) instantiated three times (:688-690) for `ObjectContext`, `SharedObjectContext`, `Context`. The generated `prologue`, `run_once`, `run_retrying`, `run_reading`, `verify`, `query_external_id`, `lookup`, `hint`, `lookup_storno`, `storno_step`, `storno_number_of` therefore exist as three monomorphic copies each, and rustdoc/IDE navigation into them is poor. `#[allow(dead_code)]` on the macro body (:348) confirms not every copy uses every helper.
- **Simpler alternative:** Split into `service/fault.rs` (Fault, `terminal`, `read_exhausted`), `service/run.rs` (Journaled + the macro), `service/lookup.rs` (Lookup, `check_pins`), and move `StornoIntent`/`storno_response` into `storno.rs`. For the macro: try a generic `async fn run_retrying<C: ContextSideEffects + …>(ctx: &C, …)` once more with an explicit `+ Send` bound on the returned future (the SDK's `RunFuture` is already bounded); if #100013 still bites, keep the macro but shrink it to the three `run_*` primitives and write the eight domain helpers once against a tiny `trait Runner { async fn run_reading… }` implemented by the three modules.
- **Worth the churn?** **Yes** for the split (mechanical, an afternoon). **Later** for the macro: verify the compiler claim first; if it holds, the current form is a legitimate workaround and the cost is compile time and navigation, not correctness.

### 6. Account-shaped value types live in `config.rs`, contradicting the module's own glossary

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/config.rs:195-481` (`AccountMode`, `Secret`, `Defaults`, `SellerConfig`, `SellerEmailConfig`); imported by `account.rs:19` and `account/static_resolver.rs:63`.
- **Evidence:** `config.rs` module doc (:1-42) says `WorkerConfig` is "what is constant for a deployment, is not account-shaped", then defines five account-shaped types in the same file "so that any resolver's configuration can reuse them". The glossary (CONTEXT.md, *Account*) explicitly says to avoid "account config" for the Account's parts. `config.rs` is 1,229 lines (460 test); moving these five types (~290 lines) leaves a ~480-line deployment-only module whose name matches its contents.
- **Simpler alternative:** `account/types.rs` (or inline in `account.rs`) for `AccountMode`, `Defaults`, `SellerConfig`, `SellerEmailConfig`, `Secret`; re-export from `config` for one release if anything external imports them (nothing does, the endpoint imports only `IssueConfig, Namespace, ReadConfig, ResolveConfig` at `endpoint/src/config.rs:20`).
- **Worth the churn?** **Yes**: pure move, unreleased crate.

### 7. `SellerConfig`/`SellerEmailConfig` duplicate the agent's `Seller`/`SellerEmail` for no journal reason

- **Severity:** low
- **Confidence:** medium: the rationale is inferred; no comment states it.
- **Location:** `crates/restate-szamlazz/src/config.rs:428-481` vs `crates/szamlazz-agent/src/ops/invoice.rs:335-356`.
- **Evidence:** Same three optional fields plus an email block; the only shape difference is `email: SellerEmailConfig` (non-`Option`, `#[serde(default)]`) vs `email: Option<SellerEmail>`. `to_seller()` / `to_seller_email()` are 30 lines of field copying. The projection cannot be justified by "journaled types must be crate-owned": `Account` is journaled and *does* carry these, but the same crate journals the agent's `InvoiceDocument`, `CreatedInvoice` and `InvoiceCreationResult` as-is (`gateway.rs:38-42`). Either the agent types are journal-safe (they are `#[non_exhaustive]` + serde, additive-only by convention) or they are not; the crate currently says both.
- **Simpler alternative:** Use `szamlazz_agent::ops::invoice::Seller` directly on `Account.seller`, pin it in `tests/journal/resolution/` like the other agent types. If the non-`Option` email block is wanted for TOML ergonomics, keep only that one custom `Deserialize`.
- **Worth the churn?** **Later**: small win, and it interacts with finding 2 (with `non_exhaustive` gone, `Seller { bank: …, ..Default::default() }` is trivial from TOML).

### 8. `contract::Selector` and `contract::PaymentMethod`/`TaxpayerStatus` re-declare agent enums variant-for-variant

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/contract/request.rs:149` (`Selector`, 3 variants ↔ `ops::query_pdf::InvoiceSelector`), `contract/document.rs:254-277` (`TaxpayerStatus`, 5 variants + `From`), `contract/document.rs:359-410` (`PaymentMethod`, 8 variants + two `From` impls), `gateway.rs:1699-1707` (`invoice_selector` converter).
- **Evidence:** ~120 lines of mirror enums and conversions. The stated reasons are real: the contract needs `schemars::JsonSchema` (orphan rule blocks deriving it on agent types), `deny_unknown_fields`, and English snake_case JSON tokens where the agent serialises Hungarian wire tokens (`"átutalás"`, `szamlazz-cli/examples/invoice.json:8`). `BuyerInput`/`PostalAddressInput` are justified separately by PII-scoping and by finding 2.
- **Simpler alternative:** An optional `schemars` feature on `szamlazz-agent` deriving `JsonSchema` on the closed enums (`PaymentMethod`, `TaxpayerStatus`, `Language`, `Currency`) would let the contract reuse them, *if* the JSON tokens were acceptable. They are not (the contract deliberately chose English tokens), so the mirror is the honest cost of an anti-corruption layer. Keep it; but the `From<szamlazz_agent::PaymentMethod>` catch-all `other => Self::Other(other.as_wire())` (:407) is the one place a new agent variant silently degrades, add a test that enumerates agent variants.
- **Worth the churn?** **No**: this projection is pulling its weight (PII, schema, tokens). Just guard the catch-all.

### 9. Four `router*` constructors in `szamlazz-adatkapcsolat::axum` where one builder would do

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/szamlazz-adatkapcsolat/src/axum.rs:89-135`.
- **Evidence:** `router`, `router_with_body_limit`, `router_with_resolver`, `router_with_resolver_and_body_limit`, a 2×2 matrix of (fixed key | resolver) × (no limit | limit), each with the same 4-line `where` clause; a third axis (e.g. a custom 401 response, or `addkeytourl` on/off) would double it again. `FixedKey` already implements `KeyResolver` internally, so the fixed-key pair is a convenience over the resolver pair.
- **Simpler alternative:** `Receiver::new(resolver).body_limit(n).into_router()` with `Receiver::with_key(key, handler)` as the convenience; `nest_at` stays. Public surface drops from 5 fns to 1 type + 3 methods.
- **Worth the churn?** **Later**: released crate, low pain today; do it alongside the next breaking release of the crate.

### 10. `Fanout` and `Archiver` sit in the protocol crate with no workspace consumer

- **Severity:** low
- **Confidence:** high on facts; medium on the judgement.
- **Location:** `crates/szamlazz-adatkapcsolat/src/fanout.rs` (312 lines), `src/archive.rs` (501 lines, feature `opendal`), `document.rs:64-88, 985, 1209, 1435` (`raw_xml: Option<Arc<str>>` on every document).
- **Evidence:** `rg Fanout|Archiver|opendal` outside the crate and its tests finds nothing: the CLI's `listen` command does not use either. `Fanout` exists because `Handler`'s `impl Future` methods are not dyn-compatible (module doc :4-6), a consequence of the trait design, solved with a 55-line `ErasedHandler` mirror. The `Archiver` is the only reader of `raw_xml()`, yet every parsed document carries an `Arc<str>` copy of the whole request body to support it.
- **Simpler alternative:** They are legitimate *library* affordances (an application on Cloudflare Workers wants "archive to R2 + run my logic" in three lines), and `opendal` is correctly gated. The one architectural leak is `raw_xml` in the document model: an alternative is for the axum layer / `Document::parse` to return `(Document, Bytes)` and for `Archiver` to take the pair, the model stays pure and non-archiving users pay nothing. `Fanout` would then need the pair too, so this is a small `Handler` signature change.
- **Worth the churn?** **No** for moving them out of the crate (they are protocol-adjacent and feature-gated). **Later** for `raw_xml`: it is a released API; fold it into the next breaking release if the `Handler` signature is touched anyway.

### 11. `WireRequest.url` and `session_cookie` leak transport concerns into the sans-IO envelope

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/szamlazz-agent/src/wire.rs:20-58, 306-318`, `client.rs:180-181`.
- **Evidence:** `AgentRequest::to_wire` hard-codes `url: ENDPOINT` (:313); the bundled `Client::send` then overwrites it with its configured endpoint (`client.rs:181`), so `to_wire` produces a field the only in-tree consumer discards. `session_cookie` is a pub field with a builder method that the bundled client "does not read" (documented :33-36), a sans-IO-only affordance parked on the shared struct with no in-tree user of `with_session_cookie` or `RawResponse::session_cookie` outside `wire.rs`.
- **Simpler alternative:** `to_wire(credentials)` → `WireRequest { content_type, body }`; URL is the transport's business (`Client` has it; a sans-IO user has `wire::ENDPOINT`). Keep `RawResponse::session_cookie()` (reading is harmless) and drop `WireRequest::session_cookie`/`with_session_cookie`, letting sans-IO users add a `Cookie` header themselves.
- **Worth the churn?** **Later**: released API, minor. Note it for the next breaking release; the sans-IO split itself is clean (`AgentRequest` is exactly the right seam: `ACTION`, `write_xml`, `validate`, `parse`, `multipart_files`).

### 12. Response-envelope parsing scaffolding is repeated per op in the agent

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs:1244-1335`, `ops/receipt.rs:680-746`, `ops/proforma.rs:96-104`, `ops/taxpayer.rs:191-210`.
- **Evidence:** The `sikeres` / `hibakod` / `hibauzenet` triple is declared 4 times; `fn from_body` (`xml::response_text` + `quick_xml::de::from_str`) 5 times; `into_success` 3 times; `api_error` 3 times with the same `Unknown("0")` fallback. `InvoiceResponse` and `MinimalInvoiceResponse` (:1246, :1268) are two shapes of the same envelope. Roughly 80–100 lines of duplication. The *request* side, by contrast, is well factored: `xml::document`/`Element` (`xml.rs:20-160`) is a small deep module and every `write_xml` reads as the wire spec.
- **Simpler alternative:** A generic `Envelope<T> { sikeres, hibakod, hibauzenet, #[serde(flatten)] payload: T }` with one `from_body(root, ns)` and one `into_success()`; each op supplies `T`. The invoice op's notification-56 fallback path (:1132-1179) is the one special case and can stay local.
- **Worth the churn?** **Later**: internal (`pub(crate)`), no API impact; do it when the next op is added so the fifth copy is never written.

### 13. `ops/invoice.rs` is a 2,000-line file carrying five carrier/waybill types that belong to a sub-module

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs:149-248` (`TransOFlex`, `PickPackPoint`, `Sprinter`, `Mpl`, `Waybill`), :928-976 (their XML).
- **Evidence:** 1,337 non-test lines; 20 public types; `write_xml` is 220 lines with `#[allow(clippy::too_many_lines)]` (:793). The waybill block is ~150 lines that no other op shares and that most users never touch. Not a god-object (one operation, one shape) but an over-wide file for navigation.
- **Simpler alternative:** `ops/invoice/waybill.rs` (types + a `fn write(&self, w: &mut Element)`), `ops/invoice/attachments.rs` (`EmailAttachment`, `InvoiceAttachments`, `AttachmentError`). The order-preserving `write_xml` stays one function: the docs are right that splitting the serializer would obscure the XSD order.
- **Worth the churn?** **Yes, cheap**, module moves with `pub use` re-exports preserve paths.

### 14. `tests/service.rs` (4,367 lines) is one `#[tokio::test]` running 36 scenarios; the harness and scenarios should be a `tests/e2e/` tree

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/tests/service.rs:1457-1505` (the single test), :87-1455 (harness, ~1,370 lines), :1507-4200 (scenarios).
- **Evidence:** One docker-gated test, 36 sequential scenario fns, plus three unit tests of the harness itself (:4222-4367). The design is deliberate (one Restate container, phase 1 then a flag day then phase 2: the flag day cannot be reordered), so splitting into independent tests is wrong; but the *file* can be split without changing the run: `tests/e2e/main.rs` (the one test), `tests/e2e/harness.rs`, `tests/e2e/mock.rs` (`holds`, `holds_after_misses`, `create_lands_but_reply_lost`), `tests/e2e/order.rs`, `tests/e2e/agent.rs`, `tests/e2e/multi_account.rs`. `tests/gateway.rs` (2,588 lines, 63 tests, already sectioned by `// -----` banners) would benefit the same way but is less urgent.
- **Simpler alternative:** As above; the module boundaries are already drawn by the banner comments.
- **Worth the churn?** **Yes**: mechanical, improves AI/IDE navigability considerably; no behaviour change.

### 15. The endpoint's hand-maintained schema tree duplicates the library types it validates

- **Severity:** low
- **Confidence:** medium: the design rationale (`schema.rs:1-10`) is sound; the alternative trades one property for another.
- **Location:** `crates/restate-szamlazz-endpoint/src/config/schema.rs` (362 lines, 40 test), `config.rs` (929 lines, **753 test**: the loader itself is ~175 lines), `sources.rs` (114).
- **Evidence:** `TOP`, `CAPPED_POLICY`, `RESOLVE_POLICY`, `ACCOUNT`, `DEFAULTS`, `SELLER`, `SELLER_EMAIL` (:48-114) list every key of `WorkerConfig`, `StaticAccount`, `Defaults`, `SellerConfig`, `SellerEmailConfig` by hand, kept in sync by a test that diffs against `Serialize` output (:354). Reason: the account-shaped types are journaled and must stay permissive, so `#[serde(deny_unknown_fields)]` cannot live on them. `MOVED` (:120-127) refuses a pre-release layout (`account.slug`) for a crate that has never been released, dead weight by its own admission.
- **Simpler alternative:** Extend the existing `Layout` pattern (`config.rs:89-103`) one level down: a strict mirror `StrictAccount { id, agent_key, endpoint, mode, supplier_id, defaults: StrictDefaults, seller: StrictSeller }` with `deny_unknown_fields`, converted into `StaticAccount`. ~80 lines of plain structs replace the 320-line walker; serde + figment already report the key path and source. What is lost: "every unknown key at once" (serde stops at the first) and the moved-key hints. Alternatively delete only `MOVED` (:120-127 and its test :722-760) now.
- **Worth the churn?** **Later** for the walker: it works, is tested against drift, and the loss of "all errors at once" is a real UX regression. **Yes** for deleting `MOVED`: the crate is unreleased and the code says so.

### 16. `Body<T>` is a reasonable workaround with two costs worth naming

- **Severity:** info
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/body.rs` (88 lines); used in 9 handler signatures in `service/handlers.rs`.
- **Evidence:** Implements the SDK's `Deserialize` with `Error = Infallible` (:51-60) and mirrors `Json<T>`'s `PayloadMetadata` (:73-88). Costs: (1) it couples to three `restate_sdk::serde` traits that are not the SDK's most stable surface; (2) generated `OrderClient`/`AgentClient` callers must wrap requests in `Body::new` (documented :26-27); (3) `Serialize` of an `Err` body fails at send time rather than construction. Benefit: a structured `{code, message}` 400 for every malformed body, refused before the prologue so nothing is journaled, which the e2e suite verifies.
- **Simpler alternative:** None inside the current SDK; the right fix is upstream (an SDK hook for input-decode errors). Keep, and add a one-line `#[deprecated]`-style note in the doc that this is a workaround to be removed when the SDK offers the hook.
- **Worth the churn?** **No.**

### 17. `Journaled` + fixture generator/replayer: proportionate, but it is a custom snapshot framework

- **Severity:** info
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/journal.rs` (1,049 lines, `#[cfg(test)]`), `support.rs:22-44`, `tests/journal/<type>/*.json` (11 directories).
- **Evidence:** The risk is genuine and specific: the crate journals the *agent crate's* `InvoiceDocument`/`CreatedInvoice`/`InvoiceCreationResult` byte-for-byte (`gateway.rs:38-42`), so a rename in a dependency kills in-flight invocations after a deploy. The mechanism, exhaustive per-variant pins (compile error on a new variant), byte-exact generator, replay of every archived shape through current types with a superset check, is exactly the property that matters. Half of it (byte-exact pin + `UPDATE_…=1` regenerate) is what `insta` does out of the box; the other half (archive-and-replay-forever) is not, and is the valuable half.
- **Simpler alternative:** `insta` for the pin/regenerate half would remove ~150 lines of file-system plumbing (:71-126, the verify/update/archive fns) but would not do the replay; keeping one custom mechanism is defensible over mixing two.
- **Worth the churn?** **No.** Justified by the problem (Restate journal replay across deployments); document the ADR reference at the top of `tests/journal/` in a `README` so a contributor who sees eleven directories of JSON knows why.

### 18. Account / Resolver / CredentialStore abstraction is proportionate, with one guard missing

- **Severity:** info
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/account.rs` (692 lines, 268 test), `account/static_resolver.rs` (904 lines, 430 test).
- **Evidence:** Two object-safe traits with one method each (`resolve`, `fetch`), a 30-line `Accounts` bundle, and one built-in implementation. That is not over-engineered: the traits are the seam that lets the e2e suite run scripted and mutable resolvers (`tests/service.rs:682-720, 834-875`) and the design's stated extension is a database-backed resolver (ADR 0006). The *code* halves are ~420 and ~470 lines; the rest is tests of the safety contract (fan-in, uniqueness, scope charset, redaction). The one gap: the contract "one account under exactly one scope, no fan-in" is enforced only by the static resolver at load time (`static_resolver.rs:363-409`); the `AccountResolver` doc (:273-290) tells other implementors to guarantee it themselves, and `check_account` cannot detect it. That is honest, but it means the safety property is a comment for every non-static resolver.
- **Simpler alternative:** None needed structurally. Consider a `resolver-contract` test helper (a fn a downstream resolver's tests can call with a list of scopes) so the contract is executable, not only documented.
- **Worth the churn?** **No** for the abstraction; **later** for the helper.

### 19. `TerminalCode` does not cover two error shapes the `Szamlazz.Agent` handlers actually raise

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/support.rs:174-179` (`terminal(status, code, message)`), used at `service/agent.rs:253-257` (`not_found`, 404) and for the 422 pass-through; `contract.rs:295` (`TerminalCode`, six variants).
- **Evidence:** Two ways to build a `TerminalError`: the typed `Fault` (six codes, tested status mapping) and a raw `terminal(404, "not_found", …)` string path. The design doc (§7, glossary *Outcome*) lists `not_found` and the 422 pass-through as first-class fault answers, but the type system does not.
- **Simpler alternative:** Add `TerminalCode::NotFound` (404) and `TerminalCode::ApiRejected` (422) and delete `terminal()`; the fault→status table (:146-157) becomes the single source of truth and the discovery/OpenAPI docs can enumerate every code.
- **Worth the churn?** **Yes, cheap.**

### 20. Glossary drift: "attempt" in caller-facing text; "tenant" in adatkapcsolat docs

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/support.rs:110, 127` ("this attempt issued nothing", in the fault body a caller reads), `contract/response.rs:31`, `gateway.rs:1545`; `crates/szamlazz-adatkapcsolat/src/axum.rs:33-43` ("tenant-specific business logic", "tenant handler/context"), `tests/protocol.rs:610-674`.
- **Evidence:** The glossary reserves "execution" and lists "attempt" under *Avoid* for the handler unit; 22 hits of `attempt(s)` in `restate-szamlazz/src`, most legitimate (`max_attempts`, `invocation_retry_policy`), four in prose that should say "execution". "tenant" is on the *Avoid* list for *Account*; the adatkapcsolat `KeyResolver` predates the glossary and is a different concept (key → handler), but the word now collides with the worker's vocabulary in the same workspace. Everything else checked is clean: no "steps" module, no `Contradiction` variant, no "slug" outside the moved-key refusal, no "ledger" for the order, no "idempotency key" for the external id (the two hits are correct uses about szamlazz.hu's API), "session" appears only for the `JSESSIONID` cookie as intended.
- **Simpler alternative:** s/attempt/execution/ in the four prose sites; "per-key handler" or "the handler selected by the key" in `axum.rs`.
- **Worth the churn?** **Yes**: five-minute edit; the fault-body wording is user-visible.

### 21. Handler attribute blocks repeated seven times

- **Severity:** info
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/handlers.rs:44-56, 72-84, 108-120, 136-148, 164-176, 191-203, 218-230`.
- **Evidence:** The identical 12-line `#[handler(invocation_retry_policy(initial_interval = "2m", …), inactivity_timeout = "4m", abort_timeout = "3m", journal_retention = "3d", idempotency_retention = "30d")]` block appears seven times; a change to the create-handler policy is a seven-site edit. The SDK's attribute macro offers no way to name a policy, so this cannot be factored without a local `macro_rules!` that emits the attribute.
- **Simpler alternative:** A `service/tests.rs` discovery test already asserts the attributes (`tests.rs:1-6`), which catches drift; that is the pragmatic mitigation. A local macro is possible but would obscure what the SDK macro sees.
- **Worth the churn?** **No**: accept, rely on the discovery test.

### 22. Minor: validated string newtypes each hand-roll the same six impls

- **Severity:** info
- **Confidence:** high
- **Location:** `restate-szamlazz/src/config.rs:223-297` (`Namespace`), `contract.rs:49-160` (`CorrectionId`, incl. a hand-written `JsonSchema`), `account.rs:162-250` (`Endpoint`), `identity.rs:27-145` (`OrderKey`); the unvalidated ones already use `opaque_string!` (`account.rs:103-154`).
- **Evidence:** ~75–110 lines each of `FromStr`/`TryFrom<String>`/`TryFrom<&str>`/`Display`/`AsRef`/`Serialize`/`Deserialize` around one `validate()`.
- **Simpler alternative:** Extend `opaque_string!` with a `validated_string!($name, $error, $validate)` arm.
- **Worth the churn?** **Later**: cosmetic.

---

## What is done well

- **`szamlazz_agent::wire::AgentRequest` is a genuinely deep interface** (`wire.rs:265-319`): five members (`ACTION`, `Response`, `write_xml`, `validate`, `parse`, plus optional `multipart_files`) and one provided `to_wire` cover all nine operations, the multipart envelope, credential injection, XML 1.0 validation, header/body error precedence and `szlahu_down`. `Client::send` (`client.rs:179-206`) is 25 lines because the trait carries the weight. The sans-IO split is real: the core has no `reqwest` type in any signature.
- **`szamlazz_agent::xml::Element`** (`xml.rs:88-160`): 70 lines that make every `write_xml` read as the XSD in element order. The doc's argument that "the writer code *is* the wire specification" is correct and the golden-file tests (`tests/golden/*.xml`) make it enforceable.
- **`restate_szamlazz::gateway`** is the crate's centre of gravity and earns its 1,700 lines: `lookup` (`gateway.rs:812-883`) and `create` (:910-1009) implement the two-step protocol with the query *inside* the create closure, the immediate re-query on a lost reply, the 71/152 reconciliation and the D/SL storno no-op detection via `CreatedInvoice::reverses` (:1353); every one of these is a verified szamlazz.hu behaviour (`docs/szamlazz-hu-behaviour.md`), and every one is returned as data. The `Unanswered`/`Unconfirmed` pair is the right shape: two error types that say exactly what a run retry policy may re-execute.
- **`restate_szamlazz::identity`** (361 lines): `OrderKey`, `ExternalId::for_kind/for_corrective/for_storno/for_unmanaged_storno/for_probe`, `normalize_buyer_name`. Every identity rule of ADR 0002 lives in one file with no dependency on the SDK.
- **The prologue as pure decisions + stamped durable steps** (`service/prologue.rs`): `resolution`, `account_of`, `resolve_exhausted`, `fetch_fault`, `open` are unit-testable functions; the journaled `Resolution` enum makes "unscoped/unknown are data, unavailable is retryable" a type, not a comment. `Credentials` having no serde impl so the compiler refuses to journal it is a small, excellent invariant.
- **Outcome-as-data in the contract** (`contract/response.rs`): `Outcome`/`ConflictReason`/`Warning` with faults reserved for "outcome unknown" is the correct split for a caller that must decide whether to retry; the flat `CreateResponse` is a deliberate JSON-first choice and the `From<&InvoiceDocument> for QueryResponse` projection keeps buyer PII out of every response by construction.
- **`szamlazz-ipn`** (405 lines): one type, one parser, one extractor; the module doc says the three things a receiver must know (snapshot not delta, no kind discriminator, unauthenticated) in the first fifteen lines.
- **ADRs and glossary are load-bearing**: ADR 0001's dependency-direction claim, the "no `Order` handler calls its own key" rule, the `steps → gateway` rename, and the `Avoid` lists are all reflected in the code; CONTEXT.md is an accurate map, not aspirational.
- **Dependency hygiene**: workspace-pinned versions, `default-features = false` on `axum`, `tokio`, `jiff`, `reqwest`, `opendal`; the three protocol crates' default feature sets pull only `base64`/`jiff`/`quick-xml`/`rust_decimal`/`serde`/`thiserror`/`form_urlencoded`/`percent-encoding`, all wasm32-clean; `send_wrapper` is correctly target-gated (`adatkapcsolat/Cargo.toml:39-40`); `Pdf::save_to` is `cfg(not(wasm32))`. `missing_docs = "warn"`, `unsafe_code = "forbid"`, clippy pedantic workspace-wide. The `restate-szamlazz` ↔ `restate-szamlazz-endpoint` split (library with `restate-sdk` unconditional; binary with clap/figment/tokio-signal) is the right one: the library has zero process concerns.

## Questions I could not resolve

1. **Does CI build the three protocol crates for `wasm32-unknown-unknown`?** `devenv.nix:22` installs the target; the CI runs a Dagger module (`github.com/sagikazarmark/daggerverse-beta/rust`, `dagger.toml`) whose steps are not visible in this repo. The wasm claims in every `lib.rs` are plausible from the dependency tree but unverified by anything I can read here.
2. **Is MSRV 1.92 checked?** `dagger.toml` pins `1.98-slim-trixie`; `Duration::from_mins/from_hours` (`config.rs:515`, `client.rs:138`) and let-chains are within 1.92, but nothing in-repo builds with 1.92.
3. **Is the `journal_helpers!` triplication still necessary?** The rust-lang/rust#100013 claim (`support.rs:343-345`) is credible for `async fn` generic over the SDK's context traits inside the macro-generated dispatcher, but I could not compile a generic variant to confirm.
4. **Can the Agent's `<szamla>` query response carry an `xs:date` timezone suffix?** The adatkapcsolat parser tolerates it (`document.rs:1541-1570`), the agent parser does not (`xml.rs:169-181`). If the same server code renders both, the agent has a latent parse failure.
5. **Are there downstream users of `Fanout`/`Archiver`/`WireRequest::session_cookie`?** Nothing in the workspace uses them; they are released API, so the "worth the churn" on findings 10–11 depends on external consumers I cannot see.
6. **Is the single-test e2e design chosen for container cost or for the flag-day ordering?** Both are implied by `tests/service.rs:1466-1505`; the answer decides whether finding 14's split should also break the test into two (phase 1 / phase 2) or stay one.
