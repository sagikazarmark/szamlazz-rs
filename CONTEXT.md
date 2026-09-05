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
The Restate Virtual Object through which every document of one order number is issued, registered with Restate as `Szamlazz.Order`. Its key is the order number (rendelésszám) trimmed of leading/trailing whitespace, case preserved — exactly what szamlazz.hu matches on. It keeps no state: the object exists for its per-key lock (same-key handlers run one at a time, which serializes issuing per order), and every handler answers from szamlazz.hu through the order's external ids. Crate: `restate-szamlazz`.
_Avoid_: order object, invoice workflow, ledger (there is none — szamlazz.hu is the source of truth)

**External id (szamlaKulsoAzon)**:
The Agent's optional per-document identifier, used by the worker as the identity handle. Deterministic from the key alone under the deployment's namespace — `{namespace}:{order}:{kind}` for the four kinds, `{namespace}:{order}:corrective:{correction_id}`, `{namespace}:{order}:storno:{number}` — so any invocation can find what an earlier one issued. Not unique server-side and never echoed in responses: a query returns the newest holder, and every document found by it is validated (order number, `tipus`, `teszt`, supplier pin) before it is trusted.
_Avoid_: idempotency key (szamlazz.hu does not treat it as one), generation (no counter — the newest holder is the answer), external reference

**Namespace**:
The external-id prefix of this deployment (`{namespace}:{order}:{kind}`); chosen by the operator, opaque to szamlazz.hu, permanent — changing it would hide every document issued so far. 1–16 bytes of `[a-z0-9-]`; `:` is excluded because it is the separator. Configured as `account.slug` for now. Type: `restate_szamlazz::config::Namespace`.
_Avoid_: slug, account slug, prefix (ambiguous with the invoice number prefix)

**Account**:
One szamlazz.hu account as the worker knows it: a resolver-owned id, mode (`live` / `test`; default `live`, always validated against `teszt` — a test account configured as live fails loudly on its first found document), optional supplier id, endpoint, document defaults, seller block and a credential reference — never its agent key; resolved once per invocation and journaled, so an invocation finishes on the account it started on and the Restate UI can show it for the retention period without exposure. Additive-only for the journal: new fields default, nothing is renamed or removed. Ownership validation reads its pins: a found document is ours when it carries the order number, the `tipus` of the kind, `teszt` == mode, and the supplier id is unset or matches. No namespace — that is a deployment setting. Type: `restate_szamlazz::account::Account`.
_Avoid_: tenant, customer, account config (the static resolver's configuration is a different type), the agent key as part of it

**Account resolver**:
The pluggable trait (`AccountResolver::resolve(scope: Option<&str>)`) that maps a request's Restate scope to its *Account*; object-safe, boxed futures, `Debug` supertrait. Answers as data: unscoped (no scope and no unscoped account), unknown (the scope names no account); `unavailable` is a retryable fault whose display text never echoes the source's own message. Its half of the safety contract: one szamlazz.hu account is reachable under exactly one scope value (unscoped counts as a value), no fan-in — the static resolver checks this at load time, a database-backed resolver must guarantee it itself, because the worker cannot detect fan-in at runtime and `check_account` only echoes configuration — and the mapping is append-only. The static resolver (`restate_szamlazz::account::StaticResolver`, built with `TryFrom` from serde-only configuration; single `[account]` shape for now) implements both this and the *Credential store* with the agent key inline and `credential_ref = id`; `resolve(None)` is the account, any scope is unknown. Bundled with the store as `Accounts`, shared by both Restate services.
_Avoid_: tenant resolver, account lookup (lookup is the first durable step of issuing), directory

**Credential store**:
The pluggable trait (`CredentialStore::fetch(credential_ref)`) that returns an account's credentials — the Számla Agent crate's `Credentials`, which has no serde implementation, so the compiler rejects any attempt to journal it. Its half of the safety contract: fetched on every handler execution, including replays, never journaled, held only for that execution — a rotation is picked up on the next execution of every in-flight invocation. Answers: the credentials, `gone` (the reference is not known), `unavailable` (retryable; display never echoes the source). The gateway is opened from a resolved *Account* and freshly fetched credentials (`Gateway::open`) over a fresh Számla Agent client every time — a boundary, not a performance choice: the default `reqwest::Client` keeps szamlazz.hu's `JSESSIONID`, and a shared client would carry one account's session into another's request.
_Avoid_: secret store (generic), key store, vault (a product), caching credentials across executions

**Idempotency-Key**:
Restate's ingress retry identity, sent by the caller as a header per logical request; Restate dedupes retries and attaches concurrent duplicates to the in-flight invocation. Recommended, never relied on for safety. Rotate it after a terminal error: Restate replays a failed invocation's stored completion for the retention period (verified), so the same key would repeat the failure instead of reconciling.
_Avoid_: request id (v1's body field; gone), correlation id

**Outcome**:
A domain result returned as data (HTTP 200): `issued`, `already_issued`, `reconciled`, `reversed`, `rejected`, `conflict{reason}`. Errors (`TerminalError`) are reserved for faults — `outcome_unknown`, `unavailable`, `account_mismatch`, `invalid_input`, `credentials_rejected` — and always mean "outcome unknown, retry with a new `Idempotency-Key` or read `get`", never "no document exists". `credentials_rejected` (HTTP 503) is szamlazz.hu answering 3/135/136/164 to any step: the worker's agent key is wrong, not the request; the attempt that raised it issued nothing (szamlazz.hu answers these codes before acting), but an earlier one may have landed, so it is a fault, never `rejected`.
_Avoid_: error/failure for a rejection or conflict, status

**Reissue**:
Issuing a new document of a kind after the one under its external id was reversed — by the service, the UI or anyone. Always explicit: `reissue: true` (with a new `Idempotency-Key`); without it the create returns `outcome: reversed`. Explicit because the service has no record of who reversed and cannot tell a stale retry of the original create from a deliberate new request; the flag on a live document is `conflict{live}`, so it can never cause a duplicate. The new document becomes the newest holder of the same external id.
_Avoid_: re-create, retry (a retry targets the same document)

**Reversal (as observed)**:
A document is reversed when szamlazz.hu reports `<sztornozott>true</sztornozott>` on it; the storno document carries `hivszamlaszam` = original and is the order-number hint only until something newer is issued under the order. The service does not track who reversed a document or when.
_Avoid_: reversal origin (v1 concept; gone), cancellation

**Foreign document**:
A live invoice-kind document (`SZ`, `ES`, `VS`) found under the order number by the order-number hint that is neither known to be ours nor the document seen under our external id. Detected live in the lookup step, on every kind but correctives; never adopted, nothing recorded; the create returns `conflict{foreign}`.
_Avoid_: external document, orphan

**Lookup step**:
The first of the two durable steps of issuing (`lookup-{kind}`): one read-only `ctx.run` that queries our external id and, for every kind but correctives, takes the order-number hint. It settles every case that needs no create — a live document of ours (`already_issued`, or `conflict{live}` with `reissue`), a reversed one (`reversed{storno_number}`, or proceed with `reissue`), an invalid holder (`conflict{external_id_collision}`), a foreign document (`conflict{foreign}`) — and otherwise hands the create step what it saw. Gateway fn: `Gateway::lookup`; outcome type `LookupOutcome`.
_Avoid_: pre-query (the create step has its own), attempt

**Create step**:
The second durable step of issuing (`create-{kind}`): one `ctx.run` under the issue policy whose every execution is query-first *inside the closure* — the external-id query first, the send only when nothing live of ours is there. A live document that is not the reversed one the lookup saw was issued by an earlier execution and is answered as `issued` without sending. A lost reply is re-queried once, immediately; nothing found makes the step **unconfirmed**, and Restate re-executes it after the policy's delay. The query lives inside the closure because a separate journaled pre-query would replay its stale "nothing" on the retry. Gateway fn: `Gateway::create` → `Result<CreateOutcome, Unconfirmed>`.
_Avoid_: attempt, issue step (the whole two-step sequence is "issuing"), send (one part of the step)

**Unconfirmed**:
The create step's one error: szamlazz.hu's answer is not known — a transport failure, an open code (1, 55, 56 without a number, `szlahu_down`) or a 71/152 with nothing under the order — after the immediate re-query found nothing. Retryable to the SDK, so the issue policy re-executes the step; its exhaustion is the `outcome_unknown` fault. Every *known* answer, including rejections and duplicates, is `Ok` data.
_Avoid_: unknown outcome (the fault is `outcome_unknown`; this is what precedes it), transport error (one cause of it)

**Issue policy**:
The run retry policy of the create step: `[issue]` in the deployment configuration — `max_attempts` executions, `initial_delay` growing by `factor` to `max_delay`, bounded by `max_duration` — mapped field for field onto `RunRetryPolicy::new()`. Deployment-level (on `WorkerConfig`), shapes no journal entry. Set explicitly because the SDK's default run policy sends no retry delay and the server would spend the handler's `invocation_retry_policy` instead. `max_duration` is the hard bound; the attempt count is not durable across every replay (ADR 0004). Type: `restate_szamlazz::config::IssueConfig`.
_Avoid_: attempt budget, backoff (the loop is gone), retry policy without qualification (the handlers have their own)

**Consumed proforma**:
A proforma that szamlazz.hu removed from its query surface because an invoice or prepayment converted it — explicitly by reference or implicitly by shared order number. Derived live in `get`: the proforma is absent under its external id while the invoice or prepayment carries `hivdijbekszam`, reported as `{state: consumed, by}`. Distinct from a deleted proforma, which is simply absent and may be recreated.
_Avoid_: converted (the invoice is converted from it; the proforma is consumed), deleted

**Gateway (`gateway` module) / `Szamlazz.Agent` service**:
The module that speaks to szamlazz.hu on behalf of one account: `restate_szamlazz::gateway::Gateway` owns the `szamlazz_agent::Client` and the account it is opened for, and exposes one plain async fn per durable step (`lookup`, `create`, `verify`, `query`, `hint`, `storno`, `delete_proforma`, `set_payments`) with outcome-as-data — every expected szamlazz.hu outcome is a value, never an `Err`; the create step's `Err(Unconfirmed)` is reserved for an answer that is not known. Not a second client — the Számla Agent `Client` is the transport it wraps. Every read of account configuration by the Restate services (ownership-validation pins, document defaults, seller block) goes through `Gateway::account()`; the services themselves hold only the gateway and the deployment-level `WorkerConfig` (namespace, issue policy). `Szamlazz.Order` calls the gateway inside `ctx.run`. The `Szamlazz.Agent` Restate service is a thin stateless facade over the same gateway for by-number operations (query, credit entries, storno of unmanaged documents). Neither Restate service calls the other.
_Avoid_: steps (for the module), client (for the module — it is not a second client), "the agent" without qualification, Invoice service
