# `Szamlazz.Order` keeps no state: szamlazz.hu is the source of truth, reached through deterministic external ids

The v1 design (ADRs 0002–0004 as first written) gave `Szamlazz.Order` a **ledger** in Virtual Object state:
one slot per document kind with a status machine (`pending`, `committed`, `rejected`, `blocked`, `reversed`,
`reversal_unverified`, `vacant`, `deleted`, `consumed`), a generation counter per slot embedded in the external
id (`{slug}:{order}:{kind}:{gen}`), a `request_id → entry` map with an HMAC payload fingerprint, corrective
entries with a per-order counter, a foreign hint and a bounded history. The ledger was written with `ctx.set`
before the first szamlazz.hu call and declared authoritative; szamlazz.hu was consulted to *verify* it on every
hit. Two operator handlers (`record_reversal`, `forget`, `ingress_private`) existed to repair it, and it carried
a schema version for migrations.

Building it showed that every question the ledger answered is already answered by Restate or by szamlazz.hu:

- **Serialization per order** — the Virtual Object's per-key lock: exclusive handlers on one key run one at a
  time (Restate; a paused or stuck invocation holds the key, ADR 0004).
- **Replay of completed steps inside an invocation** — the journal: a completed `ctx.run`, `ctx.sleep` or
  result replays after a crash; only the *open* closure re-executes (verified, ADR 0002).
- **A second live document of the same kind under one order number** — the account toggle "Rendelésszám
  ismétlődés tiltása" ON: different content → 71/152; a byte-identical resend while the first document is live
  → the same number, byte-identical response (verified). The replay compares the trimmed order number, the amount
  and the buyer name byte-exact, so the service normalizes the name once (trim + NFC) and serializes it
  identically on every attempt.
- **Whether a document is reversed** — `<sztornozott>true</sztornozott>` appears on the original after a storno,
  whoever performed it (UI, support, the service); the storno document carries `hivszamlaszam` = original and
  inherits the order number (verified). A repeat storno echoes the existing storno: storno is idempotent
  server-side (verified).
- **Which document an external id names** — a query by external id returns the **newest** holder (last-writer-
  wins, verified); a query by order number returns the newest document of any kind under it (verified).
- **Whether a proforma was consumed** — a converted proforma leaves the query surface (7 by number and by
  external id) while the converting invoice or prepayment carries `hivdijbekszam` (verified).

The one thing neither provides: **`ctx.run` is at-least-once across crashes.** A closure that crashes before its
result is journaled runs again, no sooner than the handler's `initial_interval` (verified: 20 s configured,
23.8 s observed, new process). The re-executed closure — and, after a kill or a client timeout, the *next
invocation* — must be able to find what a prior execution issued. That requires an identity known **before** the
send and computable by anyone holding the key, which the ledger provided by writing `pending` first. A
deterministic external id derived from the key alone provides it without state.

## Decision

`Szamlazz.Order` keeps **no state**. The Virtual Object exists for its per-key lock. Everything else is answered
by querying szamlazz.hu through external ids that are deterministic from the key:

- slot kinds: `"{namespace}:{order}:{kind}"`, `kind ∈ proforma | invoice | prepayment | final`
  (`ExternalId::for_kind`);
- correctives: `"{namespace}:{order}:corrective:{correction_id}"` — the caller names each corrective; a new
  `correction_id` issues a new corrective by contract, the same id finds the one it issued
  (`ExternalId::for_corrective`);
- storno: `"{namespace}:{order}:storno:{original_number}"` (`ExternalId::for_storno`), and
  `"{namespace}:by-number:{number}:storno"` for `Szamlazz.Agent.storno` of a document no `Order` manages.

**Last-writer-wins replaces the generation counter.** The question a create asks is "what is the newest document
of this kind we issued for this order?", which is exactly what the external-id query answers. After a storno the
reissued document becomes the newest holder of the same id; the stornoed original stays reachable by its number
and through the storno's `hivszamlaszam`. Nothing needs a suffix.

**Every `Found` document is validated before it is trusted**: `rendelesszam == order ∧ tipus ∈ kind-set ∧
teszt == account.mode ∧ (account.supplier_id unset ∨ szallito/id == supplier_id)`; anything else is
`conflict{external_id_collision}` (`InvoiceDocumentExt::is_ours`). External ids are not unique server-side (verified),
so this is the only protection against adopting a stranger's document.

**Retry identity is Restate's ingress `Idempotency-Key`**, supplied by the caller. The service does not know
whether one was used and never relies on it for safety: the external-id query inside the create step — the first
line of the closure on every execution — is the guard, the key is deduplication.

**`reissue: true` is required after *any* reversal.** A create that finds its document reversed returns
`outcome: reversed`; with `reissue: true` it proceeds, and on a live document it is `conflict{live}`. There is no
"the service stornoed this, so the next create is flag-free" path, because there is no record of who stornoed.

## Considered options

- **Keep the ledger.** Rejected. It was a second source of truth that had to be verified against the first on
  every hit anyway (verify-on-hit, verify TTL); every state it held was either derivable live or existed to
  describe the ledger's own uncertainty (`pending`, `blocked`, `reversal_unverified`, `vacant`). It cost the
  ledger module and everything built on it — roughly 4,400 net lines across the crate, ~2,700 of them the module
  and its tests — plus two operator handlers, an HMAC secret to deploy and rotate, a learned account fingerprint,
  and schema versioning with state migrations on every shape change.
- **`ctx.rand_uuid()` as the external id.** Rejected. Deterministic only within one invocation's journal. A
  caller retry after a kill or a client timeout is a *new* invocation with a new id and cannot answer "is there
  already one?" — the exact question the pre-query must answer.
- **A caller-supplied external id.** Rejected. Safety becomes opt-in: a caller that omits or rotates the id
  re-opens the duplicate window. And szamlazz.hu never echoes `szamlaKulsoAzon` — not in create responses, not
  in the query XML (verified) — so the caller can observe it nowhere; it has no benefit over the order number and
  the returned invoice number, which the caller already has.
- **A tiny "we stornoed this" state to keep flag-free reissue after a service-side storno.** Rejected. Such a
  marker cannot distinguish a *stale retry* of the original create — which arrives after the storno and must not
  issue (an identical resend after a storno issues a **new** invoice, verified) — from a *new* deliberate
  request. The ledger told them apart with `request_id`; without it the marker authorizes both. One uniform rule
  costs one boolean on one call per reversal and can never cause a duplicate.

## Consequences

- **Given up, deliberately** (design §12): the `request_id` retry identity (→ `Idempotency-Key`);
  `conflict{payload_mismatch}` (a different payload for a live document is `already_issued`); flag-free reissue
  after a service-side storno (→ `reissue: true` after any reversal); `recorded_document_missing` (a document
  szamlazz.hu no longer knows is simply absent — live accounts cannot delete invoices); the `payments_before`
  capture on storno (query before stornoing — the server erases `<kifizetesek>` on the original); the ledger
  snapshot (`get` is four live queries and can return `unavailable`); the operator handlers `record_reversal` /
  `forget` (nothing to repair); the account fingerprint learned into state (pin `supplier_id` in config); schema
  versioning and state migrations.
- **Gained**: nothing to migrate, repair or drift; `get` is never stale; a UI storno, a support storno and a
  service storno are one case (`sztornozott`); a kill has nothing to compensate; a reset Restate cluster loses
  only in-flight invocations; the crate is a fraction of its former size.
- **Caller contract** (design §8, in the crate READMEs): (1) send an `Idempotency-Key` per logical request;
  (2) any error from an issuing or storno handler means "outcome unknown — retry with a **new** key" (Restate
  replays a failed invocation's stored completion for `idempotency_retention`, verified) — the retry reconciles by
  external id and is safe; never read an error as "no document exists"; (3) after any reversal a create returns
  `reversed`; send `reissue: true` with a new key when a new invoice is wanted.
- **Still required**: the toggle ON (the server-side guard against a second live document of the same kind);
  the byte-stable buyer name (the replay guard); the 2-minute gap before a re-check — the handlers'
  `initial_interval` for a crash, the issue policy's `initial_delay` for a lost reply — which must wait out a
  client timeout plus an observed ≥ 57 s server stall before the create step's leading query runs again; the
  cross-kind exclusivity check (`conflict{prepaid_chain}`) and the proforma-link check (`conflict{proforma_live}`),
  which the server does not perform.
- **Foreign documents** are detected live through the order-number hint in the lookup step, on every kind but
  correctives (a live `SZ | ES | VS` that is neither ours nor the document seen under our id →
  `conflict{foreign}`); nothing is recorded.
- **Consumed proformas** are derived live in `get`: proforma absent under its id while the invoice or prepayment
  carries `hivdijbekszam` → `{state: consumed, by}`.
- `CorrectionId` (`^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`) replaces the corrective counter and the
  `request_id ↔ cseq` map; it is the caller's per-corrective identity and part of the external id.
- ADR 0002's `{gen}` suffix, `request_id` and "written to state before the first call", ADR 0003's `request_id`
  and flag-free service-side reissue, and ADR 0004's `pending` slot, operator runbook and
  `idempotency_retention = 7d` (now `30d`) are superseded; the rest of each still holds.
