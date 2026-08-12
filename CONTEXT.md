# szamlazz.hu Integration

Rust crates for integrating with szamlazz.hu, a Hungarian invoicing service: an outbound API client and two inbound receivers for szamlazz.hu-initiated pushes.

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
The reversal of an issued invoice. A distinct Agent operation, not an invoice flag.
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
