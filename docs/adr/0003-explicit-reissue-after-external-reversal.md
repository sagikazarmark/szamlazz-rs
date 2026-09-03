# A repeat create after an external reversal returns `reversed` and reissues only on explicit request

`create_invoice` (and its proforma, prepayment, final and corrective siblings) may find that the
document the ledger recorded for this order and kind has since been reversed by someone other than
the service — a storno from the szamlazz.hu UI, by support, or asserted by an operator. The question
was whether the repeat call should issue a replacement.

It does not, by default. Verification detects the reversal (`sztornozott == Some(true)` on the
recorded number — verified), the slot becomes `reversed{origin: external}`, `gen` is bumped, and the
handler returns `outcome: reversed{number, storno_number?}` — data, not an error. A new document is
issued only when the incoming request carries **`reissue: true` and a new `request_id`**
(`reissue` with a known id is `invalid_input`). After a **service-side** storno
(`Order.storno_invoice`) the slot is `reversed{origin: service}` and open flag-free: the ledger knows
the reversal was deliberate. A deleted proforma is likewise open flag-free — it is not a legal
document. The same rule holds inside the attempt loop: a `Found` document that is reversed returns
`reversed`; the loop never re-allocates.

## Why the default is "tell, don't act"

The two failure classes are asymmetric. An unwanted invoice is a numbered legal document, e-mailed
to the buyer by default (`sendEmail` defaults to true), reported to NAV, and undoable only by another
storno — the vendor itself calls a reissue after storno an accountant-visible event. A missed reissue
costs one extra call. Identity, not a flag, tells a retry from a rival caller or a stale job (the
same `request_id` returns `reversed` forever); the flag tells a deliberate re-invoice from a fresh
UUID minted by a framework. With both, misuse degrades to one `reversed` answer, never to a document.

Verified practice facts that bear on the decision:

1. Buyer/partner data (name, address, tax number), cash-accounting nature and currency **cannot** be
   fixed by a corrective invoice (szamlazz.hu knowledge base). For the commonest webshop error —
   wrong billing data — storno followed by a new invoice is the vendor's documented path, and the
   reversed invoice's order number is reusable (verified: a new invoice was issued under the same
   order after the first was stornoed). Reissue is normal, so it must be *possible* with one extra
   boolean, not *automatic*.
2. Once a corrective exists, storno is impossible (knowledge base; verified code 221), and a
   corrective netting to zero does **not** free the order number (verified: the next invoice under
   that order is 152). Such an order's invoice slot is terminally occupied; `correct_invoice` is the
   only mutation left and recovery needs a new order number.
3. An identical resend after a storno issues a **new** invoice (verified; real-issue latency, new
   document id). The server's replay never returns the stornoed one, so replay protection ends at
   storno and only the ledger can stop a stale retry.
4. Restoring a mistaken storno by reissuing with identical content is a documented flow. Any rule of
   the form "identical means bug" is wrong.
5. Credit entries do not carry over, and the server erases `<kifizetesek>` from the original on
   storno (verified). The service snapshots them into history before a storno; the caller
   re-registers on the new invoice via `SzamlaAgent.set_payments`.
6. The new invoice carries the true, caller-supplied fulfillment date; the service never defaults
   it on reissue.

Scenarios (✔ correct · ◐ one more call, nothing issued · ✘L unwanted legal document · ✘B blocks a
legitimate action):

| Scenario | Silent | Knob | `request_id` only | Fingerprint | Chosen |
|---|---|---|---|---|---|
| Retry after timeout; document live | ✔ | ✔ | ✔ | ✔ | ✔ |
| Retry lands after a UI storno (order cancelled) | ✘L | ✘L / ✔ | ✔ if id persisted, ✘L if minted per send | ✔ if byte-identical, ✘L if dates recomputed | ✔ |
| UI storno (wrong buyer data); webshop re-sends corrected | ✔ | ✔ / ✘B | ✔ | ✔ | ◐ |
| Stale job re-sends the original after a UI storno | ✘L | ✘L / ✔ | ✔ / ✘L | ✔ / ✘L | ✔ |
| Service-side storno, then create | ✔ | ✔ / ✘B | ✔ | ✔ / ✘B if identical | ✔ |
| Second caller, different content, after a UI storno | ✘L | ✘L / ✔ | ✘L-prone | content-dependent | ✔ |
| Stale retry after a service-side storno | ✘L | ✘L / ✔ | ✔ | ✔ / ✘L | ✔ |
| Mistaken storno; restore with identical content | ✔ | ✔ / ✘B | ✔ | ✘B | ◐ |
| Proforma deleted in the UI; create again | ✔ | ✔ / ✘B | ✔ | ✔ | ✔ |

The chosen rule fails nothing with the expensive error; its costs are one required field and one
flag in the two ◐ rows.

## Considered options

- **Silent reissue.** Rejected: every stale or rival request that arrives after an external storno
  produces an unwanted legal document.
- **A deployment knob (`auto | conflict`).** Rejected: `auto` is silent reissue; `conflict` blocks
  the vendor's own correction path, the service-side storno-then-create flow and the mistaken-storno
  restore, with no per-call escape.
- **`request_id` alone (new id after a reversal ⇒ issue).** Rejected: its only failure is silent.
  Frameworks that mint a UUID per send turn every retry into a "business decision", and a second
  caller is indistinguishable from a deliberate reissue.
- **A fingerprint heuristic (identical ⇒ conflict, different ⇒ issue).** Rejected: it blocks the
  documented identical-content restore and issues on a drifted retry (recomputed dates, different
  whitespace) after a UI storno. The fingerprint is kept for *mismatch detection*
  (`conflict{payload_mismatch}`), never for *authorizing* a reissue.
- **Runner-up: the flag always required, including after a service-side storno.** Not adopted: no
  safety difference given `request_id`; pure ergonomics, and the ledger already knows the
  service-side reversal was deliberate.

## Consequences

- `CreateRequest` carries a required `request_id` and `options.reissue`. `outcome: reversed` is an
  HTTP 200 response; callers must branch on it, not on status codes.
- `reissue: true` against a *live* recorded document is `conflict{live}`; against a known
  `request_id` it is `invalid_input`. Operator-asserted reversals (`record_reversal`,
  `origin: operator`) require the flag like external ones.
- A reissued invoice is a new generation with its own external id; the stornoed original stays
  reachable by number and under the old id (ADR 0002). The history event keeps `payments_before`.
- Because the identical-request replay never returns a stornoed document, the guard "never report
  `issued` for a number the ledger holds as reversed" cannot fire on observed behavior; it stays
  as a zero-cost assertion, not a documented outcome.
- After a reissue, `query --order` returns the newer invoice, not the storno document. Reversal is
  therefore always detected by number (`sztornozott`), with the order-number hint as a secondary
  signal for cold ledgers only.
- A corrective on the recorded invoice makes reissue impossible on that order regardless of flags;
  the ledger refuses `storno_invoice` (`rejected{has_corrective}`) before the server would (221).
