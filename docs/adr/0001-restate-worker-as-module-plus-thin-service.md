# The low-level szamlazz.hu layer is a Rust module inside `Order`, plus a thin `SzamlaAgent` service

`restate-szamlazz` exposes the basic Számla Agent operations as Restate services. The layer that owns
the `szamlazz_agent::Client` and the credentials had to live somewhere: either as a Restate service
that `Order` invokes, or as code that `Order` runs itself. We chose the latter.

`restate_szamlazz::szamla_agent::SzamlaAgent` is a plain Rust module: a struct holding the client
and the account config, with async functions `issue`, `storno`, `delete_proforma`, `set_payments`
and `query`. It has no Restate context, returns every expected szamlazz.hu outcome as data (never
`Err` for a rejection, a duplicate or a "not found"), and is unit-testable with wiremock. The
`Order` Virtual Object (key = order number) calls these functions **inside its own `ctx.run`
closures**. A thin, stateless `SzamlaAgent` Restate service exposes `query`, `set_payments` and
`storno` for by-number operations over the same module instance. No Restate service calls another,
and `Order` never invokes a handler on its own key. The dependency direction the owner asked for —
`Order` depends on the low-level layer, nothing depends on `Order` — is a compile-time fact:
`Order → szamla_agent ← SzamlaAgent`.

The crate pair mirrors email-rs: `restate-szamlazz` is the library (contract types, config, ledger,
the module, both services; `contract`/`config`/`ledger` compile without `restate-sdk`), and
`restate-szamlazz-endpoint` is the binary `restate-szamlazz` that hosts the services over HTTP and
ships as `ghcr.io/sagikazarmark/restate-szamlazz`.

## Considered options

- **An `Invoice`/`Issuer` Restate service called by `Order` via `ctx.service_client()`.** Rejected.
  Every cost it adds is one the module does not have: the child's *default* retry policy (500 ms
  initial interval, 70 attempts, pause) re-opens the crash window that `Order`'s 2-minute
  `initial_interval` closes — a child closure re-executed within a second of a crash queries
  szamlazz.hu before the first request has resolved server-side; the attempt bound becomes
  invocation attempts × child attempts (350 sends under defaults); a *paused* child holds the
  parent's key indefinitely; the buyer PII is journaled three times (Order input, Order's `Call`
  entry, child input); two timeout pairs must stay consistent; and `issue`/`delete_proforma` need
  `ingress_private` anyway because raw issuing must not bypass the ledger — at which point a private
  handler differs from a module function only by a second journal and a retry policy. The usual
  reasons for a separate service (independent scaling, per-handler OpenAPI for the raw operations,
  a second Restate caller) do not exist at v1; the first would be a non-goal and the second is
  undesirable.
- **`Order` does everything and a public `Invoice.storno` delegates upward into
  `Order.storno_invoice`.** Rejected. An upward edge into a Virtual Object is a deadlock class: an
  exclusive handler that `.call()`s an exclusive handler on the same VO key never completes
  (verified — parent `running`, child `pending` forever; killing the parent killed both). It also
  inverts the intended dependency direction. The module design is this option with the seam made a
  Rust boundary and the upward call removed.

## Consequences

- Every szamlazz.hu call happens inside an `Order` (or `SzamlaAgent`) `ctx.run`. Issuing, storno
  and delete runs use `RunRetryPolicy::max_attempts(1)` with outcome-as-data; pure queries use the
  default exponential run retry with `max_duration 2m`. A run failure surfaces as a terminal error
  (HTTP 500 to a synchronous caller — verified), which is why the module must never return `Err`
  for an expected outcome.
- A process crash re-executes only the *open* closure; completed runs, sets and sleeps replay from
  the journal. Worst case per episode in a pathological crash loop is loop attempts + (invocation
  attempts − 1) = 9 closure executions, each query-first (ADR 0002), finite because of `kill`
  (ADR 0004).
- `SzamlaAgent.storno` on a document that carries `rendelesszam` returns
  `outcome: managed_by_order{key}` — a convention on the key scheme, never a call into `Order`.
  Documents without an order number (no `Order` exists for them) are reversed directly.
- `issue` and `delete_proforma` exist only as module functions. When a second Restate caller
  appears, the upgrade path is an `ingress_private` handler over the module; the module boundary is
  the seam either way.
- The buyer input is journaled once, in `Order`'s own journal (`journal_retention = 3d`). The
  ledger holds numbers, ids, totals, an HMAC fingerprint and journaled timestamps — no PII.
- Rule, stated so a future `storno_invoice → create_invoice` convenience is not added: no `Order`
  handler ever `.call()`s an exclusive handler on its own key. Under this layering it is structural.
- The service name is `SzamlaAgent` (the glossary term for the surface it wraps). `Invoice`,
  `Documents`, `Agent` and `Szamlazz` were rejected as names for a layer that issues `D/SZ/ES/VS/HS`,
  deletes `D` and registers credit entries.
