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
The Agent's optional per-document identifier, used by the worker as the identity handle. Deterministic from the key alone — `{slug}:{order}:{kind}` for the four kinds, `{slug}:{order}:corrective:{correction_id}`, `{slug}:{order}:storno:{number}` — so any invocation can find what an earlier one issued. Not unique server-side and never echoed in responses: a query returns the newest holder, and every document found by it is validated (order number, `tipus`, `teszt`, supplier pin) before it is trusted.
_Avoid_: idempotency key (szamlazz.hu does not treat it as one), generation (no counter — the newest holder is the answer), external reference

**Idempotency-Key**:
Restate's ingress retry identity, sent by the caller as a header per logical request; Restate dedupes retries and attaches concurrent duplicates to the in-flight invocation. Recommended, never relied on for safety. Rotate it after a terminal error: Restate replays a failed invocation's stored completion for the retention period (verified), so the same key would repeat the failure instead of reconciling.
_Avoid_: request id (v1's body field; gone), correlation id

**Outcome**:
A domain result returned as data (HTTP 200): `issued`, `already_issued`, `reconciled`, `reversed`, `rejected`, `conflict{reason}`. Errors (`TerminalError`) are reserved for faults — `outcome_unknown`, `unavailable`, `account_mismatch`, `invalid_input` — and always mean "outcome unknown, retry with a new `Idempotency-Key` or read `get`", never "no document exists".
_Avoid_: error/failure for a rejection or conflict, status

**Reissue**:
Issuing a new document of a kind after the one under its external id was reversed — by the service, the UI or anyone. Always explicit: `reissue: true` (with a new `Idempotency-Key`); without it the create returns `outcome: reversed`. Explicit because the service has no record of who reversed and cannot tell a stale retry of the original create from a deliberate new request; the flag on a live document is `conflict{live}`, so it can never cause a duplicate. The new document becomes the newest holder of the same external id.
_Avoid_: re-create, retry (a retry targets the same document)

**Reversal (as observed)**:
A document is reversed when szamlazz.hu reports `<sztornozott>true</sztornozott>` on it; the storno document carries `hivszamlaszam` = original and is the order-number hint only until something newer is issued under the order. The service does not track who reversed a document or when.
_Avoid_: reversal origin (v1 concept; gone), cancellation

**Foreign document**:
A live invoice-kind document (`SZ`, `ES`, `VS`) found under the order number by the order-number hint that is neither known to be ours nor the document seen under our external id. Detected live on the first attempt; never adopted, nothing recorded; the create returns `conflict{foreign}`.
_Avoid_: external document, orphan

**Consumed proforma**:
A proforma that szamlazz.hu removed from its query surface because an invoice or prepayment converted it — explicitly by reference or implicitly by shared order number. Derived live in `get`: the proforma is absent under its external id while the invoice or prepayment carries `hivdijbekszam`, reported as `{state: consumed, by}`. Distinct from a deleted proforma, which is simply absent and may be recreated.
_Avoid_: converted (the invoice is converted from it; the proforma is consumed), deleted

**Steps (`steps` module) / `Szamlazz.Agent` service**:
The module `restate_szamlazz::steps` owns the `szamlazz_agent::Client` and the account config and exposes one plain async fn per durable step (`issue`, `verify`, `query`, `hint`, `storno`, `delete_proforma`, `set_payments`) with outcome-as-data — every expected szamlazz.hu outcome is a value, never an `Err`. `Szamlazz.Order` calls it inside `ctx.run`. The `Szamlazz.Agent` Restate service is a thin stateless facade over the same module for by-number operations (query, credit entries, storno of unmanaged documents). Neither Restate service calls the other.
_Avoid_: "the agent" without qualification, Invoice service, client (for the steps module — it is not a second client)
