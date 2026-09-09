# `Szamlazz.Order` keeps no state: szamlazz.hu is the source of truth, reached through deterministic external ids

Status: accepted; amended by [ADR 0006](0006-account-selection-via-restate-scopes.md); the account whose
szamlazz.hu is the source of truth is the one the invocation's scope resolved to, and the validation pins are
read from that journaled `Account` (below); amended by #47, every journaled type is additive-only and pinned
by fixtures (the *Journal compatibility* section); amended by #70; widening a field to `Option<T>` is the one
retype the rule admits (the *Widening* paragraph); amended by #127; every journaled type is crate-owned, a
`szamlazz_agent` response type is never journaled directly (the *Crate-owned projections* section), the one
deliberate break of the pre-go-live window; amended by #125; the registry is complete by mechanism, and after
go-live an archived fixture of a type still journaled is never deleted: a shape that must break is a new type and
the old one retired with its directory (the *Completeness by mechanism, and the archive rule* section).
The three journal amendments (#47, #125, #127) are **superseded by
[ADR 0009](0009-immutable-deployments-no-journal-compatibility-contract.md)**: deployments are immutable, so an
in-flight invocation never replays against a later release's code and the journal carries no cross-version
compatibility contract. The crate-owned projections of #127 stay for the reason that was never about replay
(nothing a handler does not read, and never the agent key, in an entry the UI shows). The rest of this ADR stands.

The v1 design (ADRs 0002–0004 as first written) gave `Szamlazz.Order` a **ledger** in Virtual Object state:
one slot per document kind with a status machine (`pending`, `committed`, `rejected`, `blocked`, `reversed`,
`reversal_unverified`, `vacant`, `deleted`, `consumed`), a generation counter per slot embedded in the external
id (`{slug}:{order}:{kind}:{gen}`), a `request_id → entry` map with an HMAC payload fingerprint, corrective
entries with a per-order counter, a foreign hint and a bounded history. The ledger was written with `ctx.set`
before the first szamlazz.hu call and declared authoritative; szamlazz.hu was consulted to *verify* it on every
hit. Two operator handlers (`record_reversal`, `forget`, `ingress_private`) existed to repair it, and it carried
a schema version for migrations.

Building it showed that every question the ledger answered is already answered by Restate or by szamlazz.hu:

- **Serialization per order**: the Virtual Object's per-key lock: exclusive handlers on one key run one at a
  time (Restate; a paused or stuck invocation holds the key, ADR 0004).
- **Replay of completed steps inside an invocation**, the journal: a completed `ctx.run`, `ctx.sleep` or
  result replays after a crash; only the *open* closure re-executes (verified, ADR 0002).
- **A second live document of the same kind under one order number**, the account toggle "Rendelésszám
  ismétlődés tiltása" ON: different content → 71/152; a byte-identical resend while the first document is live
  → the same number, byte-identical response (verified). The replay compares the trimmed order number, the amount
  and the buyer name byte-exact, so the service normalizes the name once (trim + NFC) and serializes it
  identically on every attempt.
- **Whether a document is reversed**: `<sztornozott>true</sztornozott>` appears on the original after a storno,
  whoever performed it (UI, support, the service); the storno document carries `hivszamlaszam` = original and
  inherits the order number (verified). A repeat storno echoes the existing storno: storno is idempotent
  server-side (verified).
- **Which document an external id names**: a query by external id returns the **newest** holder (last-writer-
  wins, verified); a query by order number returns the newest document of any kind under it (verified).
- **Whether a proforma was consumed**: a converted proforma leaves the query surface (7 by number and by
  external id) while the converting invoice or prepayment carries `hivdijbekszam` (verified).

The one thing neither provides: **`ctx.run` is at-least-once across crashes.** A closure that crashes before its
result is journaled runs again, no sooner than the handler's `initial_interval` (verified: 20 s configured,
23.8 s observed, new process). The re-executed closure (and, after a kill or a client timeout, the *next
invocation*) must be able to find what a prior execution issued. That requires an identity known **before** the
send and computable by anyone holding the key, which the ledger provided by writing `pending` first. A
deterministic external id derived from the key alone provides it without state.

## Decision

`Szamlazz.Order` keeps **no state**. The Virtual Object exists for its per-key lock. Everything else is answered
by querying szamlazz.hu through external ids that are deterministic from the key:

- slot kinds: `"{namespace}:{order}:{kind}"`, `kind ∈ proforma | invoice | prepayment | final`
  (`ExternalId::for_kind`);
- correctives: `"{namespace}:{order}:corrective:{correction_id}"`, the caller names each corrective; a new
  `correction_id` issues a new corrective by contract, the same id finds the one it issued
  (`ExternalId::for_corrective`);
- storno: `"{namespace}:{order}:storno:{original_number}"` (`ExternalId::for_storno`), and
  `"{namespace}:by-number:{number}:storno"` for `Szamlazz.Agent.storno` of a document no `Order` manages.

**Last-writer-wins replaces the generation counter.** The question a create asks is "what is the newest document
of this kind we issued for this order?", which is exactly what the external-id query answers. After a storno the
reissued document becomes the newest holder of the same id; the stornoed original stays reachable by its number
and through the storno's `hivszamlaszam`. Nothing needs a suffix.

**Every `Found` document is validated before it is trusted**: `rendelesszam == order ∧ tipus ∈ kind-set`;
anything else is `conflict{external_id_collision}` (`FoundDocument::is_ours`, first `InvoiceDocumentExt::is_ours`
on the agent crate's document). External ids are not unique
server-side (verified), so this is the only protection against adopting a stranger's document. *Amended (ADR 0006):*
the formula gained `teszt == account.mode`, `account` being the invocation's journaled `Account`, resolved from the
scope, `mode` defaulting to `live`. *Amended (ADR 0006, XPRB amendment, then account-pin amendment):* a
`(account.supplier_id unset ∨ szallito/id == supplier_id)` term (`szallito/id`, the undocumented row id of the
seller record as printed on the document) went from mandatory in the multi-account shape to optional in both; then
both account terms were dropped, since neither `teszt` nor `szallito/id` is in a create response and neither check
could fire before the first document of a fresh order was issued into whatever account the key opens. The worker
holds no account pin; which account a key opens is the operator's go-live check. The
non-uniqueness of external ids was re-confirmed on 2026-09-06 (same kind, across kinds, original vs. its storno, and
reusable after a reversal, the *Reissue* path end to end).

**Retry identity is Restate's ingress `Idempotency-Key`**, supplied by the caller. The service does not know
whether one was used and never relies on it for safety: the external-id query inside the create step (the first
line of the closure on every execution) is the guard, the key is deduplication.

**`reissue: true` is required after *any* reversal.** A create that finds its document reversed returns
`outcome: reversed`; with `reissue: true` it proceeds, and on a live document it is `conflict{live}`. There is no
"the service stornoed this, so the next create is flag-free" path, because there is no record of who stornoed.

## Considered options

- **Keep the ledger.** Rejected. It was a second source of truth that had to be verified against the first on
  every hit anyway (verify-on-hit, verify TTL); every state it held was either derivable live or existed to
  describe the ledger's own uncertainty (`pending`, `blocked`, `reversal_unverified`, `vacant`). It cost the
  ledger module and everything built on it (roughly 4,400 net lines across the crate, ~2,700 of them the module
  and its tests) plus two operator handlers, an HMAC secret to deploy and rotate, a learned account fingerprint,
  and schema versioning with state migrations on every shape change.
- **`ctx.rand_uuid()` as the external id.** Rejected. Deterministic only within one invocation's journal. A
  caller retry after a kill or a client timeout is a *new* invocation with a new id and cannot answer "is there
  already one?": the exact question the pre-query must answer.
- **A caller-supplied external id.** Rejected. Safety becomes opt-in: a caller that omits or rotates the id
  re-opens the duplicate window. And szamlazz.hu never echoes `szamlaKulsoAzon`, not in create responses, not
  in the query XML (verified), so the caller can observe it nowhere; it has no benefit over the order number and
  the returned invoice number, which the caller already has.
- **A tiny "we stornoed this" state to keep flag-free reissue after a service-side storno.** Rejected. Such a
  marker cannot distinguish a *stale retry* of the original create, which arrives after the storno and must not
  issue (an identical resend after a storno issues a **new** invoice, verified), from a *new* deliberate
  request. The ledger told them apart with `request_id`; without it the marker authorizes both. One uniform rule
  costs one boolean on one call per reversal and can never cause a duplicate.

## Consequences

- **Given up, deliberately** (design §12): the `request_id` retry identity (→ `Idempotency-Key`);
  `conflict{payload_mismatch}` (a different payload for a live document is `already_issued`); flag-free reissue
  after a service-side storno (→ `reissue: true` after any reversal); `recorded_document_missing` (a document
  szamlazz.hu no longer knows is simply absent, live accounts cannot delete invoices); the `payments_before`
  capture on storno (query before stornoing, the server erases `<kifizetesek>` on the original); the ledger
  snapshot (`get` is four live queries and can return `unavailable`); the operator handlers `record_reversal` /
  `forget` (nothing to repair); the account fingerprint learned into state (pin `supplier_id` on the `Account`,
  itself dropped since: ADR 0006, account-pin amendment); schema versioning and state migrations.
- **Gained**: nothing to migrate, repair or drift; `get` is never stale; a UI storno, a support storno and a
  service storno are one case (`sztornozott`); a kill has nothing to compensate; a reset Restate cluster loses
  only in-flight invocations; the crate is a fraction of its former size.
- **Caller contract** (design §8, in the crate READMEs): (1) send an `Idempotency-Key` per logical request;
  (2) any error from an issuing or storno handler means "outcome unknown, retry with a **new** key" (Restate
  replays a failed invocation's stored completion for `idempotency_retention`, verified), the retry reconciles by
  external id and is safe; never read an error as "no document exists" (#67 later scoped this to the three
  "outcome unknown" codes; `outcome_unknown`, `unavailable`, `credentials_rejected`; design §7); (3) after any
  reversal a create returns `reversed`; send `reissue: true` with a new key when a new invoice is wanted.
- **Still required**: the toggle ON (the server-side guard against a second live document of the same kind);
  the byte-stable buyer name (the replay guard); the 2-minute gap before a re-check (the handlers'
  `initial_interval` for a crash, the issue policy's `initial_delay` for a lost reply), which must wait out a
  client timeout plus an observed ≥ 57 s server stall before the create step's leading query runs again; the
  cross-kind exclusivity check (`conflict{prepaid_chain}`) and the proforma-link check (`conflict{proforma_live}`),
  which the server does not perform.
- **Foreign documents** are detected live through the order-number hint in the lookup step, on every kind but
  correctives (a live `SZ | ES | VS` that is neither ours nor the document seen under our id →
  `conflict{foreign}`); nothing is recorded.
- **Consumed proformas** are derived live in `get`: proforma absent under its id while the invoice or prepayment
  carries `hivdijbekszam` → `{state: consumed, by}`.
- `CorrectionId` (`^[A-Za-z0-9][A-Za-z0-9._-]{0,39}$` since #64, first `{0,63}`, and not one of the external-id
  tokens) replaces the corrective counter and the `request_id ↔ cseq` map; it is the caller's per-corrective
  identity and part of the external id (ADR 0002, "Bounded inputs").
- ADR 0002's `{gen}` suffix, `request_id` and "written to state before the first call", ADR 0003's `request_id`
  and flag-free service-side reissue, and ADR 0004's `pending` slot, operator runbook and
  `idempotency_retention = 7d` (now `30d`) are superseded; the rest of each still holds.

## Amended (#47): journal compatibility; every journaled type is additive-only, pinned by fixtures

Giving up state migrations (above) did not give up every compatibility rule: the *journal* is the one thing an
in-flight invocation carries across a deploy. Every `ctx.run` result: the `namespace` pin, the `account` step's
`Resolution` (carrying the `Account`), and the gateway's `LookupOutcome`, `CreateOutcome`, `QueryOutcome`,
`StornoLookupOutcome`, `StornoOutcome`, `DeleteOutcome`, `SetPaymentsOutcome`, `ProbeOutcome`, `TaxpayerOutcome`,
is written as JSON by the deployment that ran the step and read back by whichever deployment replays the
invocation. An entry
the new code cannot decode is a retryable SDK error: the invocation replays into the same failure until the
handler's attempts are spent (five on the `Szamlazz.Order` issuing handlers, 2 m apart and doubling to 10 m,
about 24 minutes holding the order key) and is killed. The `Account` was documented additive-only from ADR 0006; the outcome types
embed `szamlazz_agent`'s `InvoiceDocument`, `InvoiceCreationResult` and `CreatedInvoice` *as they are*, whose serde
layout had no such rule, and the `Szamlazz.Agent.query` handler's run enum had already been reshaped once (#32),
accepted then for a one-step read-only handler with a one-day retention.

**Decision.** Every journaled type is **additive-only**: a new field carries a serde default, a new variant may be
added, and no field or variant is renamed, removed or retyped. The rule covered the agent crate's response types
the outcomes carried, whose JSON layout was thereby part of this crate's journal contract, until the #127
amendment below took them out of the journal; it is stated once, in the
`gateway` module docs. It is enforced in CI by `service::journal`:

- **the generator** pins one JSON fixture per variant of every journaled type under
  `crates/restate-szamlazz/tests/journal/<type>/<variant>.json`: the JSON the current code writes must equal the
  committed file byte for byte, and the run never writes unless `UPDATE_JOURNAL_FIXTURES=1`. Under that flag a
  missing fixture is written, and a *differing* one is kept beside the new shape as `<variant>.<n>.json` before the
  new shape is written, so an old shape is archived rather than overwritten;
- **the compatibility test** replays every fixture in every type's directory (the current shapes and every shape
  archived before them) through the current types: each must decode *and* re-encode to a superset of itself (a
  renamed `Option` field silently decodes to `None`; the superset check catches it where "decodes" alone would not).
  A directory with no journaled type behind it fails, so a type that stops being journaled is removed knowingly;
- the `Journaled` marker trait is the bound of the run helpers (`run_once`, `run_retrying`, `run_reading`): only an
  implementor can be a `ctx.run` result, and the pins take only implementors, so the trait is the link from the
  run sites to the fixture directory. Each enum's pins name its variants in an exhaustive `match`, so a new variant
  fails to compile until it is listed, and the generator then asks for its fixture.

The fixtures carry every element the wire can put in a document (postal addresses, ledger blocks, a financial
item, labels, two payments, a PDF), so a rename anywhere in the nested agent types is caught, not only at the top.
Verified on a scratch branch: renaming `InvoiceDocument::labels` fails both tests on every document-carrying
fixture (missing field `tags`); renaming `LookupOutcome::Reversed::storno_number` fails the compatibility test on
the superset check ("decodes, but re-encodes without part of the fixture"); adding a defaulted field to `Account`
passes the compatibility test, fails the generator on the one changed fixture, and regenerates into
`account.1.json` + `account.json` with both replaying.

**Considered: crate-owned projections instead of the agent's types.** The document outcomes could carry
restate-szamlazz's own `JournaledDocument` / `JournaledCreation` structs mapped from the agent's, decoupling the
journal from the agent crate's layout. Rejected for them at the time: it duplicates some twenty-five fields across three types
plus a mapping layer that can itself drift, for a coupling the fixtures already make visible; a change to the agent
types fails this crate's CI through the workspace, which is the wanted outcome. The agent crate's `query_xml` module
already promises its response types round-trip through JSON "for journaling or caching"; the fixtures hold it to
that. `TaxpayerOutcome` (#49) is the one journaled type that took the projection route (`QueryTaxpayerResponse`
is crate-owned and doubles as the handler's response, so there is no second type to keep in step), and it is pinned
by the same fixtures (`tests/journal/taxpayer-outcome/`), so either route ends in the same check. *Reversed by the
#127 amendment below*, once the coupling's cost had shown: the `teszt` widening (#70) needed a rule exception to
land at all, and every future change to an agent response type had to be checked against this crate's fixtures
first.

**Consequences.** An upgrade with in-flight invocations is safe by construction, and design §10's
rolling-update guidance says so with this section as the reason: the drain-first roll is still recommended to
avoid stalling in-flight orders for a retry interval, but it is not what keeps them alive. A change that *must*
break a journaled shape is a deliberate act: drain before deploying (the flag-day script) so that nothing is in
flight to be killed, and, once a production deployment exists, retire the type rather than delete its archive
(the #125 amendment below). `Journaled` is `pub(super)` to `service` and sealed (#125); a new run site outside
`service::support` would have to bypass the helpers to journal an unpinned type.

**Widening (#70).** One retype is additive in the sense the rule cares about: a field `T` becoming `Option<T>`,
when every value the old type ever wrote decodes to `Some` and re-encodes byte for byte. `InvoiceInfo::test`
(`teszt`) went from `bool` to `Option<bool>` in #70, the old code wrote `true` or `false`, the new code reads both
as `Some`, and the ten document-carrying fixtures with `"test": true` replay through the compatibility test
unchanged, which is the proof the rule asks for; no fixture was regenerated. What the widening adds is a value the
old code could not write (`null`), which the *previous* deployment cannot decode: a rollback with in-flight
invocations would kill them on that entry. The rule was always forward-only (it promises that the next deployment
decodes what the previous one wrote, never the reverse), so nothing new is given up; but a widening is still a
contract change to review, not a free refactor, and the compatibility test (not the type signature) is what says
it is admitted. Narrowing (`Option<T>` → `T`) is a retype like any other.

## Amended (#127): journaled types are crate-owned; a `szamlazz_agent` response type is never journaled directly

The #47 amendment let the document outcomes carry the agent crate's `InvoiceDocument`, `InvoiceCreationResult` and
`CreatedInvoice` as they were, and made the agent crate's serde layout part of this crate's journal contract. The
cost showed within weeks: `teszt` going to `Option<bool>` (#70) needed the *Widening* exception to land at all,
every further change to an agent response type (a rename, a retype, a field the receiver-side model wants) had to
be checked against this crate's fixtures first, and the crate contradicted itself: `TaxpayerOutcome` journaled a
crate-owned projection and `SellerConfig` existed beside `Seller` for exactly this reason (#68), while the document
outcomes did not. The journaled document also carried the buyer block, the seller block, the line items and the
base64 PDF, none of which any handler reads, into an entry the Restate UI shows for the retention period.

**Decision.** Every journaled type is **crate-owned**. The document outcomes carry the worker's projections of
what the handlers read, and nothing else (`restate_szamlazz::gateway::document`):

- `FoundDocument`, from a queried `InvoiceDocument`: `document_id` (`alap/id`), `number`, `document_type`
  (`tipus`), `order_number` (`rendelesszam` trimmed, empty read as none: the one reading of the element, made
  once, in the projection), `reversed` (`sztornozott`), `referenced_invoice_number` (`hivszamlaszam`),
  `referenced_proforma_number` (`hivdijbekszam`), `appearance` (the `eszamla` code as an integer; the agent
  crate's `InvoiceAppearance` reads it, so a code the crate learns later is read on replay), `issue_date`
  (`kelt`), `fulfillment_date` (`telj`), `due_date` (`fizh`), `currency`, `test` (`teszt`), the grand total
  (`net_total`, `vat_total`, `gross_total`) and `payments` (`RecordedCreditEntry`: date, title, amount, comment,
  bank account). `document_id` is read by no handler and is carried so that an entry names the document the way
  szamlazz.hu's records do. The external id of #127's field list is **not** carried: szamlazz.hu never echoes
  `szamlaKulsoAzon` in a query or create response (above), so it cannot be read off a document, and every handler
  holds it from the key already. `LookupOutcome`, `CreateOutcome` and `QueryOutcome` carry it boxed where they carried
  `Box<InvoiceDocument>`. The checks the services make on a found document (`is_live`, `is_ours`,
  `carries_order`, `is_storno_of`, `e_invoice`, `payment_amounts`) are its methods; `InvoiceDocumentExt` is gone.
  `Szamlazz.Agent.query`'s `QueryResponse` (a caller contract, unchanged) is projected from it.
- `IssuedDocument`, from a create reply (`InvoiceCreationResult`, `TryFrom` rather than the `From` #127 named:
  the agent type's number is optional, a PDF preview's reply has none, and on `main` the create step already turned
  such a reply into `Unconfirmed::Open` before it could be journaled; the conversion now says so in its type, and the
  handler's unreachable "issued without a number" arm is gone) and from a storno reply (`CreatedInvoice`, `From`):
  `number`, `document_id`, `net_total`, `gross_total`, `outstanding`, `customer_account_url`,
  `notification_delivery_failed`; never the PDF. `CreateOutcome::Issued` and `StornoOutcome::Reversed` carry it.

The projection is made at the gateway's one wire boundary (`Gateway::query_raw`, the create and storno sends);
nothing past it holds an agent response type. The rule is stated in the `gateway` module docs and checked by
`service::journal` as before, plus a guard that no variant of any journaled type serialises a `supplier`, `buyer`,
`items`, `financial_items`, `labels` or `pdf` key at any depth (the archived pre-#127 shape is its positive
control). The projection types are `#[non_exhaustive]` and additive-only like every journaled type; a change to an
agent response type now reaches the worker as a compile error in the `From` impls, never as a journal entry the
next deployment cannot decode.

**The archived shapes are the one deliberate break of the pre-go-live window.** The old `InvoiceDocument`-shaped
entries do not decode into the flat projection, and were not made to: a legacy mirror struct with a custom
`Deserialize` plus a carve-out in the compatibility test's superset check (for the keys the projection drops) would
be permanent complexity for a scenario that cannot occur, there having been no production deployment before the
change and so nothing in flight to be killed. The generator archived the twelve replaced shapes as
`<variant>.1.json` (`lookup-outcome/{live,reversed,collision,foreign}`,
`create-outcome/{issued,found,reversed,live-again,reconciled,collision}`, `query-outcome/found`,
`storno-outcome/reversed`); `service::journal::DELIBERATE_BREAKS` lists them, and the compatibility test skips
each and asserts it still fails to replay, so the list cannot outlive its reason. The archives are the record of the
shape that was replaced (one of them the data guard's positive control), not a way to break the journal again: a
break after go-live is a deleted archive and a drained deploy (the #47 amendment's *Consequences*), and the endpoint
README's rolling-update guidance names this release as the one whose drain is mandatory.

**Consequences.** The agent crate's response types are free to evolve without a journal review; the worker's
journal contract is the projections' and the crate's own. A field a handler comes to need is added to the
projection with a serde default (additive), read from the agent type in the `From` impl; a field the agent crate
renames costs one line there. The journal fixtures shrink from ~170 lines per document to ~35, and pin what an
entry holds field by field rather than through the agent crate's layout. `InvoiceDocumentExt` is gone; the design
doc's and CONTEXT.md's references to it name `FoundDocument`'s methods instead.

## Amended (#125): completeness by mechanism, and the archive rule

Two parts of the enforcement above were discipline. A new `impl Journaled` was caught only if its author also added its pins to `registry()` (the unclaimed-directory
check runs fixture directory → registry, never implementors → registry), and a new enum variant compiled once
*named* in the exhaustive match, nothing requiring a *sample* for it. Both are mechanism now: the trait is
sealed and implemented through the `journaled!` list beside it, the one place it can be implemented, which also
yields the list of implementors the registry test holds `registry()` to (a type journaled without pins fails by
name); and each enum's pins name its variants through `variants!`, whose list `pins` checks the samples against
(a variant that compiles but has no fixture fails by name). What no mechanism can decide is the **archive rule**:
an archived shape (`<variant>.<n>.json`) is the only record of a shape a running deployment may have journaled,
and the generator cannot tell a legitimate deletion from an illegitimate one. `5ea51f9` (2026-09-07) regenerated
`resolution/account.json` without `mode` and `supplier_id` and committed no `account.1.json`, a removal admitted
because nothing had been deployed to replay the old shape and recorded in the commit message only; after go-live
the same commit would kill every order in flight across the upgrade. So: once the first production deployment
exists, an archive of a type the code still journals is never deleted and a fixture is never regenerated without
its archive; before that, a regeneration without an archive is a judgement call recorded in the commit message.
The one way a directory goes is retirement: a shape that must change beyond what additive allows is a new
journaled type under a new directory (and a new run-name row), and the old type is dropped from the `journaled!`
list with its directory, archives included, in the same commit (the unclaimed-directory check demands it). Safe
because that deploy drains first: once nothing of the previous deployment is in flight, no invocation can replay
the retired type, and a completed invocation's journal is read by the UI, never replayed. The #47 amendment's
*Consequences* ("delete the archived fixture") and the #127 amendment's "a break after go-live is a deleted archive"
are superseded by this. The rule is stated where the generator's instructions are (`service::journal`'s module
docs) and in CONTEXT.md's *Journaled type* entry.
