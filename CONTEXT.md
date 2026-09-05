# szamlazz.hu Integration

Rust crates for integrating with szamlazz.hu, a Hungarian invoicing service: an outbound API client, two inbound receivers for szamlazz.hu-initiated pushes, and a Restate-backed durable worker that issues documents exactly once per order.

## Language

### Integration surfaces

**Számla Agent**:
The szamlazz.hu XML API for issuing and querying documents. Single HTTPS endpoint; the multipart form field name selects the operation. Crate: `szamlazz-agent`.
_Avoid_: "the API" (ambiguous — there are three surfaces)

**IPN**:
Instant payment notification. A form-urlencoded POST szamlazz.hu sends to a configured URL when an invoice's or proforma's paid amount changes. The amounts are an absolute current payment-status snapshot, not a new payment or delta, and the payload has no reliable document-kind discriminator. Unauthenticated; retried every 3 minutes up to 10 times. Crate: `szamlazz-ipn`.
_Avoid_: webhook (too generic), payment callback

**Adatkapcsolat**:
The Online Pénzügyi Adatkapcsolat (Financial Data Connection) product. szamlazz.hu pushes full XML documents (outgoing invoices, incoming invoices, bank transactions, receipts) to a single registered receiver URL, authenticated by the `X-Szamlazzhu-Key` header; the document type is identified by the XML root element. Crate: `szamlazz-adatkapcsolat`.
_Avoid_: data connection, push API, feed

**Ack**:
The response XML an Adatkapcsolat receiver returns for a pushed document: echoes the document id (invoices, optionally with a registration number / iktatószám) or carries a control code.
_Avoid_: response (overloaded)

**Control code (KEY_ERR / KEY_DEL)**:
Deliberate protocol speech in an Ack: KEY_ERR tells szamlazz.hu the key is wrong (stop sending until it changes); KEY_DEL severs the connection. Not errors — errors (non-200) mean "retry within 72 hours".

### Documents

**Invoice (számla)**:
A finalized request for payment, identified by an invoice number (számlaszám). Created via the Agent's invoice operation.

**Invoice direction (outgoing / incoming)**:
Which ledger side an invoice sits on for the account holder: outgoing (kimenő) = issued by the account holder, incoming (bejövő) = received from a supplier and registered with szamlazz.hu. Orthogonal to invoice kind (proforma, storno, prepayment, …). Distinct from bank-transaction direction — an incoming invoice is settled by an outgoing transaction.
_Avoid_: sales/purchase invoice, AR/AP, inbound/outbound

**Proforma (díjbekérő)**:
A payment request that is not yet an invoice. Created via the invoice operation with a flag; deletable (real invoices are not — they can only be reversed).

**Prepayment invoice (előlegszámla)** / **Final invoice (végszámla)**:
Advance invoice and the invoice that settles it. Both are invoice-operation flags.

**Corrective invoice (helyesbítő számla)**:
An invoice that corrects a previously issued one, referencing its number.

**Storno invoice (sztornó)**:
The reversal of an issued invoice. A distinct Agent operation, not an invoice flag. Idempotent on the server: repeating the storno of an already reversed invoice echoes the existing storno number as success, with no error code and no second document. Sent on a proforma or delivery note it is a success-shaped no-op that echoes the requested number unchanged.
_Avoid_: cancellation, void

**Receipt (nyugta)**:
A simpler proof-of-payment document with its own create/reverse/query/send operations. The only document type with an idempotency key (hívásazonosító).

**Delivery note (szállítólevél)**:
A non-financial document listing delivered goods; an invoice-operation flag.

### Agent concepts

**Agent key (számlaagentkulcs)**:
API-only credential passed inside the request XML. Preferred over username/password.

**Credit entry (jóváírás / kifizetés)**:
A payment registered against an invoice via the Agent. IPN reports the resulting current payment status, not the individual credit entry.
_Avoid_: payment (overloaded — reserve for the buyer's act)

**Response version (válaszverzió)**:
Agent request field selecting the response body format: 1 = plain text or raw PDF bytes, 2 = structured XML with base64 PDF.

**Outstanding amount (kintlévőség)**:
The unpaid remainder of an invoice's gross total.

**VAT rate (áfakulcs)**:
A numeric percentage or a NAV-defined special code (AAM, TAM, EUT, KBAET, …) on a line item. The code set is NAV-driven and changes over time — it is an open set, not a fixed enum.

**Line item (tétel)**:
One row of a document: name, quantity, unit, net unit price, VAT rate, and net/VAT/gross values whose arithmetic szamlazz.hu verifies server-side.

### Restate worker concepts

**Order**:
The Restate Virtual Object through which every document of one order number is issued, registered with Restate as `Szamlazz.Order`. Its key is the order number (rendelésszám) trimmed of leading/trailing whitespace, case preserved — exactly what szamlazz.hu matches on; it carries no account marker, because Restate namespaces the key per *Scope* (the same order number under two scopes is two instances with two locks). It keeps no state: the object exists for its per-key lock (same-key handlers run one at a time, which serializes issuing per order), and every handler answers from szamlazz.hu — the *Account* the invocation resolved to — through the order's external ids. Every handler first runs the *Prologue*: it pins the namespace, resolves the request's scope to its *Account* in a durable step named `account` — once per invocation, journaled, so the invocation finishes on the account it started on — fetches the credentials outside the journal for this execution, and opens the *Gateway* for this execution; nothing of that outlives the execution. Crate: `restate-szamlazz`.
_Avoid_: order object, invoice workflow, ledger (there is none — szamlazz.hu is the source of truth), tenant object

**External id (szamlaKulsoAzon)**:
The Agent's optional per-document identifier, used by the worker as the identity handle. Deterministic from the key alone under the deployment's *Namespace* — `{namespace}:{order}:{kind}` for the four kinds, `{namespace}:{order}:corrective:{correction_id}`, `{namespace}:{order}:storno:{number}`, `{namespace}:by-number:{number}:storno` for an unmanaged storno — so any invocation can find what an earlier one issued; the two-segment `{namespace}:check-account` is the sentinel *check_account* probes and nothing the service issues carries. It carries no account marker: the namespace is one per deployment and shared by every account, and the *Scope* selects the account. Not unique server-side and never echoed in responses: a query returns the newest holder, and every document found by it is validated (order number, `tipus`, `teszt`, supplier pin of the resolved *Account*) before it is trusted. The only namespace marker any worker response carries.
_Avoid_: idempotency key (szamlazz.hu does not treat it as one), generation (no counter — the newest holder is the answer), external reference, account id or scope inside it (the scope selects the account; the id carries only the namespace)

**Namespace**:
The external-id prefix of this deployment (`{namespace}:{order}:{kind}`); chosen by the operator, opaque to szamlazz.hu, permanent — changing it would hide every document issued so far. 1–16 bytes of `[a-z0-9-]`; `:` is excluded because it is the separator. One per deployment, shared by every *Account* it serves. Configured as the top-level `namespace` key of the deployment configuration, beside `[issue]` and `[resolve]` on `WorkerConfig`; it is not part of any account. Pinned per invocation in the *Prologue*. Type: `restate_szamlazz::config::Namespace`.
_Avoid_: slug (the pre-#20 name), prefix (ambiguous with the invoice number prefix), tenant prefix

**Account**:
One szamlazz.hu account as the worker knows it: a resolver-owned id, mode (`live` / `test`; default `live`, always validated against `teszt` — a test account configured as live fails loudly on its first found document, on any handler), optional supplier id (required in the multi-account shape — the only server-side account identity a found document exposes), endpoint, document defaults, seller block and a *Credential ref* — never its agent key; resolved once per invocation and journaled, so an invocation finishes on the account it started on and the Restate UI can show it for the retention period without exposure. Additive-only for the journal: new fields default, nothing is renamed or removed. Ownership validation reads its pins on every document any handler finds: under one of our external ids a document is ours when it carries the order number, the `tipus` of the kind, `teszt` == mode, and the supplier id is unset or matches (else `conflict{external_id_collision}`); found by number — `Szamlazz.Order`'s verifies, `Szamlazz.Agent.query` and `storno` — it must carry `teszt` == mode and a matching or unset supplier id (else the `account_mismatch` fault, naming the observed and expected pins, never the key). `Szamlazz.Agent.set_payments` is the one exemption: it sends without a query, and a credit entry is not a legal document. No namespace — that is a deployment setting. Type: `restate_szamlazz::account::Account`.
_Avoid_: tenant, customer, organizer (the caller's concept; an account is what an organizer or event maps to), account config (the static resolver's configuration is a different type), the agent key as part of it

**Credential ref**:
The opaque reference an *Account* carries to its credentials, handed to the *Credential store* on every handler execution (`CredentialStore::fetch(credential_ref)`). Owned by the resolver that produced the account; journaled with it, since it is not a secret. The static resolver sets it to the account's `id`. Type: `restate_szamlazz::account::CredentialRef`.
_Avoid_: reference without qualification (reserved for this), key id, secret name, scope (the scope selects the account; the ref selects its credentials)

**Account resolver**:
The pluggable trait (`AccountResolver::resolve(scope: Option<&str>)`) that maps a request's *Scope* to its *Account*; object-safe, boxed futures, `Debug` supertrait. Answers as data: unscoped (no scope and no unscoped account), unknown (the scope names no account); `unavailable` is a retryable fault whose display text never echoes the source's own message. Its half of the safety contract: one szamlazz.hu account is reachable under exactly one scope value (unscoped counts as a value), no fan-in — the static resolver checks this at load time, a database-backed resolver must guarantee it itself, because the worker cannot detect fan-in at runtime and `check_account` only echoes configuration — and the mapping is append-only. The static resolver (`restate_szamlazz::account::StaticResolver`, built with `TryFrom` from serde-only configuration) implements both this and the *Credential store* with the agent key inline and `credential_ref = id`, in one of two mutually exclusive shapes: `[account]` — `resolve(None)` is the account, any scope is unknown — or `[accounts.<scope>]` — `resolve(None)` is unscoped, a scope is its account or unknown; the multi shape is checked at load time (a supplier id on every account; unique supplier ids, `(endpoint, agent key)` pairs and ids; scope keys `[a-z0-9_]` of at most 36 bytes). There is no default account. Bundled with the store as `Accounts`, shared by both Restate services.
_Avoid_: tenant resolver, account lookup (lookup is the first durable step of issuing), directory, account cache (a resolver may cache internally; the worker does not)

**Scope**:
The Restate scope value a request arrives under (`/restate/scope/{scope}/call/…`); the caller's identifier for a szamlazz.hu account and the only channel for selecting one — never a header, a body field or the Virtual Object key. Opaque to the worker: it is handed to the *Account resolver* as is. Restate namespaces the Virtual Object key and the `Idempotency-Key` per scope (both are part of the invocation's identity, and the scope is the partition key), so the same order number under two scopes is two `Szamlazz.Order` instances and the same key under two scopes is two invocations; a path segment of each request, not a session — callers set it on every call. One account ⇔ one scope, and unscoped counts as a scope value (the single-account shape serves its account unscoped; the multi-account shape refuses unscoped requests). Format: Restate's `[a-zA-Z0-9_.-]`, non-empty, at most 36 characters (ASCII, so bytes; a dashed UUID is exactly 36); the static resolver's keys are the strict subset `[a-z0-9_]` so environment overrides can address them. Routing, not authorization: the ingress sits behind a gateway that sets the scope from the authenticated identity, never forwards a caller-supplied scope path and strips `x-restate-*` request headers. Reaches the worker only under protocol v7, which Restate's ingress does not gate a scoped path on — *check_account* per scope after every deploy is the check. Kafka ingress is untested in multi-account mode. Decision: ADR 0006.
_Avoid_: tenant id, tenant, account header, reference (see *Credential ref*), account id (the *Account*'s `id` is the resolver's own identifier, not the scope), event or organizer as the scope (they map to an account in the caller)

**Credential store**:
The pluggable trait (`CredentialStore::fetch(credential_ref)`) that returns an account's credentials by its *Credential ref* — the Számla Agent crate's `Credentials`, which has no serde implementation, so the compiler rejects any attempt to journal it (a narrow guard: `AgentKey::expose()` is one line from journalable; the e2e journal scan is the real guarantee). Its half of the safety contract: fetched on every handler execution, including replays, never journaled, held only for that execution — a rotation is picked up on the next execution of every in-flight invocation. Answers: the credentials, `gone` (the reference is not known), `unavailable` (retryable; display never echoes the source); both end as a terminal `unavailable` after a short in-process retry, never a Restate retry. The gateway is opened from a resolved *Account* and freshly fetched credentials (`Gateway::open`) over a fresh Számla Agent client every time — a boundary, not a performance choice: the default `reqwest::Client` keeps szamlazz.hu's `JSESSIONID`, and a shared client would carry one account's session into another's request.
_Avoid_: secret store (generic), key store, vault (a product), caching credentials across executions, session (the szamlazz.hu cookie is what the fresh client isolates)

**Idempotency-Key**:
Restate's ingress retry identity, sent by the caller as a header per logical request; Restate dedupes retries and attaches concurrent duplicates to the in-flight invocation. Deduplicated per *Scope*: the same key under two scopes is two invocations. Recommended, never relied on for safety. Rotate it after a terminal error: Restate replays a failed invocation's stored completion for the retention period (verified), so the same key would repeat the failure instead of reconciling. A Pretix-style caller uses the webhook notification id.
_Avoid_: request id (v1's body field; gone), correlation id

**Outcome**:
A domain result returned as data (HTTP 200): `issued`, `already_issued`, `reconciled`, `reversed`, `rejected`, `conflict{reason}`. Errors (`TerminalError`) are reserved for faults — the six codes of `TerminalCode`, which every handler may raise: `outcome_unknown` (500), `unavailable` (503), `account_mismatch` (409), `invalid_input` (400), `credentials_rejected` (503), `unknown_account` (400); the by-number `Szamlazz.Agent` handlers also answer a miss as 404 `not_found` and pass a szamlazz.hu error through as 422 — and always mean "outcome unknown, retry with a new `Idempotency-Key` or read `get`", never "no document exists"; the ingress marks them `x-restate-error-source: invocation`, which a caller pages on rather than auto-retries. `credentials_rejected` is szamlazz.hu answering 3/135/136/164 to any step: the worker's agent key is wrong, not the request; the execution that raised it issued nothing (szamlazz.hu answers these codes before acting), but an earlier one may have landed, so it is a fault, never `rejected`, and 503 rather than a 4xx. `unknown_account` is the prologue's `account` step finding no account for the request — unscoped where accounts are scoped, or a scope no account is reachable by; raised before anything is issued, and the same request never succeeds, so the caller fixes the scope rather than retrying. `unavailable` also covers the prologue's own faults — the resolve policy exhausted, the credential store gone or unavailable — and *check_account*'s probe when its exchange settled nothing. No response names the account; `external_id` is the only namespace marker, and `order_key` in a storno response is meaningful only under the same scope.
_Avoid_: error/failure for a rejection or conflict, status

**Reissue**:
Issuing a new document of a kind after the one under its external id was reversed — by the service, the UI or anyone, at any distance in time: the service needs nothing from Restate to do it months later, only the key and the scope the caller recorded. Always explicit: `reissue: true` (with a new `Idempotency-Key`); without it the create returns `outcome: reversed`. Explicit because the service has no record of who reversed and cannot tell a stale retry of the original create from a deliberate new request; the flag on a live document is `conflict{live}`, so it can never cause a duplicate. The new document becomes the newest holder of the same external id. A reissue whose create reply was lost is reported as `issued` by the re-executed create step, not as a conflict.
_Avoid_: re-create, retry (a retry targets the same document)

**Reversal (as observed)**:
A document is reversed when szamlazz.hu reports `<sztornozott>true</sztornozott>` on it; the storno document carries `hivszamlaszam` = original and is the order-number hint only until something newer is issued under the order. The service does not track who reversed a document or when.
_Avoid_: reversal origin (v1 concept; gone), cancellation

**Foreign document**:
A live invoice-kind document (`SZ`, `ES`, `VS`) found under the order number by the order-number hint that is neither known to be ours nor the document seen under our external id — including another channel or namespace on the same szamlazz.hu account. Detected live in the lookup step, on every kind but correctives, unconditionally (there is no setting); never adopted, nothing recorded; the create returns `conflict{foreign}`. A duplicate-order-number answer (71/152) whose external-id re-query finds our document reversed or absent is the same situation seen from the create step: the order-number query names the existing document (`conflict{duplicate_order_number, existing_number?}`), never adopts it.
_Avoid_: external document, orphan

**Lookup step**:
The first of the two durable steps of issuing (`lookup-{kind}`): one read-only `ctx.run` that queries our external id and, for every kind but correctives, takes the order-number hint. It settles every case that needs no create — a live document of ours (`already_issued`, or `conflict{live}` with `reissue`), a reversed one (`reversed{storno_number}`, or proceed with `reissue`), an invalid holder (`conflict{external_id_collision}`), a foreign document (`conflict{foreign}`) — and otherwise hands the create step what it saw. Gateway fn: `Gateway::lookup`; outcome type `LookupOutcome`.
_Avoid_: pre-query (the create step has its own), attempt

**Create step**:
The second durable step of issuing (`create-{kind}`): one `ctx.run` under the issue policy whose every execution is query-first *inside the closure* — the external-id query first, the send only when nothing live of ours is there. A live document that is not the reversed one the lookup saw was issued by an earlier execution and is answered as `issued` without sending. A lost reply is re-queried once, immediately; nothing found makes the step **unconfirmed**, and Restate re-executes it after the policy's delay. The query lives inside the closure because a separate journaled pre-query would replay its stale "nothing" on the retry. Gateway fn: `Gateway::create` → `Result<CreateOutcome, Unconfirmed>`.
_Avoid_: attempt, issue step (the whole two-step sequence is "issuing"), send (one part of the step)

**Unconfirmed**:
The one error of the create and storno steps: szamlazz.hu's answer is not known — a transport failure, an open code (1, 55, 56 without a number, `szlahu_down`) or, on a create, a 71/152 with nothing under the order — after the immediate re-query found nothing. Retryable to the SDK, so the issue policy re-executes the step; its exhaustion is the `outcome_unknown` fault. Every *known* answer, including rejections and duplicates, is `Ok` data.
_Avoid_: unknown outcome (the fault is `outcome_unknown`; this is what precedes it), transport error (one cause of it)

**Issue policy**:
The run retry policy of the create and storno steps: `[issue]` in the deployment configuration — `max_attempts` executions, `initial_delay` growing by `factor` to `max_delay`, bounded by `max_duration` — mapped field for field onto `RunRetryPolicy::new()`. Deployment-level (on `WorkerConfig`), shapes no journal entry. Set explicitly because the SDK's default run policy sends no retry delay and the server would spend the handler's `invocation_retry_policy` instead. `max_duration` is the hard bound; the attempt count is not durable across every replay (ADR 0004). Type: `restate_szamlazz::config::IssueConfig`.
_Avoid_: attempt budget, backoff (the loop is gone), retry policy without qualification (the handlers have their own)

**Consumed proforma**:
A proforma that szamlazz.hu removed from its query surface because an invoice or prepayment converted it — explicitly by reference or implicitly by shared order number. Derived live in `get`: the proforma is absent under its external id while the invoice or prepayment carries `hivdijbekszam`, reported as `{state: consumed, by}`. Distinct from a deleted proforma, which is simply absent and may be recreated.
_Avoid_: converted (the invoice is converted from it; the proforma is consumed), deleted

**Gateway (`gateway` module) / `Szamlazz.Agent` service**:
The module that speaks to szamlazz.hu on behalf of one account: `restate_szamlazz::gateway::Gateway` owns the `szamlazz_agent::Client` and the account it is opened for, and exposes one plain async fn per durable step (`lookup`, `create`, `verify`, `query`, `hint`, `lookup_storno`, `storno`, `delete_proforma`, `set_payments`, `probe`) with outcome-as-data — every expected szamlazz.hu outcome is a value, never an `Err`; the `Err(Unconfirmed)` of the create and storno steps is reserved for an answer that is not known. Not a second client — the Számla Agent `Client` is the transport it wraps. Every read of account configuration by the Restate services (ownership-validation pins, document defaults, seller block) goes through `Gateway::account()`; the services themselves hold no gateway — only the *Accounts* bundle and the deployment-level `WorkerConfig` (namespace, issue and resolve policies) — and the *Prologue* opens one (`Gateway::open`) per handler execution, over a fresh client, from the resolved *Account* and freshly fetched credentials. `Szamlazz.Order` calls the gateway inside `ctx.run`. The `Szamlazz.Agent` Restate service is a thin stateless facade over the same module for by-number operations (query, credit entries, storno of unmanaged documents) and the *check_account* probe. Neither Restate service calls the other. First named `steps` (ADR 0001).
_Avoid_: steps (for the module — the pre-rename name), client (for the module — it is not a second client), session (for the module — the szamlazz.hu cookie is what its fresh client isolates), "the agent" without qualification, Invoice service

**Prologue**:
The four steps every handler of both Restate services runs after parsing its key and before its operation: **pin** the namespace in a pure durable step (`namespace`); **resolve** the request's scope to its *Account* in a durable step named `account` under the *Resolve policy* — unscoped and unknown journaled as data and answered as `unknown_account`, an unavailable resolver retried, its exhaustion `unavailable`; **fetch** the account's credentials outside the journal on every execution with a short in-process retry, then terminal `unavailable`; **open** the *Gateway* over a fresh client. Its decisions are pure functions (`service::prologue`); its durable behaviour is observable only under Restate. Helper: `support::{object, shared, service}::prologue`.
_Avoid_: preamble, setup, middleware (there is no interception layer; it is the first lines of every handler), init

**check_account (`Szamlazz.Agent.check_account`)**:
The read-only probe for onboarding and deploy pipelines, and the deploy-time canary for the experimental Restate flags: the *Prologue* like every handler, then one durable step (`probe`) — a query of the sentinel external id `{namespace}:check-account`, which nothing the service issues carries, expecting "not found" — answering the *Scope* the SDK saw, the *configured* account (`id`, `mode`, `supplier_id`), the *Namespace* and whether szamlazz.hu accepted the credentials: `{state: ok}`, or `{state: rejected, code, message}` on 3/135/136/164 **as data, not a fault**. `scope: null` under a scoped call means the server did not forward the scope (`protocol_v7` off — the ingress does not refuse a scoped path for it); the probe is the only defence against that case, since the worker has no per-request signal of "was this call scoped?" it is willing to depend on (an ingress-path guard on the undocumented, caller-overridable `x-restate-ingress-path` header was considered and dropped). Credential acceptance is its only szamlazz.hu-verified fact (the supplier id appears only in found-document bodies). Issues nothing; no input; explicit `journal_retention` so the leak assertion can scan it. Called under each configured scope after a deploy, it proves the scope reaches the worker, resolves to the configured account and its key works; it cannot detect fan-in — it only echoes configuration. Gateway fn: `Gateway::probe` → `ProbeOutcome`; response type `contract::CheckAccountResponse`.
_Avoid_: health check (it checks one account under one scope, not the process), login test, ping

**Execution**:
What one handler execution runs on: the *Gateway* the *Prologue* opened and the `WorkerConfig` with the pinned namespace. Built by the prologue, dropped with the execution — no gateway, client or credentials outlive it; "handler execution" is the unit, since a run-policy retry re-executes the whole handler with replay. Type: `service::prologue::Execution` (the handler bodies are its methods).
_Avoid_: attempt (for the handler; the SDK's word for a run retry is also "attempt", and neither is journaled), session (the szamlazz.hu cookie), context (the SDK's `ctx`)

**Resolve policy**:
The run retry policy of the *Prologue*'s `account` step: `initial_delay` growing by `factor` to `max_delay`, bounded by `max_duration`, no attempt cap. Deployment-level (on `WorkerConfig`), shapes no journal entry, set explicitly for the same reason as the *Issue policy*. Defaults `1s → 10s`, `1m`; configured as `[resolve]` in the deployment configuration. Type: `restate_szamlazz::config::ResolveConfig`.
_Avoid_: retry policy without qualification, account policy
