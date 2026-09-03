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
The Restate Virtual Object that owns every document issued for one order number, registered with Restate as `Szamlazz.Order`. Its key is the order number (rendelésszám) trimmed of leading/trailing whitespace, case preserved — exactly what szamlazz.hu matches on. Same-key handlers run one at a time, which is what serializes issuing per order. Crate: `restate-szamlazz`.
_Avoid_: order object, invoice workflow

**Ledger**:
The `Order`'s Virtual Object state: per-kind slots, corrective entries, the request-id map, a foreign hint and a bounded history. Holds numbers, ids, totals, an HMAC fingerprint and journaled timestamps — never buyer data. It is the source of truth for what the service issued; szamlazz.hu is consulted to verify it, not to rebuild it.
_Avoid_: cache (it is authoritative, not derived), database

**Slot**:
The ledger entry for one document kind of an order (`proforma`, `invoice`, `prepayment`, `final`). Exactly one slot per kind; its status moves through `pending`, `committed`, `rejected`, `blocked`, `reversed`, `reversal_unverified` (a service-side storno exhausted its attempts unconfirmed; the next storno retries), `vacant` (nothing of ours after a foreign detection or an operator `forget`), and for proformas `deleted` or `consumed`. A `pending` slot means "we may have issued something we have not yet confirmed" and is what makes killing a stuck invocation safe.

**Generation (gen)**:
The counter in a slot that increments only on a verified reversal (invoice kinds), an operator-recorded reversal, an operator `forget`, or deletion/consumption (proforma). Each generation is one document identity; the external id embeds it so a reissued invoice never shares an id with the stornoed one. Never bumps on transport errors, rejections, 71/152 or foreign detections.
_Avoid_: version, attempt, sequence number

**Request id**:
The caller-supplied `request_id` on every issuing handler: the retry identity. The same id returns the entry's current state forever; a different id is a new logical request; a known id with a different payload is `conflict{payload_mismatch}`. Ledger-only — never sent to szamlazz.hu.
_Avoid_: idempotency key (Restate's ingress `Idempotency-Key` is a different, retention-bound mechanism the service does not rely on), correlation id

**External id (szamlaKulsoAzon)**:
The Agent's optional per-document identifier, used by the worker as the identity handle: deterministic from ledger state (`{slug}:{order}:{kind}:{gen}`), written to state before the first call, queried before every create. Not unique server-side and never echoed in responses, so every document found by it is validated before adoption.
_Avoid_: idempotency key (szamlazz.hu does not treat it as one), external reference

**Outcome**:
A domain result returned as data (HTTP 200): `issued`, `already_issued`, `reconciled`, `reversed`, `rejected`, `conflict{reason}`. Errors (`TerminalError`) are reserved for faults — `outcome_unknown`, `unavailable`, `account_mismatch`, `invalid_input` — and always mean "outcome unknown, re-call with the same request id", never "no document exists".
_Avoid_: error/failure for a rejection or conflict, status

**Reissue**:
Issuing a new generation of a document kind after the recorded one was reversed. Explicit (`reissue: true` plus a new request id) when the reversal was external or operator-asserted; flag-free after a service-side storno or a proforma deletion.
_Avoid_: re-create, retry (a retry targets the same generation)

**Reversal origin**:
Who reversed a recorded document, as the ledger knows it: `service` (via `Szamlazz.Order.storno_invoice`), `external` (detected by verification — UI, support, another integration), `operator` (asserted through the private `record_reversal` handler). Decides whether the next create needs `reissue`.

**Foreign document**:
A live invoice-kind document found under the order number that the ledger does not own and no external id of ours resolves to. Recorded only as a hint (`foreign_hint`), never adopted; the create returns `conflict{foreign}`.
_Avoid_: external document (collides with reversal origin `external`), orphan

**Consumed proforma**:
A proforma that szamlazz.hu removed from its query surface because an invoice or prepayment converted it — explicitly by reference or implicitly by shared order number. Distinct from `deleted`: a consumed slot is terminal for the order; a deleted one may be recreated.
_Avoid_: converted (the invoice is converted from it; the proforma is consumed), deleted

**Steps (`steps` module) / `Szamlazz.Agent` service**:
The module `restate_szamlazz::steps` owns the `szamlazz_agent::Client` and the account config and exposes one plain async fn per durable step (`issue`, `verify`, `query`, `hint`, `storno`, `delete_proforma`, `set_payments`) with outcome-as-data — every expected szamlazz.hu outcome is a value, never an `Err`. `Szamlazz.Order` calls it inside `ctx.run`. The `Szamlazz.Agent` Restate service is a thin stateless facade over the same module for by-number operations (query, credit entries, storno of unmanaged documents). Neither Restate service calls the other.
_Avoid_: "the agent" without qualification, Invoice service, client (for the steps module — it is not a second client)
