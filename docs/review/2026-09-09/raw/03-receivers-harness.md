# Architecture review: `szamlazz-adatkapcsolat`, `szamlazz-ipn`, `restate-e2e-harness` (research only, 2026-09-09)

Workspace at `8355ff5` (post ADR 0009, #170/#171/#172). `cargo clippy … --all-targets --all-features`: clean (exit 0). `cargo doc --no-deps --all-features`: zero warnings. README doctests: only `szamlazz-agent` compiles its README; the three crates here do not (already #148, #149; the harness crate is new, see H-R6).

Ticket staleness: #147, #148, #149, #130 (all four items), #129, #131, #74, #75 verified still open against current code. #141 is **partially stale**: bullet 3 (killed-invocation premise) is done, `prologue.rs:107-171` now holds the fetch by a `FetchHold` signal, not a clock; bullet 6 (journal-fixture archive discipline) is **obsolete** under ADR 0009 (no fixtures exist); bullets 1 (phase 2 still `sequentially!`, first panic hides the rest, `main.rs:186-193`), 4 (literal `Idempotency-Key`s, `pins.rs:43`) and 5 (`RUN_NAMES` vs discovered handlers, `pins.rs:203-205` only flags a handler an invocation exists for) still open. #155 unchanged.

---

## 1. `szamlazz-adatkapcsolat`

### 1.1 Seams

**A-S1 · Three enumerations of "which pushed kind", one of them the protocol's and private: Strong**
`document.rs:204-230` (`RootKind`, 4 variants, `pub(crate)`), `ack.rs:44-49` (`InvoiceDirection`, 2 variants, public, lives in the *ack* module but is what `InvoiceDocument::validate` and the archiver take), `archive.rs:42-58` (`DocumentKind`, 4 variants, private, with a conversion from `InvoiceDirection` at `:198-201`). The KEY_ERR-Ack-per-kind mapping, protocol speech, is written in the *axum adapter* (`axum.rs:421-432`, `key_error_response`) and `InvoiceAck::for_document` is `#[cfg(feature = "axum")]` (`ack.rs:105-114`): protocol behaviour gated on one adapter. Consequence for the crate's headline claim ("framework-free and `wasm32`-clean", `lib.rs:26-29`): a non-axum integrator cannot follow the protocol's own order (identify the root, authenticate, *then* parse) because `Document::identify` and `RootKind` are crate-private (`document.rs:133,141`); to shape a KEY_ERR Ack for an unauthenticated push they must run the full parse (three passes incl. base64 PDF decode) on a body they have not authenticated. `identify` is therefore tested only through the router (`tests/protocol.rs`), so the interface is not the test surface. Direction: one public `RootKind` in `document.rs` (or a `kind` module), `Document::identify(body) -> Result<RootKind, ParseError>` public, `RootKind::key_error_ack(&self) -> Vec<u8>` (or `ControlCode::ack_for(kind)`) in `ack`, `archive::DocumentKind` deleted, `InvoiceDirection` kept only if `RootKind::direction() -> Option<InvoiceDirection>` is not enough. Deletion test on the two private copies: pure knowledge duplication. No ADR conflict; extends #95.

**A-S2 · `Document` is `#[non_exhaustive]` while `Handler` makes a new stream a breaking change by design; the README quick start does not compile downstream: Strong**
`document.rs:28` vs `handler.rs:38-39` ("Every method is required so adding a stream … cannot silently acknowledge"). Adding a `Document` variant *already* breaks every `Handler` impl; `non_exhaustive` buys nothing and costs every consumer a `_ =>` arm. Verified: the README example (`README.md:14-27`) compiled as an external crate fails with `E0004: non-exhaustive patterns: '_' not covered … Document is marked as non-exhaustive`. The lib.rs doctest (`lib.rs:44-46`) hides this by using `let … else`. #148's "README is a doctest" would surface the compile error but not the design contradiction. Direction: drop `#[non_exhaustive]` from `Document` (an ADR-0007-scale note in the changelog; a new root is a 0.x bump either way), or keep it and fix the README with a wildcard and say why. Same reasoning applies to `InvoiceDirection` (`ack.rs:43`): exactly two invoice streams exist and the crate itself matches it exhaustively.

**A-S3 · The shape rule's justification does not cover the id of a bank transaction or a receipt: Worth exploring (ADR/#95 conflict)**
`document.rs:48-57` and CONTEXT *Shape vs content*: "refuses only what the receiver cannot Ack … a missing document id". `Ack` for a bank transaction or receipt batch carries **no id at all** (`ack.rs:153-201`), so a `<banktranz>` without `<id>` or a batch with one `<nyugta>` lacking `alap/id` *is* Ackable; yet `BankTransaction.id: i64` (`:1304`) and `ReceiptInfo.id: i64` (`:1390`) are required, and one bad receipt refuses the whole batch, which szamlazz.hu never resends. The stated rule (Ackability) and the implemented rule (identity) diverge exactly on the two kinds where a refusal is permanent. Direction: either the glossary says "identity, not Ackability" and `parse` reads a receipt's missing id as content (the batch still Acks), or the receipt id stays shape and the rule's text says so and why. Related: the per-element namespace check (`document.rs:171-202`) refuses an extension element in another namespace, contradicting `:69-70` ("a protocol extension the receiver must survive"); this is #74 B-13, marked info there, and deserves a decision rather than an "info".

**A-S4 · `ValidationError` is a string: Worth exploring**
`error.rs:76-89`: `#[non_exhaustive] struct { message: String }`, built from `format!("missing required {field}")` at `document.rs:1196-1221`. The interface is `Display`; `tests/document.rs:254,285,431,636` compare message strings. A consumer wanting "which element" (to log a metric per missing element, or to tolerate exactly `alap/adoszam`) must parse the text. A shallow interface over a deep implementation (the per-XSD tables). Direction: `enum ValidationError { MissingRequired { path: &'static str }, UnknownToken { path, token }, Negative { path }, Empty { path } }` with the same `Display`; the tests then match variants, and the `Display` text stays verbatim as CONTEXT requires.

**A-S5 · `KeyResolver`: one in-tree adapter: FINE with #130**
Adapters: `FixedKey` (`axum.rs:276-293`), test-only `Tenants`/`UnavailableResolver` (`tests/protocol.rs:799-828`); the CLI uses `router(key, …)` = `FixedKey`. By the rule this is a hypothetical seam, but a published crate's extension point is legitimately one-adapter in-tree. Its shape is wrong for the documented second adapter (#130 item 1, still current), the docs claim logging the router does not do (#130 item 2, `handler.rs:60-61` vs `axum.rs:389`), and the `Tenants` test resolver itself compares keys with `match` (`:806-810`), not constant-time, illustrating #130 item 3. Nothing new.

**A-S6 · `Ack` depth: FINE, two wrinkles**
`to_xml` hides quick_xml, the namespace-per-root rule, XML 1.0 character validation: deleting it forces real knowledge onto consumers. Wrinkles: asymmetric rendering interface (`InvoiceAck::to_xml(InvoiceDirection)` vs `Ack::to_bank_transaction_xml()` / `to_receipts_xml()`, `ack.rs:122,184,190`), which A-S1's single `RootKind` would unify; `accept(id)` dead under the router (#130 item 4).

**A-S7 · `archive`: not a pass-through; FINE**
`Archiver` carries policy: layout (`archive.rs:363-380`), the `if_not_exists` versioning loop (`:336-361`), PDF-stripped JSON (`:385-393`), name sanitising (`:410-424`). Whether it belongs in a protocol crate is #74 J-05-10. `ArchiveError` exposes `opendal::Error`/`serde_json::Error` (`:89-98`): inherent, since the constructor takes an `Operator`.

### 1.2 Naming vs domain

**A-N1 · `ControlCode::KeyError` names a control code "error": Worth exploring**
`ack.rs:23`, `InvoiceAck::key_error()` `:80`, `Ack::key_error()` `:164`, `axum.rs:421 key_error_response`. CONTEXT: "Control code … Not errors". The wire token is `KEY_ERR`, so the name mirrors the wire; still the Rust name says the thing the glossary forbids. `Disconnect` (for `KEY_DEL`) already shows the alternative style. Direction: `KeyUnknown` / `key_unknown()` (breaking, bundle with #130's 0.4).

**A-N2 · "response XML" for Ack: nit**
`ack.rs:1,116,182,188`, `handler.rs:42`. The glossary defines Ack with the same words, so it is tolerable in the definition; the three method docs "Renders the response XML" should say "Renders the Ack".

**A-N3 · Code terms absent from the glossary: glossary gap**
`tenant` (`axum.rs:34,36,64,204,364`), "multi-customer router" (`:203,218`), "stream" (`ack.rs:38`, `handler.rs:38`), "registration number / iktatószám", "redelivery", "fan-out", `BodyLimit`, `KeyResolver`, `RootKind`. The glossary's `_Avoid_: tenant` sits under the worker's *Account*; the receiver has no term for "the party a key resolves to". One term, in CONTEXT, would let the crate stop saying "tenant"/"customer" interchangeably.

**A-N4 · Everything else checked: FINE**
No `webhook`, `payment callback`, `push API`, `feed` anywhere; "Data Connection" only as the translation of the proper noun (`lib.rs:2`); "shape"/"content" used exactly as the glossary; `invalid document structure:` kept verbatim (`error.rs:58`); "required" used only in the XSD sense ("the XSD requires", `README.md:52-54`, `document.rs:1196`); `parse` lenient, `parse_strict` opt-in; `TransactionDirection::Other` matches CONTEXT.

### 1.3 Rust conventions

**A-R1 · `Handler::Error: Display` and `KeyResolver::Error: Display`, not `std::error::Error`: Speculative**
`handler.rs:62`, `axum.rs:62`. Forces `Fanout` to stringify (`fanout.rs:77,116`) and loses `source()` chains for every member; the router never reads the error anyway (#130 item 2). `Infallible` implements `Error`, so tightening costs nothing in-tree; it is a breaking trait change, so bundle with #130.

**A-R2 · `Unknown(i64)` vs `Other(String)` for the open-token pattern: nit**
`InvoiceAppearance::Unknown` (`document.rs:294`), `TransactionDirection::Other` (`:1241`); the agent crate uses `Other` for strings and `Unknown` for the appearance code too, so it is a workspace pattern (numeric → `Unknown`, token → `Other`), just undocumented. Record it or unify in #129.

**A-R3 · Checked and FINE**
`#[non_exhaustive]` on every public enum/struct with public fields (except A-S2's over-application); `#[must_use]` on `InvoiceAck`, `Ack`, `Fanout`, `ArchiverBuilder` types and on every getter; `From<i64> for InvoiceAppearance` infallible; private `TransactionDirection::from_code` infallible; `Pdf: AsRef<[u8]>` + `as_bytes()`; `# Errors` on every fallible `pub fn`, `# Panics` on `nest_at`; serde attrs uniformly `default, rename(deserialize = …)`; the `de` helpers honour the no-byte-offset rule (`split_at_checked`, slice pattern, `strip_suffix`); RPITIT `impl Future + MaybeSend` on both traits is right for MSRV 1.92 and wasm; `send_wrapper` gated under `target_arch = "wasm32"` and the `axum` feature; `doc_cfg` via `docsrs` (docs.rs sets it); `pub(crate) mod de`; zero `unwrap()`. Third-party error types in public API (`From<quick_xml::DeError>`, `error.rs:62`) → #131. Derive gaps (`PartialEq` on document types) → #131. Integer widths (`i32`/`i64` ids within one crate) → #129.

---

## 2. `szamlazz-ipn`

### 2.1 Seams

**I-S1 · One module plus one adapter: FINE**
`lib.rs` (types + `from_form_bytes`/`from_pairs`) and `axum.rs` (the extractor). `from_pairs` (`lib.rs:176`) is the framework-free seam for stacks that pre-decode forms; the axum `FromRequest` (`axum.rs:66-79`) is its one adapter. Cohesive; nothing to split.

**I-S2 · The glossary's two properties are modelled by *absence*, correctly: FINE**
"No reliable document-kind discriminator": there is no `kind` field; `document_number` (`:100`) replaces the older `invoice_number` (alias kept, `:99`), `proforma_number`'s doc says its presence is not a discriminator (`:115-117`). "Absolute snapshot": documented on the type and on `paid_gross` (`:79-90,103-107`); a type cannot prevent addition, so documentation is the ceiling here. `is_fully_paid()` inviting action on an unauthenticated body is #75 B-09.

**I-S3 · Leniency: #149, still current**
`lib.rs:203-208` (date), `:195,198` (amounts), `:222` (method) still hard-fail; tests `:371-397` lock it in. Nothing new; #149's acceptance criteria are the right shape.

### 2.2 Naming vs domain

**I-N1 · FINE.** "IPN" throughout; no `webhook`/`payment callback`; `PaymentNotification` with `#[doc(alias = "IPN")]`; "snapshot" as in the glossary. `IpnParseError` / `IpnRejection` / `PaymentNotification` mix the `Ipn` prefix and not; harmless.

### 2.3 Rust conventions

**I-R1 · `IpnParseError::Invalid { message: String }` drops `source()`: nit**
`lib.rs:203-207,251-254`: jiff/rust_decimal errors are `to_string()`ed. With #149 the date and amounts stop being errors, so this shrinks to `document_number`; fold into #149.

**I-R2 · `PaymentNotification::new` is a public test-fixture constructor: Speculative**
`lib.rs:132-147`, "Mainly for constructing test fixtures". Four positional parameters, two of them `Decimal`s in an order a caller can swap silently. With `#[non_exhaustive]` some constructor is needed; a `Default`-less struct-update is impossible from outside. Acceptable; note that #149 changes its parameter list (a `payment_method: String` that becomes `Option`), so it will churn.

**I-R3 · Checked and FINE**
`#[non_exhaustive]` on the struct and error; `serde` feature-gated derives with `alias`; `Hash`/`Eq` derivable and derived; `SOURCE_IPS: &[IpAddr]` typed (its rot is #75 B-12); `IpnRejection` wraps axum's `BytesRejection` privately and exposes it only via `source()` (`axum.rs:47-54`); `doc_cfg`; `# Errors`; no `unwrap`.

---

## 3. `restate-e2e-harness` (and its consumer under `restate-szamlazz/tests/e2e/harness/`)

### 3.1 Seams

**H-S1 · The step-name table *check* lives in the consumer while the crate ships only its three primitives: Strong**
Crate: `RunPath`, `RunPatterns::pattern`, `is_prefix_of_path` (`run_names.rs:18,117,128`); `is_prefix_of_path` is a three-line `zip().all()`, and the crate's own rustdoc (`run_names.rs:7-12`, `README.md:26-29`) states the full rule: every invocation's sequence a prefix of one of its handler's paths, every path walked in full, every handler seen in the table. That algorithm is written **only** in the consumer, `pins.rs:176-249` (~70 lines, no unit test, verified only against a live server), and would be rewritten by every second consumer. Deletion test on `is_prefix_of_path`: the consumer loses three lines. Deletion test on a crate-owned `check`: the consumer loses the whole invariant. Direction: `run_names::Table::check(&self, invocations: &[(String, Invocation)], journals: &BTreeMap<String, Vec<JournalEntry>>) -> Result<Walked, Violations>` (unpinned handlers, unexplained sequences, unwalked rows), unit-tested in the crate over scripted rows like the sampler is; `pins.rs` becomes one call plus the eprintln. Fits ADR 0009 (the table is "the sequence third" of a resume) and the crate's "building blocks" charter (`lib.rs:1-2`).

**H-S2 · The consumer's `Harness` re-exposes `Admin` method by method: Worth exploring**
`tests/e2e/harness/mod.rs:361-443,535-542`: `kill`, `cancel`, `await_status`, `sql`, `runs`, `journal`, `invocation`, `purge`, `all_journals`, `all_invocations` are one-line forwarders to `self.restate.admin()`; only `in_flight_on`/`await_in_flight_on`/`watch` add information (the `Target`). A pass-through layer of ten methods that will grow with every `Admin` method. Direction: `Harness::admin(&self) -> &Admin` (or `Deref<Target = Restate>`) and keep only the szamlazz-shaped methods.

**H-S3 · The ingress path grammar is the consumer's, eight times: Worth exploring**
`mod.rs:227,245,255,270,281,313,325,349`: `/restate/call/{svc}/{key}/{handler}`, `/restate/scope/{scope}/call/…`, `/restate/scope/{scope}/send/…`; `e2e_smoke.rs:100,120,134` writes the same grammar. `Restate::invoke(path: &str, …)` (`server.rs:455`) takes raw strings, so the knowledge of Restate's URL shape (scope segment placement, `call` vs `send`) is the consumer's. Direction: an `ingress::Call { service, handler, key: Option, scope: Option, mode: Call | Send }` builder in the crate, with `Restate::invoke(&Call, body, idempotency)`; `Reply::invocation_id` already knows `send` answers 202.

**H-S4 · `plain_http()` is a one-adapter public export whose documented second consumer does not exist: Worth exploring**
`lib.rs:112-121`: "a consumer's own loopback clients are built from it too." Callers: `server.rs:229` and `e2e_smoke.rs:174`, both the crate's own. The consumer builds its own `tls_certs_only(empty())` builder at `tests/common/mod.rs:40-46` (with the cookie jar, timeout and redirect policy the gateway needs, over `szamlazz_agent::reqwest`), and `Gateway::open_with_http` takes that. So the rule "one adapter = hypothetical seam" applies to `pub fn plain_http`. Direction: either `pub(crate)` it and correct the doc sentence and CONTEXT's *Restate e2e harness* entry (which lists it as an export), or re-export `reqwest` from the crate (as `szamlazz-agent` does, `lib.rs:78`) so a consumer can actually build on it without a second `reqwest` version in scope.

**H-S5 · `Admin`: not a bag: FINE, one locality note**
`admin.rs:133-471`: each method is one SQL string plus a projection, but the knowledge is real (journal v2's two-row runs, `Invocation::COLUMNS`, the purge being asynchronous, the poll deadlines and `poll_until`'s "report the last failure, never a probe started at the deadline" rule at `:43-73`). Deletion test passes. Locality: `Watch`/`Sampler`/`Retries` (`:473-665`) are the sampler, not the admin API; `introspection.rs:3-5` has to reach back into `admin` to name `Retries`. A `watch.rs` sibling would make the module map match `lib.rs:16-22`'s bullets. Speculative.

**H-S6 · Hard rule honoured: FINE**
`Cargo.toml:15-27`: `reqwest`, `restate-sdk`, `serde`, `serde_json`, `tokio`, `nix`; no `tracing`, no `restate-szamlazz`, no `wiremock`. No szamlazz type or symbol in `src/`. Note the glossary's "it logs nothing, it panics" is slightly off: `server.rs` `eprintln!`s nine informational/warning lines (pid and ports `:309`, signal `:191`, kill failures `:141,148`). Harmless; adjust the glossary wording. (Adjusted with the review.)

**H-S7 · Consumer `Reply` newtype exists to fix one type parameter: Speculative (consistent with #128)**
`tests/e2e/harness/ingress.rs:13-33`: 33 lines whose content is `self.0.fault::<Fault>()`. CONTEXT records the newtype as a decision, so leave it; a `type Reply = …` cannot add a method, which is the whole reason.

### 3.2 Naming vs domain

**H-N1 · szamlazz vocabulary in the published crate's rustdoc and unit tests: Worth exploring**
`run_names.rs:45-46,113-114` (`verify-storno-{number}`, `storno-1`), `:142-190` (`storno_invoice`, `delete_proforma`, `query_taxpayer`, `taxpayer-{prefix}`, `proforma-for-delete`), `admin.rs:786,799,815,862,884` (`create-invoice`, `lookup-invoice`). Not a dependency, but the crate that "knows nothing of any particular endpoint" (`lib.rs:27`) documents itself with one endpoint's step names on docs.rs. Direction: neutral names (`step-a`, `verify-{id}`) in doc examples and test tables; five minutes.

**H-N2 · "harness" for the crate: glossary conflict, resolve in the glossary**
CONTEXT *Restate e2e harness* `_Avoid_: harness (for the crate; the szamlazz Harness is the consumer's type)`. The crate calls itself "the harness" 36 times (`server.rs` 13, `README.md` 9, `lib.rs` 6, `gate.rs` 5, `admin.rs` 2, `introspection.rs` 1: "the harness's HTTP client", "a server the harness started"). The crate is *named* `restate-e2e-harness`; the rule cannot be honoured in its own docs. Reword the `_Avoid_` to "the szamlazz `Harness` type is the consumer's; 'the harness crate' when both are in scope". (Reworded with the review.)

**H-N3 · Two names for one thing inside the crate: nit**
`README.md:26` "the run-name matcher", `lib.rs:23` "the run-name matcher", `run_names.rs:1` "the *step-name table*'s matching". CONTEXT's term is *Step-name table*; pick one in the README bullet.

**H-N4 · `Launcher::Reuse { … }` (a variant) and `Reuse::Allowed | Never` (a policy) share a word: Speculative**
`gate.rs:24,70`. `launcher_or_skip(Reuse::Allowed)` may yield `Launcher::Reuse`; the same identifier names a source and a permission. `ReusePolicy` or `Launcher::Running` would separate them.

**H-N5 · Leftovers checked: FINE**
No `18080`/`19070`, no fixed port, no `testcontainers`, no container naming/labels; "docker" appears only in the `docker cp` recipe for obtaining the binary and in `host.docker.internal` (`gate.rs:183,191`, `lib.rs:46-54`, `README.md:38-51`), both legitimate, and the gate test asserts the failure message names no docker launcher (`gate.rs:266`). "pin" appears as the verb "a consumer pins its table" (`run_names.rs:9,56,113`, `README.md:26`) and `unpinned` in the consumer (`pins.rs:179-221`); the noun "run-name pin" is gone. CONTEXT retired "pin" from the *journal* vocabulary; the consumer's `mod pins` and "the run-wide pins" (`main.rs:44,247`) are the remaining noun uses, worth a rename to `checks` when #141 is picked up.

### 3.3 Rust conventions

**H-R1 · `ServerSpec::validate(&self)` panics; `Document::validate` returns `Result`: Worth exploring (workspace consistency)**
`gate.rs:165-171` vs `document.rs:120`. Same verb, opposite contracts, across crates in one workspace. The crate's style is panicking (documented at `lib.rs:79-83`), and `is_valid_name` is the pure query; rename the panicking one `assert_valid` (or inline it into `launch`, its only caller `:185`).

**H-R2 · Nothing is `#[non_exhaustive]`; every row type has public fields: Speculative**
`Reply` (`ingress.rs:12-22`), `Retries` (`admin.rs:493-514`), `Invocation`, `JournalEntry` (`introspection.rs:18-28,80-93`), `Deployment` (`server.rs:530-535`), `RunPath`, `ServerSpec`, `Target`, `Launcher`. Consistent inside the crate (plain data, `const`-constructible specs) and defensible for a 0.1 test-support crate, but every added header on `Reply` or column on `Invocation` is a breaking release. Decide once and write it in `lib.rs`.

**H-R3 · Panic-not-`Result` style: consistent and documented: FINE**
`#![allow(clippy::missing_panics_doc)]` with rationale (`lib.rs:79-83`); the two genuine `Result`s (`Admin::sql`, `server_gate`) carry `# Errors`; 31 `expect`s with noun-phrase messages, zero `unwrap`; informative panics (`Process::exited` with the log tail, `poll_until` reporting the last failure) are where the docs say. `Admin::sql`'s `String` error is fine for a harness and the sampler is the reason it is a `Result` at all.

**H-R4 · Unix gating: FINE**
`compile_error!` first (`lib.rs:85-89`), `#[cfg(unix)]` on the two spawning modules to keep the diagnostic to one line, `nix` under `target.'cfg(unix)'` (`Cargo.toml:26-27`), `e2e_smoke.rs:12` `#![cfg(unix)]`. `tokio`'s `signal` feature is unconditional but unreachable on non-unix.

**H-R5 · `FEATURES` is a hard-coded 1.7.8 policy: Speculative**
`gate.rs:111-115`, asserted iff-present at `server.rs:374-382`. A consumer cannot say "don't care" about a feature, and a Restate release that turns one on by default breaks every `flags: &[]` spec. Fine today; make it `ServerSpec`'s (a `features: &[(&str, bool)]`) when the second consumer appears.

**H-R6 · Small items**
`README.md:57-75` duplicates the `lib.rs:58-77` example; only the lib copy is a doctest (verified: 1 doctest). `Launcher::launch` reads `RESTATE_ENDPOINT_HOST` (`gate.rs:186-188`) although the gate's purity story puts environment reads in `launcher_or_skip`. `Restate::deploy` binds the endpoint on `0.0.0.0` (`server.rs:420`) even in spawned mode where `127.0.0.1` would do (the reuse mode needs it reachable from a container); the admin API is loopback-only for the reason given at `:41-43`, the endpoint is not, and it has no identity key. `Reply::fault`'s three envelope assertions (`ingress.rs:40-63`) are pure and have no unit test; only the smoke test and the consumer exercise them.

---

## 4. Checked and found FINE (summary)

- **Clippy** (all targets, all features, pedantic + `unwrap_used`): clean. **Rustdoc**: no warnings. MSRV 1.92 features used (`let` chains, `is_multiple_of`, `split_at_checked`, RPITIT) all inside.
- **adatkapcsolat**: shape/content rule applied consistently in the `de` helpers (bool and number lexical failures are shape, date is content, unknown tokens kept); the `de` helpers' no-byte-offset rule honoured; `parse` / `parse_strict` / `validate` triad matches CONTEXT; the router's decision order (401 → 413 → identify-400 → resolve → parse-400 → 500) matches its own docs and the README; feature gates and `doc_cfg` correct; serde attribute convention uniform; `#[must_use]` and `# Errors`/`# Panics` complete.
- **ipn**: crate shape, the two glossary properties modelled by absence, feature gates, derives, error `source()` chain through `IpnRejection`.
- **harness**: the hard rule (dependencies, no `tracing`, no szamlazz symbol); the server gate matches the glossary (reuse → binary → skip/fail-in-CI, `Reuse::Never` for the canary); free ports, unique base dir per launch, process group kill, SIGINT/SIGTERM handler; `poll_until` bounding waits as a whole; the sampler and matcher pure and unit-tested; `sql_literal` escaping; `Watch` ending on completion and cancelling a sample in flight.
