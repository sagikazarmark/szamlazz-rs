# szamlazz-rs: architecture, naming, conventions, docs and tracker review, 2026-09-09

Workspace at `8355ff5` (#172 merged: ADR 0009, immutable deployments). Five reviewers in parallel, one lead: the worker (`restate-szamlazz`); the Számla Agent crate and the CLI; the two receivers and the e2e harness crate; documentation drift; the issue tracker. The lead spot-checked every load-bearing claim against HEAD. Tracking issue: #173.

The questions asked: are there seams not worth their value; is any naming off from the domain glossary (CONTEXT.md, its `_Avoid_` lists in both directions); where does the code deviate from Rust conventions (a hand-rolled parse where `FromStr` belongs, a `From` that can fail, a `Default` on a type with no sensible absence, …); and is anything in the documentation or the tracker out of sync with the code.

Vocabulary: *module*, *interface*, *implementation*, *depth* (deep / shallow), *seam*, *adapter*, *leverage*, *locality*; the deletion test; "the interface is the test surface"; one adapter is a hypothetical seam, two a real one.

## 1. Verdict

`cargo clippy --workspace --all-targets --all-features` (pedantic, `unwrap_used`) and `cargo doc --no-deps` are clean on every crate; every finding below is judgement, not lint.

**The worker is deep where it matters.** `Gateway` hides every szamlazz.hu quirk behind eleven async fns with two `Err`s; every `ctx.run` is a one-line gateway call; the decisions are pure fns since #137/#138; the identity types carry a compile-time length proof. Two structural gaps remain: a domain concept without a module (the *Storno invoice* protocol, ADR 0007, spread over `support.rs`, `storno.rs` and `agent.rs`, with the file named "support" owning most of it), and a seam whose reason left with ADR 0009 (the `Static*` mirror types over the journaled value types, kept "so that an earlier deployment's entry replays").

**The protocol crates carry the naming drift.** "payment" where the glossary says credit entry, "cancelled" where it says reversed, one wire element under two English names across the invoice and receipt surfaces, a flag name (`e_invoice`) on a code. The adatkapcsolat crate enumerates "which pushed kind" three times and writes the KEY_ERR Ack in the axum adapter, so the framework-free crate cannot follow its own protocol order off axum; its README quick start does not compile as an external crate (`Document` is `#[non_exhaustive]` while `Handler` makes a new stream breaking by design).

**The e2e harness crate ships primitives and documents a rule it does not implement**: the step-name table check (~70 lines) lives in the consumer, verified only against a live server.

**Documentation: 17 stale claims, 10 ambiguous**, fixed with this review. **Tracker: 33 open, none closable, 19 with stale paragraphs**, clustered around four events (the endpoint crate removed, the docker launcher removed, ADR 0009, the account-pin amendment); every *Blocked by* edge but one pointed at a closed issue. Edited with this review.

## 2. Seams (the candidates)

| # | Finding | Strength | Ticket |
|---|---|---|---|
| C1 | The Storno invoice protocol has no module; `support.rs` (993 lines) is a bucket of four purposes; the prologue's steps and decisions are split for a reason `RunCtx` removed | Strong | #174 |
| C2 | `StaticDefaults` / `StaticSeller` / `StaticSellerEmail` mirror `Defaults` / `SellerConfig` / `SellerEmailConfig` field for field; the rationale ("permissive for replay") is gone under ADR 0009 | Strong | #175 |
| C3 | Adatkapcsolat: `RootKind` (private), `InvoiceDirection`, `archive::DocumentKind` are one enumeration; `identify` private; KEY_ERR Ack in the axum adapter; `Document` `#[non_exhaustive]` against `Handler`'s design; README fails E0004 downstream; the shape rule's justification does not cover the id of a bank transaction / receipt | Strong; decision | #176 |
| C4 | Agent: the `sikeres` / `hibakod` / `hibauzenet` verdict declared in five response structs; `ErrorCode::Unknown("0")` fabricated at six sites; `storno.rs`, `receipt.rs`, `query_xml.rs` import shared types from sibling ops | Strong | #177 (sibling of #145) |
| C5 | Agent: `LineItem::calculated` / `calculated_for_currency` panic on caller money and have no production caller; `Totals` and `ReceiptTotals` are one wire shape under two public trees | Strong | #178 |
| C6 | Harness: the step-name table check in the consumer; Restate's URL grammar written eleven times; `plain_http()` public with one adapter; the consumer `Harness` forwards eleven `Admin` methods one to one | Strong | #179 |
| C7 | `support::Lookup::classify` re-derives `Gateway::seen`: the same `is_ours` on both sides of the seam; the collision `warn!` twice | Worth exploring | #181 |
| C8 | Agent: `InvoiceCreationResult` is `CreatedInvoice` with seven `Option`s dependent on `invoice_number.is_some()`; the worker re-derives the case as `Unnumbered` | Worth exploring | #180 |
| C9 | Traps: unused `Gateway: Clone` (shares the cookie jar), public `gross_total` with no caller, `Mpl: Default` (ADR 0008), the receipt writer's silent field drop, `ServerSpec::validate` panics where `Document::validate` returns | Strong | #182 |

Noted, not recommended: the create and storno write steps as structural twins in `gateway.rs`; `Execution<'ctx, C: RunCtx>` to collapse the `ctx` pair; a structured `ValidationError`; `Handler::Error: std::error::Error` (both in #130); the CLI's command tree per document rather than per operation (#76); a `watch.rs` in the harness (#189).

Checked and fine: `service::Body<T>` (the SDK hard-codes the plain-text 400); `RunCtx` + `run_ctx!` (three SDK contexts, no shared trait); the boxed-future resolver and store traits (`Arc<dyn>`; `async fn` in trait is not dyn-compatible on 1.92); `journaled!` / `variants!` / `Journaled` (ADR 0009 item 3); `RetryPolicyConfig<T: Table>`; `gateway.rs`'s size (interface, one cohesive impl, tests); `Fault` constructors split by audience; `AgentRequest` (12 adapters) and `http_client` (three adapters); the header verdict in `RawResponse`; one `xml` writer module; the adatkapcsolat `Ack`, `archive` (policy, not pass-through), the `de` helpers' no-byte-offset rule; ipn as one module plus one adapter with the glossary's two properties modelled by absence; the harness's hard rule, server gate, free ports, process-group kill, unix gating.

## 3. Naming vs the domain

Avoided words that appear nowhere in `src/` or `tests/`: tenant (worker), slug, health check, preamble, middleware, invalid payload, unknown outcome, error class, `InvoiceDocumentExt`, generation, orphan, re-create, push API, feed, payment callback.

| # | Where | Code | Glossary | Ticket |
|---|---|---|---|---|
| N1 | ~12 rustdoc sites and two test names in the worker; `receipt.rs:526-534` in the agent | "additive-only", "journal fixtures", "pins", "account mode" | *Journaled type* and *Account* `_Avoid_` (ADR 0009, ADR 0006 amendment) | #183 |
| N2 | CONTEXT.md:190, design §3, `gateway.rs:18` | "ownership-validation pins" | the glossary contradicted itself | fixed (prose); #183 (rustdoc) |
| N3 | agent `RecordedPayment`, `payments`; CLI `payment register` | payment | *Credit entry*: reserve "payment" for the buyer's act | #185 |
| N4 | agent `cancelled_receipt_number`, "cancels an issued receipt" | cancel | *Storno*: avoid cancellation, void | #185 |
| N5 | agent `jogcim` as `method: PaymentMethod` out, `title: String` back | two names, two types | *Found document* uses `title` | #185 |
| N6 | agent `InvoiceInfo.e_invoice: InvoiceAppearance` | a flag name on a code | *Invoice appearance* | #185 |
| N7 | agent `tipus`, `szamlaLetoltesPld`, `afakulcs`, `szamlaSablon` under two English names each | | | #185, #139 |
| N8 | adatkapcsolat `ControlCode::KeyError` | "error" | *Control code*: not errors | #130 |
| N9 | `tests/e2e/pins.rs`, "the run-wide pins", `main.dang` "run-name pin", the harness crate's in-place-redeploy rationale | pin | *Step-name table* | fixed (prose); #179, #183 |
| N10 | `contract::Outcome` vs `contract::StornoOutcome` vs `gateway::StornoOutcome` | a wire token and a journaled step outcome share a name | | #184 |
| N11 | `DeleteOutcome::Transport`, `SetPaymentsOutcome::Transport` swallow `szlahu_down` | one cause named for a lost one-shot reply | no glossary term for the shape | #184 |
| N12 | the harness crate says "the harness" 36 times | | the `_Avoid_` could not be honoured by the crate's own docs | fixed (glossary reworded) |
| N13 | harness rustdoc and tests use szamlazz step names | | the crate "knows nothing of any endpoint" | #183 |
| N14 | glossary gaps: `IssuedKind` / `DocumentKind`, `Lookup`, `StornoVerdict`, `FinancialItem` (`qutet`), `Waybill`, `ExchangeRate`, "tenant" in the receiver, `Launcher::Reuse` vs `Reuse::*` | | | #184, #130 |

## 4. Rust conventions

| # | Finding | Ticket |
|---|---|---|
| R1 | Five bounded newtypes (`Namespace`, `OrderKey`, `CorrectionId`, `InvoiceNumber`, `Endpoint`) with five different conversion sets (C-CONV) | #186 |
| R2 | `DeleteProformaResponse::reason: Option<String>` mixes pseudo-codes and szamlazz codes; `"not_stornoable"` literal twice with two messages (the pattern #128 fixed in the journal) | #187 |
| R3 | `ErrorCode` has `From<&str>` / `From<String>` / `From<u16>` and no `FromStr`; `From<u16>` allocates to run a string match | #188 |
| R4 | Third-party error types on the public API: the worker's `InvalidEndpoint::Uri(#[from] http::uri::InvalidUri)` beside the protocol crates' | #131 |
| R5 | `Fault::credentials_rejected` is a constructor with a side effect (the paging `warn!`, 18 call sites) | #153 |
| R6 | `ValidatedWorkerConfig: Deref<Target = WorkerConfig>` (C-DEREF); `AsRef` and `into_inner` exist | noted only |
| R7 | Open-set treatment inconsistent: `ReceiptTemplate` `#[non_exhaustive]` with no `Other`; `InvoiceAppearance::Electronic(i32)` leaks its invariant | #129, #182 |
| R8 | `VatRate`, `Currency`, `PaymentMethod` lack `Hash`; bounded collections without `len` / `Deref<[T]>` / `IntoIterator` | #188 |
| R9 | Wire booleans in three styles, no stated rule | #188 |
| R10 | `ClientBuilder::endpoint` accepts any string; failure deferred to every `send` | #188 |
| R11 | `IpnParseError::Invalid { message }` drops `source()`; the harness has no evolvability rule written down | #149, #189 |
| R12 | Small duplications: two `type BoxFuture`, `Order::execute` / `execute_shared`, a body rendered in `gateway.rs` tests and `tests/common`, `support.rs` tested from `service/tests.rs` while its siblings test inline, responses built by `pub` fields and by `with_*` | #174 |

Checked and fine: `thiserror` with `#[source]` chains; display texts never echo a store's or resolver's message; getters without `get_`; `#[must_use]`; `# Errors` / `# Panics` complete; serde attribute style uniform per crate; requests closed and exhaustive, responses open and `#[non_exhaustive]` (worker); redacting `Debug` on the secrets, no serde on `Credentials`; RPITIT on the adatkapcsolat traits; `doc_cfg` via `docsrs`; the harness's panic style documented, zero `unwrap`.

## 5. Documentation drift (fixed with this review)

Doctests: agent 6 (three of them README blocks), ipn 1, adatkapcsolat 4, restate-szamlazz 2, harness 1, all passing. `crates/restate-szamlazz/README.md`'s two `rust` fences are compiled by nothing (read-checked, correct); the adatkapcsolat README quick start fails downstream (#176, #148).

| Document | Was | Now |
|---|---|---|
| CONTEXT.md *Gateway* | "ownership-validation pins" | dropped |
| CONTEXT.md *Found document*, *Restate e2e harness* | `Doc`'s location; "logs nothing"; the "harness" `_Avoid_` | stated; "a few `eprintln!` lines"; reworded |
| restate-szamlazz README | `from_parts(Accounts, WorkerConfig)`; "table below"; "additive-only" ×2 | `ValidatedWorkerConfig`; "above"; dropped |
| design doc | "ADRs 0001–0006"; "§9" for go-live; the `query_taxpayer` account-check sentence; "eight codes"; `options.proforma` refused on all but `create_invoice`; the VO key among post-prologue faults; "validates against the account"; issue #15; "pins" §3; "permissive for replay" ×2; no harness crate in §2 | fixed each |
| ADR 0001 | eight of eleven gateway fns; no supersession notes (#30/#37; #127/ADR 0009) | eleven; two notes |
| ADR 0004, 0005, 0008, 0009 | #114 missing from the status line; "validation pins" in the status; `tests/sans_io.rs`; "the redeploy scenario" | fixed each |
| szamlazz-hu-behaviour.md | `create_proforma` looks up two ids | three (#62) |
| compose.yaml, `.dagger/modules/ci/main.dang` | "its own container"; "run-name pin" | the spawned binary; "step-name table check" |
| harness README, `run_names.rs`, `lib.rs` | the in-place-redeploy rationale only | both premises |

## 6. Tracker (edited with this review)

33 open before the review, none closable. 19 edited: #45, #59, #68, #74, #75, #76, #106, #112, #121, #129, #130, #131, #132, #135, #139, #141, #144, #145, #148, #149, #151, #153, #154, #155, #156. Dead *Blocked by* edges removed: #45→#37, #68→#66, #132→#123, #151→#136, #153→#137/#138, #154→#137, #156→#136; #155→#141 stays. A correcting comment on #68 (its 18:01 comment described an unmerged branch). Kept as written: #50 (tabled on the SDK), #109, #111, #126, #146, #147, #150.

New: #173 (tracking) and #174–#189.
