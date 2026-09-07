# `Order` is keyed by the order number and identifies documents by a deterministic external id

Status: partially superseded by [ADR 0005](0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md);
amended by [ADR 0006](0006-account-selection-via-restate-scopes.md) (the account namespace, below) and by #40
(the caller trims the key: a Virtual Object key with leading or trailing whitespace is refused as `invalid_input`
rather than trimmed by the handler, because Restate's per-key lock is on the raw key and a padded key would be a
second instance of the same order with its own lock and the same external ids; the `OrderKey` type itself still
trims) and by #64 (the key's alphabet is tightened to what this record already claimed and the code did not enforce —
no internal whitespace of *any* kind, where the code refused only runs — plus no `:` and Unicode NFC, and its length
is 40 bytes, not 64; the composed external id is bounded at 110 bytes by bounding its parts; the caller's
`invoice_number` is bounded the same way; see "Bounded inputs" below).
Still holds: the key rule (trimmed, case preserved, validated), the deterministic external id, the query-first
create inside a single `ctx.run` — now the create *step* under the issue policy's run retry policy (ADR 0004,
amended by #22) rather than one `max_attempts(1)` run per attempt — the `Found`-validation rule, the 2 m
`initial_interval`, and the toggle precondition. Superseded: the `{gen}` suffix (the newest holder under
`{namespace}:{order}:{kind}` is the answer; no counter), the `cseq` corrective counter (→ caller-supplied
`correction_id`), `request_id` as retry identity (→ Restate's ingress `Idempotency-Key`), "written to state
before the first call" (no state; the external id is derived from the key), and "one szamlazz.hu account per
deployment" (→ the Restate scope selects the account, ADR 0006; the slug is the deployment's namespace).

Issuing through the Számla Agent is at-least-once at every layer: the HTTP client can time out
after the server has issued, and a `ctx.run` closure that crashes before its result is journaled is
executed again. The design has to guarantee one legal document per order and kind anyway, and it has
to do so without a server-side idempotency key (the Agent has none for invoices).

`Order` is a Virtual Object keyed by the **order number** (`rendelésszám`), trimmed of leading and
trailing whitespace, case preserved. This matches the server: the create path trims before both the
replay match and the duplicate check, while case is significant (two invoices coexist under
`PRB-C-Case` and `prb-c-case`), and the query path matches exactly (a padded order number is
creatable but not queryable) — all verified. The key is validated (1–40 bytes after trim, no control
characters, no internal whitespace of any kind, no `:`, Unicode NFC → `invalid_input`; #64); nothing is
case-folded or NFC-normalized because the server does neither — a key outside the alphabet is refused,
never rewritten. The key carries no account namespace *because the Restate scope does*
(ADR 0006; first written as "one szamlazz.hu account per deployment"): Restate namespaces the Virtual
Object key and the `Idempotency-Key` per scope, so the same order number under two scopes is two `Order`
instances; the deployment's namespace lives in the external id and is shared by every account.

A document's identity is a deterministic `szamlaKulsoAzon` (external id) derived from ledger state:
`{slug}:{order}:{kind}:{gen}` for the slot kinds (`proforma | invoice | prepayment | final`),
`{slug}:{order}:corrective:{cseq}` for correctives, `{external_id_of_original}:storno` for a storno.
It is written to state — a `pending` slot plus `requests[request_id]`, via `ctx.set` — **before the
first szamlazz.hu call**. The create closure is *query by external id, then create* inside **one**
`ctx.run` with `RunRetryPolicy::max_attempts(1)`: `Found` ⇒ reconcile, code 7 ⇒ create with that
external id, 71/152 after the create ⇒ re-query by external id. The invocation retry policy sets
`initial_interval = 2m`. The caller supplies a `request_id` on every issuing handler; it is the
*retry identity* (same id ⇒ the entry's current state, forever; different id ⇒ a new logical
request) and stays ledger-only. Precondition: the account toggle **"Disable order number
repetition" is ON**; it is the second guard, not the first.

## The crash window, and what closes it

- The create step is one `ctx.run` whose closure begins with the external-id query and ends with
  the send (or the re-query after a lost reply). A crash mid-closure leaves no journal entry;
  Restate re-dispatches the invocation as an ordinary retryable failure and the closure runs again —
  no sooner than the handler's `initial_interval` (verified: 20 s configured, 23.8 s observed, new
  process). A *lost reply* (transport failure, an open code) is not a crash: the closure re-queries
  once and, finding nothing, returns a retryable error, and the run retry policy — the issue policy,
  `initial_delay 2m` — re-executes the whole handler after the delay; the journal replays to the
  create step and the closure begins again with the query. Either way the re-executed closure starts
  with the external-id query, so a request that landed is `Found`, not re-issued. The query must be
  *inside* the closure: a separate journaled pre-query would replay its stale "nothing" (ADR 0004).
- Read-your-writes lag by external id is ≈ 0 (verified: hit 771 ms after the create returned, and
  at +2, +10, +60 s). The 2-minute gap — the handler's `initial_interval` and the issue policy's
  `initial_delay` alike — is therefore not for lag but for a first request that is still in flight
  server-side: one create stalled ≥ 57 s with no response against a 60 s client timeout. The gap
  must exceed timeout plus stall; 2 min keeps a margin — never below ~90 s. In code since #61:
  `WorkerConfig::validate` floors `issue.initial_delay` at the client's exported `REQUEST_TIMEOUT`
  plus a 30 s margin, and every write handler's `initial_interval` is pinned at 2 m by the discovery
  test (ADR 0004, #61 amendment).
- External ids are **not unique** server-side (two invoices under different orders with the same id
  were both issued, no warning) and a query by a shared id returns the newest holder (last-writer-
  wins) — verified. Every `Found` document is therefore validated before adoption:
  `rendelesszam == order ∧ tipus ∈ kind-set ∧ teszt == account.mode ∧ szallito/id == supplier_id`;
  anything else is `conflict{external_id_collision}`. For the same reason a generation's id is
  never reused: the gen-0 id on gen 1 would hide the stornoed original behind the newer document.
- An external id attaches only on the call that *creates* the document; a replayed create, a
  repeat storno with a new id and a whitespace-padded replay all stored nothing (verified). The
  ledger is the only readable id → number mapping (query responses never echo the external id), and
  a first send that lands as a replay of a pre-existing identical document is findable only through
  the order-number hint.
- The second guard is the server's identical-request replay under the toggle: a byte-identical
  resend while the first document is live returns the same number. Its fingerprint is the order
  number, the amount and the buyer name **byte-exact** — not `keltDatum`, not the external id, not
  the comment (verified). Consequences: the buyer name is normalized once at validation and
  serialized identically on every attempt; the ledger fingerprint includes dates only when the
  caller supplied them; pinning `issue_date` buys nothing. The guard ends at storno: an identical
  resend after a storno issues a **new** invoice (verified), so after a reversal the external-id
  pre-query is the only guard and a `Found` document with `sztornozott` must return
  `outcome: reversed` instead of re-allocating (ADR 0003).

## Considered options

- **A stateless service relying on Restate's ingress `Idempotency-Key`.** Rejected: it is opt-in
  per caller, bounded by `idempotency_retention` (a retry after the window creates a new
  invocation), and it replays a stored *terminal failure* for the whole retention period (verified:
  second call with the same key → same invocation id, identical 500 body, no re-execution). A caller
  retrying `outcome_unknown` would keep hearing "unknown" while the document exists. Without state
  there is also nothing to reconcile against.
- **`ctx.rand_uuid()` as the external id.** Rejected: deterministic only within one invocation's
  journal. A caller retry after a kill or timeout is a new invocation with a new id and cannot find
  the first invocation's document; a lost ledger cannot be rebuilt.
- **`{kind}:{seq}` ids (a per-order attempt counter).** Rejected: not recoverable after ledger
  loss. With `seq` restarted at 1, the first probe returns 7, the order-number hint shows a live
  invoice that is not in the (empty) ledger, and the service records our own document as foreign
  and answers `conflict` — converging only after `seq_original` calls, each a wrong conflict with a
  mislabeled ledger. `{kind}:{gen}` (gen = verified reversals or deletions of that kind) recovers:
  gen 0 found live ⇒ validate, adopt ⇒ `already_issued`/`payload_mismatch`; found reversed ⇒ gen 1;
  7 ⇒ issue at that gen. `{kind}:{request_id}` was rejected for slot kinds because cold recovery
  would depend on the caller resending the same id; correctives, where several documents per base
  are legitimate, use a per-order counter (`cseq`) with the `request_id ↔ cseq` map in the ledger.
- **Reconciling by order number only.** Rejected: identity-blind. `query --order` returns the most
  recently issued document *of any kind* carrying that order number (a corrective after six kinds;
  the storno after a storno; the reissued invoice after storno + reissue — verified), so it cannot
  tell our gen-0 document from a foreign one or a later kind, and loses a stornoed original as soon
  as anything newer exists. Kept as a secondary hint: foreign detection, proforma consumption, storno
  discovery on a cold ledger.

## Consequences

- The toggle is a documented precondition the service cannot detect. With it OFF the server neither
  replays an identical resend nor answers a drifted one with 71/152, so the second guard is gone and
  the external-id pre-query stands alone; running without the toggle is unsupported.
- `gen` bumps only on a *verified* reversal (invoice kinds) or deletion/consumption (proforma) —
  never on transport errors, rejections, 71/152 or foreign detections: nothing of ours was created,
  or if it was we want to find it under the same id.
- `request_id` matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$` and is unique per order across kinds
  (`conflict{request_id_reused}`); a known id with a different fingerprint is
  `conflict{payload_mismatch}`; `reissue: true` with a known id is `invalid_input`. Callers should
  not rely on the ingress `Idempotency-Key`; `request_id` is the retry identity.
- Same-key exclusivity serializes every create for one order, so each generation has exactly one
  creating call — the property that makes cold-ledger recovery by `{kind}:{gen}` work.
- `supplier_id` is `szallito/id` from the first query response (the `szlahu_id` header is the
  document id); the account's `mode` is validated against `<teszt>` on every adopted document. *Amended
  (ADR 0006):* both pins are read from the invocation's journaled `Account`; `supplier_id` is required in
  the multi-account shape.
- External ids of ~110 characters with `: . _ -` are accepted and queryable (verified), so the namespace
  (1–16 chars `[a-z0-9-]`; then called the slug) plus order, kind and gen fit without hashing. *Amended
  (#64):* 110 is also the **bound** — `ExternalId::MAX_LEN` — because szamlazz.hu documents no limit and a
  truncated id would make every leading query by the full id answer 7 and a lost reply re-send.
- Two order numbers differing only in case are two VO keys and two server documents. Internal
  whitespace and control characters are rejected rather than collapsed, because the server's
  handling of them is untested.

## Bounded inputs (#64)

The 2026-09-06 review found four caller inputs the worker later relied on without a bound (J5, J8, J11, A2,
J-07-12). The decisions:

- **The `OrderKey` alphabet is what this record claimed.** The code admitted a single internal space, an NBSP,
  `:` and non-NFC text while the text above said internal whitespace is rejected. The code now matches the
  text, and the text is made precise: after trimming, 1–40 bytes, no control character, **no whitespace of any
  kind** (`char::is_whitespace`, so a space and an NBSP alike), **no `:`** (the external-id separator — `ORD:1`
  plus a kind would otherwise compose an id another order's composition reads as), and **Unicode NFC**
  (refused, not normalised). The alternative — amending the text to the lenient code — was rejected: the
  server's handling of internal whitespace and NFC is unverified (behaviour notes, "Still unverified"), and a
  `rendelesszam` the server stored differently from the key would fail `carries_order` on the worker's own
  document and strand the order behind a permanent `conflict{external_id_collision}`. Refusing is always
  safe; a later probe (#15 probe 4) may relax the rule, never the other way round. Nothing is trimmed,
  collapsed or normalised on the caller's behalf. The `OrderKey` type still trims its edges (its `FromStr`,
  `TryFrom<String>` and serde are the lenient entry for a caller building keys from its own order numbers);
  the handler refuses an untrimmed *raw* key as before (#40).
- **A `correction_id` is never an external-id token.** `invoice`, `proforma`, `prepayment`, `final`,
  `corrective`, `storno`, `by-number`, `check-account` — in any letter case — are refused
  (`InvalidCorrectionId::Reserved`). With `:` out of the key this was already impossible to exploit; the rule
  is belt and braces, and it pins the token set (`ExternalId::TOKENS`) in one place.
- **The composed external id is bounded at 110 bytes by bounding its parts**, not by checking the
  composition: `Namespace` 16, `OrderKey` 40, `CorrectionId` 40, the caller's `InvoiceNumber` 40. The
  longest shape, `{namespace}:{order}:corrective:{correction_id}`, is 16 + 1 + 40 + 1 + 10 + 1 + 40 = 109;
  `const` assertions in `identity.rs` prove every shape at compile time, so raising any part's bound fails
  the build until the budget is re-balanced. Bounding the parts was preferred to refusing at construction
  because it gives the caller a rule per field rather than a rule about a combination, and to hashing
  because a hashed id is not derivable by eye from the key. 40 was chosen for all three because a dashed UUID
  (36) fits each — the Pretix-shaped `{event}-{code}` key and a UUID `correction_id` are the expected
  callers — and one number is easier to remember than three. The real server limit is unknown (#15 probe 5);
  110 is what was verified.
- **The caller's `invoice_number` is a contract type**, `contract::InvoiceNumber`: 1–40 bytes, no whitespace,
  no control character, no `:`, refused by its `Deserialize` so a bad number is a *Malformed body*
  (`invalid_input` before the prologue). It flows into step names (`verify-{number}`, `storno-{number}`) and
  into the storno external ids, so it is bounded like the other segments. Nothing is trimmed: a padded number
  would be answered 7 by szamlazz.hu, and the rule is the better diagnosis. szamlazz.hu's own numbers are far
  inside the bound; NAV allows 50 characters, so 40 is a bet that no real prefix approaches it — raise it,
  and the storno shapes, if it does.
- **Arithmetic on caller input is checked, never panicking** (J8). The `LineItem::try_calculated` path of #60
  already answers an overflowing `unit_price × quantity` as `invalid_input`; #64 makes the two remaining
  `Decimal` sums in the worker checked too — `outstanding` (on szamlazz.hu's amounts, on the response path of
  `create_*` and `query`) and `gross_total` (a public helper with no production caller today; checked so that
  a future caller cannot reintroduce the panic). The check runs *after* the prologue — it needs the account's
  currency defaults for the rounding — so `namespace` and `account` are journaled before the 400; a duplicate
  pre-prologue check was rejected as a second site for one rule. The e2e scenario proves the property that
  matters: an overflowing body sent beside a healthy create against the same endpoint issues nothing, exactly
  one create reaches the wire, and the healthy create is `issued` without a retry.
- **Retroactivity.** A tighter key hides nothing issued so far: no deployment of the worker has issued against
  a production account yet (#15 is the go-live checklist), and the test-account documents issued during the
  probes were under keys inside the new alphabet. Had there been documents under a key the new rule refuses
  (a 41–64-byte key, a key with an internal space), they would be reachable only by `Szamlazz.Agent.query`
  by number or order number — the `Order` for that key would be unaddressable — so the rule would have had to
  grandfather them or migrate them; that is the cost a *later* tightening would carry, and why the alphabet
  is settled now, before the first live document, rather than after.
