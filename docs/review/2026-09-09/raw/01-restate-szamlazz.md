# Architecture review: `restate-szamlazz` (research only, 2026-09-09)

Workspace at `8355ff5` (#172, ADR 0009 merged). Mechanical checks: `cargo clippy -p restate-szamlazz --all-targets --all-features` (workspace `clippy::all` + `pedantic`, `unwrap_used`) is **clean**; `cargo doc --no-deps --all-features` emits **no warnings**. Everything below is therefore judgement, not lint.

Terminology: *module* = a unit with an interface and an implementation; *deep* = small interface hiding much implementation; *seam* = a place you can substitute across; *adapter* = a concrete implementation of a seam; *leverage* = behaviour unlocked per line of interface; *locality* = one concept in one place.

---

## Axis 1 · Seams not worth their value, and concepts smeared across files

### S-1 · The `Static*` input mirror types survive only on a reason ADR 0009 removed: **Strong**
- `src/account/static_resolver.rs:186-210` (`StaticDefaults`), `:212-270` (`Default` + `From` copying ten fields twice), `:274-296` (`StaticSeller`), `:300-319` (`StaticSellerEmail`); the value types they mirror: `src/account.rs:107-151`, `:167-193`, `:200-225`.
- **What is wrong.** Three closed input types mirror three journaled value types field for field, with two field-by-field copy impls and a round-trip test holding them together. The module doc gives the reason: the value types "stay permissive so that an `account` entry of an earlier deployment replays" (`static_resolver.rs:42-45`, `:659`; `account.rs:159-166` gives the same reason for `SellerConfig` not being the agent's `Seller`). Under ADR 0009 no entry is decoded by a later release, so the journaled types may be closed too. The only surviving difference is `#[serde(deny_unknown_fields)]`.
- **Why.** Deletion test: deleting `StaticDefaults`/`StaticSeller`/`StaticSellerEmail` and putting `deny_unknown_fields` on `Defaults`/`SellerConfig`/`SellerEmailConfig` concentrates nothing and removes ~130 lines plus a test whose only job is to keep two copies equal. The seam had one adapter each side and its stated purpose is gone.
- **Direction.** Close the value types; `StaticAccount` holds `Defaults`/`SellerConfig` directly. Keep `StaticAccount` itself (it carries the inline `agent_key: Secret`, a genuine difference). Caveat worth one sentence in the ADR: an embedder's database-backed resolver that deserialises `Defaults` from its own storage becomes strict too. No ADR conflict; ADR 0009 is the enabler. The design doc still carries the stale reason (`docs/design/restate-szamlazz.md:703`, `:755-757`).

### S-2 · The storno protocol is smeared across `support.rs`, `storno.rs` and `agent.rs`; `support.rs` is a bucket, not a module: **Strong**
- Storno logic in `support.rs`: `StornoIntent` `:321-371`, `StornoVerdict` `:378-391`, `reversed_response` `:393-399`, `after_storno_lookup` `:401-427`, `storno_response` `:429-471`, `storno_number_from_hint/lookup` `:473-518`, `lookup_storno`/`storno_step` `:878-936`, `storno_number_of[_unmanaged]` `:938-993`: about 330 of `support.rs`'s 993 lines. The two verdict fns live elsewhere: `storno_verdict`/`not_stornoable` `src/service/storno.rs:250-281`, `unmanaged_storno_verdict` `src/service/agent.rs:156-174`. `storno.rs` itself also holds `delete` and `get` (`:125-248`).
- **Why.** Locality: to read the *Storno invoice* protocol (CONTEXT.md, ADR 0007) you open three files, and the file named "plumbing" holds most of the domain decisions. `support.rs`'s doc header (`:1-4`) lists four unrelated purposes (fault mapping, journaled runs, external-id validation, "the account check", the last stale). A module whose name is `support` fails the deletion test in the opposite direction: deleting it moves everything, because it owns no concept.
- **Direction.** A `service/storno/` module owning the shared protocol (intent, verdict, after-lookup, response, the two best-effort number reads, the storno step) with the two shells (`Order::storno_invoice`, `Agent::storno`) calling into it; `delete` and `get` to their own files (`service/delete.rs`, `service/status.rs`); `support.rs` left with the fault mapping, `RunCtx`, the run helpers and `Lookup`. No ADR conflict.

### S-3 · The *Prologue* is split between `prologue.rs` (decisions) and `support.rs` (durable steps) although `RunCtx` removed the reason: **Worth exploring**
- `src/service/prologue.rs:1-10` says "The durable steps themselves are `support::run_prologue`, generic over `support::RunCtx`"; `support::execute`/`run_prologue` at `src/service/support.rs:636-719`.
- **Why.** The split dates from when the durable side was three macro stamps. With `RunCtx` (`:578-596`) a generic `run_prologue` can sit beside the decisions it sequences. CONTEXT.md's *Prologue* entry has to name two modules for one concept.
- **Direction.** Move `execute`, `execution_span` (already there), `run_prologue` into `prologue.rs`; `support` keeps `RunCtx` and the `run_*` helpers. No ADR conflict.

### S-4 · Two enums, two layers, one validation: `support::Lookup::classify` re-derives `Gateway::seen`: **Worth exploring**
- `src/service/support.rs:520-565` (`Lookup::{Absent, Ours, Collision}` + `classify(QueryOutcome, …)`) vs `src/gateway.rs:1703-1713` (`Seen::{Absent, Live, Reversed, Collision}`) and `:1223-1246` (`Gateway::seen`). The collision `warn!` is written in both (`support.rs:560`, `gateway.rs:1232`).
- **Why.** The exclusivity, proforma-link, `get` and delete reads call `Gateway::query` (raw `QueryOutcome`) and classify against `(order, kind)` in the service, while the lookup and create steps do the same validation inside the gateway. Same `is_ours` question, two adapters of it, one on each side of the seam. It also makes `QueryOutcome` the journaled type for a read whose meaning is "ours / not ours", so the journal records less than the handler decided.
- **Direction.** A gateway read fn `Gateway::lookup_ours(external_id, order, kind) -> Result<OwnedLookup, Unanswered>` (the `Seen` shape plus the two answered codes) that the four service reads journal; `Lookup::classify` goes. ADR 0009 permits the journaled-type change. No ADR conflict.

### S-5 · `gateway::gross_total` is public API with no production caller: **Strong (small)**
- `src/gateway/build.rs:192-203`, re-exported `src/gateway.rs:87`; only callers `build.rs:323`, `:603` (tests).
- **Direction.** Delete, or `#[cfg(test)]`. Trivial.

### S-6 · `Gateway` derives `Clone` against its own boundary: **Worth exploring (small)**
- `src/gateway.rs:247-251`. Nothing calls it (grep: no `gateway.clone()`); `Execution` uses `Arc<Gateway>`.
- **Why.** The type's documented invariant is one fresh client per open (`:800-817`); a `Clone` shares the `reqwest::Client` and its cookie jar. An unused derive that invites the exact misuse the docs forbid is a trap, not an interface.
- **Direction.** Drop `Clone`. No ADR conflict.

### S-7 · `build_create` is on `Gateway` but reads only `self.account`; extends #154: **Worth exploring**
- `src/gateway/build.rs:71-190`; every build test opens a gateway to test a pure projection (`build.rs:237-246`).
- **Why.** The gateway is "not a second client" (CONTEXT.md); a projection needing no client on the client-holding type couples `contract` validation to `reqwest`. #154 splits validate/assemble but keeps both on `Gateway`.
- **Direction.** `fn build_create(account: &Account, …)` (or `Account::build_create`); `Execution::build` passes `self.gateway.account()`. Fold into #154.

### S-8 · The e2e `Harness` is two-thirds forwarders: **Worth exploring (low)**
- `tests/e2e/harness/mod.rs:361-443` (13 one-line forwards to `self.restate.admin().…`), `:535-542`, `:555-588` (5 forwards to `szamlazz::*`).
- **Why.** Depth is in `switch_to_multi_account`, the `call*` URL builders, `requests_of_order`/`create_bodies_of` (delimiter logic), `absent`, `reset`. The rest adds a name without hiding anything.
- **Direction.** Expose `pub(crate) fn admin(&self) -> &Admin` and `mock()`; keep the deep methods. Consequence of the #167 crate split; not a defect of the split.

### S-9 · The create and storno write steps are structural twins in `gateway.rs`: **Speculative**
- `create_inner`/`settle_or`/`settled_by_query`/`seen` (`gateway.rs:1011-1100`, `:1189-1246`) vs `storno_inner`/`storno_settle_or`/`storno_settled_by_query`/`storno_seen` (`:1439-1534`, `:1546-1587`).
- **Why.** Two written-out adapters of one "query-first write step" shape; by the two-adapters rule a real seam exists. But the pair differs in the duplicate-order-number branch and the echo branch, and the win is ~150 lines. Not the reason to split `gateway.rs`; the file is deep and cohesive (see FINE).

### S-10 · `Execution` and `ctx` travel side by side; every handler does the `let ctx = &ctx;` reborrow: **Speculative**
- `src/service/handlers.rs:64-73` and every handler after it; `Order::execute` vs `execute_shared` (`src/service.rs:106-125`) exist only because the two ctx types are concrete.
- **Direction.** `Execution<'ctx, C: RunCtx<'ctx>>` holding `&C` would collapse the pair and the reborrow. Cost: a type parameter through `create.rs`/`storno.rs`/`agent.rs`. Only if S-2/S-3 are done anyway.

---

## Axis 2 · Naming off from the domain

### N-1 · ADR 0009's retired vocabulary ("additive-only", "fixture", "pin" for the journal) still in rustdoc and test names: **Strong**
- `src/gateway/document.rs:106-107`, `:245` ("Additive-only, like `FoundDocument`"), `:396-399` ("The field order is the journal fixtures' to pin"); `src/contract/agent.rs:282-287`, `:327`, `:916-920` + test `query_taxpayer_response_projects_the_agent_info_and_decodes_additively`; `src/account.rs:159-166` (compatibility-contract rationale), `:817-820` + test `account_is_additive_only_with_the_production_endpoint_by_default`; `src/account/static_resolver.rs:42-45`, `:659`; `src/test_support.rs:169`, `:269` ("the worker's pins and derivations": the *account* pins, gone since ADR 0006's amendment).
- CONTEXT.md *Journaled type* `_Avoid_`: "additive-only / fixture / pin / archive rule (the pre-ADR-0009 contract; gone)". #153's rustdoc clause predates ADR 0009 and does not cover these.

### N-2 · CONTEXT.md itself says "ownership-validation pins": **Strong (doc)**
- `CONTEXT.md:190` (*Gateway* entry): "Every read of account configuration … (ownership-validation pins, document defaults, seller block)". Contradicts its own *Account* entry (`_Avoid_`: "mode / supplier id / account pin (all gone)"). Same phrase at `docs/design/restate-szamlazz.md:39` and `src/gateway.rs:18` (the latter in #153).

### N-3 · `tests/e2e/pins.rs` and "the run-wide pins": **Worth exploring**
- `tests/e2e/main.rs:44`, `:90`, `:247-251`; `tests/e2e/harness/run_names.rs:6`.
- CONTEXT.md *Step-name table* `_Avoid_`: "run-name pin (the pre-ADR-0009 name; 'pin' is retired from the journal vocabulary)". The file also holds the no-state and no-agent-key scans, which are invariants, not pins. `tests/e2e/invariants.rs` (or `run_wide.rs`) fits.

### N-4 · `contract::Outcome` vs `contract::StornoOutcome` vs `gateway::StornoOutcome`: **Worth exploring**
- `src/contract/create.rs:120`, `src/contract/storno.rs:43`, `src/gateway.rs:685`; the collision forces `StornoOutcome as GatewayStornoOutcome` (`src/service/support.rs:21-23`).
- The create token is unqualified, the storno token qualified; and the storno *step* outcome (journaled) shares a name with the storno *wire* token. Either `contract::CreateOutcome`/`StornoOutcome` (wire) with the gateway pair as is, or a `Step` suffix on the gateway side. Wire strings unchanged either way.

### N-5 · `DeleteOutcome::Transport` / `SetPaymentsOutcome::Transport` name one cause for "the one-shot write's answer is not known" and swallow `szlahu_down`: **Worth exploring**
- `src/gateway.rs:737-738`, `:1605` (`Err(error) => Transport(error.to_string())` after the `Api` arm, so `ServiceUnavailable` lands in `Transport`); `:772-773`, `:1640` likewise.
- CONTEXT.md's *Unanswered*/*Unconfirmed* both `_Avoid_` "transport error (one cause of it)" and split `Transport` from `Unavailable` carefully; the two one-shot writes fold them. CONTEXT.md also has no term for this third shape (a journaled "lost reply" of a `run_once` write). Code term missing from the glossary.

### N-6 · `config::Policy` is "policy without qualification" and doubles `table::{Issue, Read, Resolve}`: **Speculative**
- `src/config.rs:245-265` vs `:270-351` (`Table::POLICY`). CONTEXT.md *Retry policy config* `_Avoid_`: "policy without qualification". Two names for one thing; `Table` could carry the display name.

### N-7 · Domain types the code has and CONTEXT.md does not name: **Worth exploring (doc)**
- `IssuedKind` / `DocumentKind` (`src/identity.rs:257-374`): CONTEXT.md speaks of "the four kinds" and "corrective" throughout but never names the distinction that an order carries at most one live document of a `DocumentKind` while `IssuedKind` adds correctives. `support::Lookup` (`Absent/Ours/Collision`, the classified external-id read every non-issuing handler decides on) and `StornoVerdict` (`Proceed/AlreadyReversed/Answered`) are the two other decision shapes with no glossary entry.

### N-8 · Minor vocabulary
- `unknown_mode` (`src/test_support.rs:214`): "mode" is retired.
- `payments` fields holding `RecordedCreditEntry` (`src/gateway/document.rs:101`, `payment_amounts()` `:160`) while CONTEXT.md reserves "payment" for the buyer's act; the wire names (`set_payments`, `PaymentEntry`) are sanctioned by CONTEXT.md, the internal field is not.
- `tests/e2e/harness/szamlazz.rs:276` `create_body(order) -> String` (szamlazz.hu XML) vs `tests/e2e/harness/mod.rs:99` `create_body(unit_price, reissue) -> Value` (the JSON request): one name, two bodies, one harness.

---

## Axis 3 · Rust conventions

### R-1 · Stringly-typed pseudo-codes on the wire responses, the pattern #128 already fixed in the journal: **Worth exploring**
- `DeleteProformaResponse::reason: Option<String>` mixes `absent`, `proforma_paid`, `external_id_collision` and szamlazz codes (`src/contract/storno.rs:160-197`; sites `src/service/storno.rs:294-303`, `:326`). `"not_stornoable"` is a literal at two sites with two different messages (`src/service/support.rs:451-455` for the echo, `src/service/storno.rs:277-281` for the verify). The create side has `ConflictReason` (enum) beside `code` (szamlazz's).
- **Direction.** A `RejectionCode`-style enum (`DeleteReason::{Absent, ProformaPaid, ExternalIdCollision, Szamlazz(String)}`) serialising to the same strings; one `not_stornoable` constructor. JSON unchanged.

### R-2 · Inconsistent constructor/conversion sets across the five bounded newtypes: **Worth exploring**
- `Namespace` (`src/identity.rs:31-100`): `FromStr`, `TryFrom<String>`, no `TryFrom<&str>`, no `From<Namespace> for String`, no inherent constructor. `OrderKey` (`:137-226`): inherent `parse` + `FromStr` + `TryFrom<String>`, no `TryFrom<&str>`. `CorrectionId`/`InvoiceNumber` (`:391-481`, `:548-634`): private `validate` + `FromStr` + `TryFrom<String>` + `TryFrom<&str>` + `From<_> for String`. `Endpoint` (`src/account.rs:296-453`): inherent `parse` + `FromStr` + `TryFrom<String>`, no `TryFrom<&str>`.
- C-CONV: pick one set (the `CorrectionId` set is the fullest) and apply it to all five; the previous review's `bounded_string!` idea would enforce it.

### R-3 · `Fault::credentials_rejected` is a constructor with a side effect: **Speculative**
- `src/service/support.rs:195-208` emits the paging `warn!` while building the value (18 call sites). A test that builds the fault logs; a fault built and discarded pages. C-CTOR expects constructors pure. Moving the warn to the raise boundary (`From<Fault> for TerminalError`, `:213-219`) loses the `namespace` tag since `Fault` carries none, so the fix is not free; #153's single mapping fn is the natural home.

### R-4 · Third-party error type in the public API: **Worth exploring (small)**
- `InvalidEndpoint::Uri(#[from] http::uri::InvalidUri)` (`src/account.rs:459-462`). Same class as #131's semver item, which lists only the protocol crates. A `http` major bump becomes a `restate-szamlazz` break.

### R-5 · `Deref` on a validated newtype: **Speculative**
- `ValidatedWorkerConfig: Deref<Target = WorkerConfig>` (`src/config.rs:184-190`). C-DEREF reserves `Deref` for smart pointers; `AsRef` and `into_inner` are already there. Common practice, but the guideline is explicit.

### R-6 · Small duplications
- `agent.rs:291-298` inlines exactly `support::verify` (`support.rs:831-843`).
- `type BoxFuture` defined twice (`src/service/support.rs:30`, `src/account.rs:477`).
- `Order::execute` / `execute_shared` (`src/service.rs:106-125`) differ only in the ctx type.
- `gateway.rs:1874-1882` renders an `xmlszamlavalasz` body that `tests/common/mod.rs:292-305` also renders (as a `ResponseTemplate`); a shared body-string fn would leave one renderer.
- `support.rs` has no inline `mod tests`; its decisions are tested from `src/service/tests.rs` (`:720-1310`) while `create.rs`, `storno.rs`, `agent.rs`, `prologue.rs` test inline. Locality convention split.
- `Identity`'s `respond_to` builds responses by setting `pub` fields *and* by `with_*` (`src/service/create.rs:106-117`); the response types offer both (`src/contract/create.rs:326-437`). Two styles for one thing.

---

## Ticket status (stale or narrowed by #136–#172)

- **#156**: "the three `Doc` fixture renderers" is done (#134 left one, `tests/common/mod.rs`); "journal fixtures handled per the archive rule" is void (ADR 0009). Remaining: the `tests/gateway/` tree and `eszamla` default `2 → 1` (`tests/common/mod.rs:216-218` still `2`).
- **#153**: "the journaled enums stay byte-identical (the additive-only rule)" is no longer a constraint; the design space is wider (a shared `Answered` sub-enum inside the outcomes is now allowed). Of its rustdoc list, `lib.rs:173-180` and `contract.rs:4-11` are already fixed; `gateway.rs:18` and `support.rs:3-4` are not. N-1 above is the ADR-0009 drift #153 predates.
- **#154**: still valid (`ExternalId::new("-")` at `create.rs:620`, `for_storno(&str)` at `identity.rs:771`); S-7 extends it.
- **#139, #131, #151**: still valid; code unchanged at the cited sites (`create.rs:368`, `storno.rs:269`, `gateway.rs:1700`; `create.rs:823`; `gateway.rs:823`).
- Previous review's **F-10** (`normalize_buyer_name` in `identity.rs:856-865`) was never ticketed and is still there.

---

## Checked and FINE

- **`service::Body<T>`** (`src/service/body.rs`): warranted. The SDK decodes `T` before the handler and hard-codes the plain-text 400 (`restate-sdk-0.12.0/src/endpoint/context.rs:222-226`); no hook. 88 lines, one reason, exact `Json<T>` metadata.
- **`RunCtx` + `run_ctx!`**: necessary. `scope()`/`invocation_id()` are inherent on each SDK context (`restate-sdk-0.12.0/src/context/mod.rs:39-342`), no shared trait; the boxed `Send` future sidesteps rust#100013. Three impls of one trait = a real seam.
- **`AccountResolver`/`CredentialStore` boxed futures**: still needed. `Accounts` holds `Arc<dyn …>` (`account.rs:663-666`) so service types don't vary with the resolver; `async fn` in trait is not dyn-compatible on 1.92.
- **`journaled!`, `variants!`, `Journaled`**: kept by ADR 0009 item 3 as the privacy scan's mechanism; `variants!` is a test-only macro that turns one list into an exhaustive match plus a name table. Not re-litigated.
- **`RetryPolicyConfig<T: Table>`**: the type parameter is the minimal mechanism for per-table defaults on *partial* tables (`#[serde(default)]` needs a per-table `Default`); one struct, three aliases. Only the `Policy` duplicate (N-6).
- **`gateway.rs` at 2371 lines**: ~800 lines are the outcome types with their rustdoc (the interface), ~870 one cohesive impl, ~500 tests. Deep; a split into `gateway/outcome.rs` would be navigational only. The one internal repetition is S-9.
- **`gateway/document.rs` projections**: kept by ADR 0009 item 3 for privacy; the `From`/`TryFrom` split is right (`Unnumbered` for the fallible one).
- **`Fault` constructors split** (`contract.rs:229-269` public shape + setters; `support.rs:105-209` service prose): a seam by audience (caller vs service); `contract` stays SDK-free. Acceptable; only R-3.
- **`identity.rs`**: the compile-time length proof (`:806-836`) and `TOKENS` exclusion are good depth; only R-2 and the un-ticketed F-10.
- **Contract `deny_unknown_fields` / `#[non_exhaustive]`**: consistent: requests closed and exhaustive, responses open and `#[non_exhaustive]`, schemas pinned by tests (`contract.rs:505-613`).
- **Error types**: `thiserror` throughout, `#[source]` chains (`ResolverUnavailable`, `FetchFailure`, `InputError::ItemOverflow`), display texts never echo a store's or resolver's message (tested `prologue.rs:502-512`, `:704-765`).
- **Handler attribute repetition** (`handlers.rs`): macro literals, pinned by the discovery test; nothing to share.
- **`tests/common/mod.rs` vs `src/test_support.rs`**: no duplication; `test_support` includes `common` by `#[path]` and adds only the parse seam (`wire`/`parse`/`assigned_order`), `LogCapture` and `open_gateway`. `service::journal::wire_document()` is deliberately a different renderer (every element).
- **Avoided words**: no `tenant`, `slug`, `health check`, `preamble`, `middleware`, `invalid payload`, `unknown outcome`, `error class`, `InvoiceDocumentExt`, `generation`, `orphan`, `re-create` in `src/` or `tests/`. `attempt` appears only for the SDK's `max_attempts` and the credential-fetch loop, both sanctioned by CONTEXT.md. `session` only for the szamlazz.hu cookie. `webhook` once, in the sanctioned `set_payments` race note.
- **CONTEXT.md names that match code exactly**: `Unanswered`, `Unconfirmed` (variants `Transport/Open/Unavailable/ReQueryFailed`), `FoundDocument`, `IssuedDocument`, `SzamlazzAnswer`, `Rejection`, `RejectionCode::{Szamlazz, Request}`/`REQUEST`, `LookupOutcome`, `CreateOutcome` (incl. `LiveAgain`, `Reconciled`, `DuplicateOrderNumber`), `StornoLookupOutcome`, `ProbeOutcome`, `TaxpayerOutcome`, `Gateway::{lookup,…,probe}`, `open_with_http`, `Execution`, `CALL_DEADLINE`, `ExternalId::{MAX_LEN, SEPARATOR, TOKENS, for_probe}`, `Endpoint::{is_cleartext, normalized}`, `IssueConfig::{MIN_INITIAL_DELAY, RE_CHECK_MARGIN}`, `WorkerConfigError::IssueDelayBelowFloor`, `ValidatedWorkerConfig::unchecked`, `StornoIntent::from_verified`, `support::best_effort`, the step names in `RUN_NAMES`.
