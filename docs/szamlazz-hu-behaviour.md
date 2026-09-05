# Verified szamlazz.hu Számla Agent behaviour

What the `restate-szamlazz` design relies on that szamlazz.hu does not document, observed against the
live Számla Agent. Every row below was observed on **one szamlazz.hu TEST account, on one day
(2026-09-03)**, with e-invoicing enabled (`<eszamla>1</eszamla>`), every document marked
`<teszt>true</teszt>`, and the account toggle "Disable order number repetition" ON, through the
`szamlazz-agent` crate. Probe ids (`A1`, `B4`, `C2-5`, …) refer to the review record's probe findings
A–D and their raw request/response logs, which are not part of this repository. Restate runtime
facts live in ADRs 0001, 0002, 0004 and 0005, not here.

Treat these as facts about *that* account. Some may depend on account settings (e-invoice, cash
accounting), and szamlazz.hu may change any of them without notice; the go-live checklist at the end
is the minimum to re-establish them on the target account.

Notation: `SZ` invoice, `D` proforma, `ES` prepayment, `VS` final, `HS` corrective, `SS` storno,
`SL` delivery note (the `<tipus>` codes). "7" etc. are `<hibakod>` values.

## Order numbers and the duplicate toggle

| Behaviour | Verified how | Design consequence |
|---|---|---|
| With the toggle ON, a second document of the same kind under an order number with different content is rejected with 152 "Már létező rendelésszám: {order}. …". The message names the order number only, never the existing invoice number; HTTP 200; headers `szlahu_error_code`/`szlahu_error` set, no `szlahu_szamlaszam`/`szlahu_id`. | A4c-2, A4d-alt, A5-price2000, A6; C1-7 | 71/152 → re-query by external id: a live document of ours → `reconciled`; not ours → `conflict{external_id_collision}`; reversed and ours, or absent → the duplicate is not ours and the order-number query names it → `conflict{duplicate_order_number}` with `existing_number` when the newest document under the order is a live document of our kind, without it otherwise; nothing under the order at all is a contradiction the create step retries. Never `rejected` (except for correctives), never a new document. |
| The check is **per document kind**: `SZ`, `D`, `ES`, `VS`, `SL`, `HS` each accepted the same order number in sequence; a second `SL` (different price) → 152. `SZ`-vs-`SZ` → 152. | C1-1…C1-7, D4-create-sz | Cross-kind exclusivity (plain invoice vs prepayment chain) is the service's own check (`conflict{prepaid_chain}`); 71/152 is intra-kind only. |
| Correctives are exempt: an `HS` was accepted under an order already carried by its base and by five other kinds. `HS`-vs-`HS` not tested. | B7-create-corrective, C1-6 | `correct_invoice` takes no order-number hint (the live base under the order is expected) and a 71/152 its re-query cannot resolve is `rejected`, not a conflict; a new `correction_id` issues a new `HS` by contract; the external-id query is the only guard. |
| Create **trims** leading/trailing whitespace from the order number: `" PRB-C-Case "` and `"PRB-C-Case "` replayed the existing invoice; with a different price → 152 naming the *trimmed* value. | C4-3, C4-4, C4-5 | VO key and every `rendelesszam` are derived from the trimmed bytes. |
| Case is **preserved and significant**: `prb-c-case` created a second invoice next to `PRB-C-Case`; each queryable under its own spelling. | C4-2, queries | No case-fold; two spellings are two orders. |
| Query by order number is **exact**: padded values → 7; case-sensitive. | C4 queries | A padded order number would be creatable-but-unqueryable → trim is mandatory; internal whitespace/control characters are rejected (untested server-side). |
| After a storno the order number is **reusable** — also by a byte-identical request. | A5-replay | A create that finds its document reversed returns `outcome: reversed`; a new document is issued only with `reissue: true`, and it becomes the newest holder of the same external id; ADR 0003, ADR 0005. |
| A corrective netting to zero does **not** free the order number: `SZ`-78 → `HS`-79 (−1270) → new `SZ` → 152. The corrected `SZ` carries no marker of correction. | C5 | Once an `HS` exists the order's invoice kind is terminally occupied; recovery needs a new order number. |
| `query --order` returns the **most recently issued document of any kind** carrying the order number: the `HS` after six kinds; the `SS` right after a storno; the reissued `SZ` after storno + reissue. | C1-q-order, B2, B3, B6, A5-q-order | The hint is a secondary signal (foreign detection, storno-number discovery for `outcome: reversed`), never the sole liveness check — that is the external-id query plus `sztornozott`. |

## Identical-request replay

| Behaviour | Verified how | Design consequence |
|---|---|---|
| A byte-identical resend seconds later returns the **same number** with `sikeres=true`; body and headers are byte-identical to the original (same `szfejguid`, same `szlahu_id`). No replay marker exists. | A4-base-1/2 | `Issued(r)` may be a replay; report it as `issued` either way. |
| Replay is ~0.7–0.95 s (one at 1.8 s); a real issue 1.8–5.5 s. | A4, C4-3 | Latency is a heuristic, never a decision input. |
| Fingerprint **includes**: the (trimmed) order number as gate, the amount (unit price 1001 → 152), the buyer name **byte-exact** (`"próba vevő kft. "` — case and trailing space — → 152). | A4d-alt, A4c-2 | Normalize the buyer name once at validation (trim + NFC) and serialize it identically on every attempt, so the replay is a stable second guard. |
| Fingerprint **excludes**: `szamlaKulsoAzon` (other external id → replay), `keltDatum` (+1 day and −1 day → replay), `megjegyzes` (comment → replay). | A4a-2, A4b-2, A4b-3, A4e-2 | Pinning `issue_date` is not a guard; the service sends it only when the caller supplies it. A replayed document's `kelt` may differ from the request. |
| Untested: due date, fulfillment date, item name/quantity/VAT, buyer address, currency, payment method; the documented "2 days" window. | — | Not relied on; only affects how permissive the replay is. |
| Replay lasts only **while the matching document is live**: after `SZ`-72 → `SS`-73, the byte-identical resend issued `SZ`-74 (new id, real-issue latency); a different-price request then got 152 because 74 holds the order number. | A5 | Replay protection ends at storno. After a reversal the external-id query is the only guard; the lookup step answers `outcome: reversed` for a reversed document and the create step is never reached without `reissue: true` (ADR 0003). |
| Re-converting a consumed proforma under the same order (different external id) → byte-identical replay of the existing `SZ`. | C2-6 | Same replay behavior via the proforma path; the replay stored no external id. |

## External ids (`szamlaKulsoAzon`)

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Not unique**: two `SZ` under different orders with the same external id → both issued, no warning. | A3-create1/2 | Validate every `Found` document (`rendelesszam`, `tipus`, `teszt`, `szallito/id` when pinned), else `conflict{external_id_collision}`. |
| Query and PDF by a shared external id return the **latest** holder (last-writer-wins): 50 over 49; 74 over 72. | A3-query-ext, A3-pdf-ext, A5-q-ext | The newest holder is exactly the document a create asks about, so `{namespace}:{order}:{kind}` needs no generation suffix; a reissue becomes the newest holder and the stornoed original stays reachable by number and via the storno's `hivszamlaszam`. |
| Read-your-writes lag **≈ 0**: query by external id succeeded 771 ms after the create returned, and at +2 s, +10 s, +60 s; `pdf --external-id` works. | A1-q0/q2/q10/q60, A1-pdf | The 2-minute re-check gap is justified by in-flight requests (see Latency), not by lag. |
| A 110-character id containing `: . _ -` and a unicode id were accepted and queryable. | A2-create-long, A2-create-uni | `{namespace}:{order}:{kind}`, `…:corrective:{correction_id}` and `…:storno:{number}` fit without hashing. |
| **Never echoed**: the query XML has `<rendelesszam>` but no external-id element; create responses carry none either. | A1-q-raw; every query in A–D | The id → number mapping is readable only by querying *with* the id, which is why the id must be derivable from the key alone. |
| Attaches **only on the call that creates**: a replayed create with another id (A4a), a repeat storno with a new id (B4x), a padded replay (C4) stored nothing — query by those ids → 7. | A4a-2, B4x-query-new-extid, C4 `prb-c-case-3` | A first send that lands as a replay of a pre-existing identical document is invisible by external id; only the order-number hint finds it (`conflict{foreign}`). |
| On `xmlszamlast` the external id **attaches to the `SS`**: the storno document is queryable by the storno request's id (`tipus=SS`, `hivszamlaszam` = original); the original keeps its own id. | B6-storno-with-extid, B6-query-storno-extid, B6-query-orig-extid | Storno gets a query-first guard under `{namespace}:{order}:storno:{original_number}`. |
| The external id of a deleted or converted proforma → 7. | D1-query-extid, D4-query-proforma-extid, C2-5 | A proforma 7 by id means deleted *or* consumed; `get` disambiguates via the invoice's or prepayment's `hivdijbekszam`. |

## Reversal signals

| Behaviour | Verified how | Design consequence |
|---|---|---|
| `<sztornozott>` is **absent** (not `false`) before a storno; afterwards the original's `<alap>` gains exactly `<sztornozott>true</sztornozott>` (after `<teszt>`). The `SS` never carries it. Also visible when the original is fetched by its external id. | B1 diff, A5-q-72/73, B6-query-orig-extid | The lookup step detects a UI storno without any state or operator: `sztornozott == Some(true)` → `outcome: reversed`. The agent crate exposes `Option<bool>`; `None` ⇒ live. |
| The `SS` inherits `<rendelesszam>` and carries `<hivszamlaszam>` = original. `<gazdEsemAzon>` of an `SS`/`HS` equals the original's `<id>`; a converted `SZ` inherits the `D`'s id. | B1, A5-q-73, B7-query-corrective, D4-query-sz | The `storno_number` on `outcome: reversed` comes from the hint when the newest document under the order is the matching `SS`, else it is absent. `gazdEsemAzon == original.id` is an optional consistency check. |
| A storno negates the **quantity** (−1), not the unit price; `SS` totals are negative (`szamlabrutto=-1270`, `kintlevoseg=-1270`). | B1 | Storno response validation: `gross_total < 0`. |
| A storno **wipes `<kifizetesek>`** from the original (body shrank; `payments=[]`); the `SS`'s `kintlevoseg` is the full negative gross, prior credits not netted. The query XML has no `kintlevoseg` element at all. | B8 | The service does not snapshot payments; a caller that needs them queries before stornoing (`Szamlazz.Agent.query`) and re-registers on the new invoice via `set_payments`. Outstanding is observable only via response headers. |

## Storno semantics

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Repeat storno** of a stornoed invoice → `sikeres=true` echoing the **existing** `SS` (same number, same `szlahu_id`, −1270); no error code; no second `SS`; 741 ms vs 2352 ms for the real storno. | B4-repeat-storno, B4-query-order-after-repeat | Storno is idempotent per original number: re-sending is safe; the storno step is query-first on every execution under the issue policy. A re-executed step whose leading query finds the `SS` sends nothing; one that re-sends gets the echo, not a duplicate. |
| Storno of a **proforma** or a **delivery note** → `sikeres=true` echoing the *requested* number with **positive** totals; the document is unchanged (no `<sztornozott>`, no `SS`). | B5-storno-proforma, B5-storno-delivery-note | Success-shaped no-op. Validate: `invoice_number ≠ requested ∧ gross_total < 0`, else `NotStornoable` → `rejected{not_stornoable}`. `tipus` is not in the storno response — confirm `SS` in a follow-up query. |
| Storno of an **`SS`** → 14 "Sztornó és jóváíró számlát nem lehet sem sztornózni, sem jóváírni." | B5-storno-SS | Type 14 in the crate; `rejected{14}`. |
| Storno of an invoice that **has a corrective** → 221 "Ez a számla nem sztornózható (van helyesbítő számlája)." | B7-storno-corrected-orig | The server is the guard: `rejected{221}`; type 221. |
| Storno `keltDatum` other than today → 352 "A számla kelte csak a mai nap lehet: 2026.09.03.." | B3-storno-earlier-kelt | Never send `keltDatum` on a storno; type 352. (352 on *create* is untested.) |

## Proformas: conversion, auto-linking, deletion

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Converting `D` → `SZ` with `dijbekeroSzamlaszam` under the **shared order number** is not a 152; the `SZ` carries `<hivdijbekszam>`. | C2-3, D4-create-sz | `options.proforma: auto` (default) passes the live proforma found under `{namespace}:{order}:proforma`. |
| After conversion the `D` is **gone**: 7 by number and by external id; delete → 335. | C2-5, D4-delete-converted, D4-query-proforma-* | `get` reports `proforma: {state: consumed, by}` when the proforma is absent under its id while the invoice or prepayment carries `hivdijbekszam`; `delete_proforma` answers `{deleted: true, reason: absent}`. |
| **Auto-linking by order number**: an `ES` issued *without* `dijbekeroSzamlaszam` under the `D`'s order shows `<hivdijbekszam>D-…</hivdijbekszam>` and the `D` became unqueryable. | C1-3, C2 | `proforma: none` is unenforceable: with a live `D` of ours under `{namespace}:{order}:proforma` the create returns `conflict{proforma_live}`; the caller deletes the proforma or lets `auto` link it. Consumption by an `ES` as well as an `SZ` is derived in `get`. |
| A **second conversion** from a consumed `D`: same order → replay of the existing `SZ`; different order → a plain `SZ` with the reference **silently dropped** (no `hivdijbekszam`, own `gazdEsemAzon`). | C2-6, D4-create-sz2 | `dijbekeroSzamlaszam` is best-effort and the create response cannot reveal a dropped link. `proforma: {number}` verifies the `D` first; `get` shows the link that actually landed via `referenced_proforma`. |
| An `SZ` referencing an explicitly **deleted** `D` → success, reference silently ignored (5455 ms). | D5 | 7 on the proforma verify ⇒ `conflict{proforma_missing}`. |
| Delete: success is `<xmlszamladbkdelvalasz><sikeres>true</sikeres>` with **no** `szlahu_*` headers. A second delete → 335 "Nincs ilyen díjbekérő (vagy törölték, vagy nem is létezett)." with headers; a never-existed number → the same 335. Delete by order number works. | D1, D2 | `335 ⇒ deleted` is safe because the number was just found under our external id. |
| A credit entry on a `D` is accepted (`kintlevoseg 0`, `<kifizetesek>` readable); a **fully paid `D` deletes without any guard**, taking its payment history with it. | D3 | The paid guard is service-side (`force`); `kifizetesek` is read in the proforma lookup. |
| `<vevo>` in a query is **live partner master data** (overwritten by a later create with the same buyer name), not an at-issue snapshot; `<alap><email>` is per-document. | D6-query-bademail1 | Never compare `<vevo>` to the request; the service compares no payload — a different payload for a live document is `already_issued`. |

## Prepayment and final invoices

| Behaviour | Verified how | Design consequence |
|---|---|---|
| A `VS` issued **without** `elolegSzamlaszam` under the `ES`'s order is linked anyway: `<hivszamlaszam>` = the `ES`. | C6-2, C6-3 | Pass `elolegSzamlaszam` explicitly regardless; the server links by order number. |
| The server does **not** net the prepayment into the final: `VS` gross 1270, `kintlevoseg` 1270. | C6-2 | The caller supplies the negative prepayment line. |
| A second `VS` against a settled `ES` → 73 "A hivatkozott előlegszámla nem beazonosítható. …" — with the correct number, under the same or a new order; 73 fires before any 152; headers set. | C6-4, C6-5 | The 1:1 rule is enforced via 73 → `rejected{73}`; the lookup step on `{namespace}:{order}:final` answers `already_issued` first when the final is ours; type 73. |
| References are one-directional: the settled `ES`, converted `D` and corrected `SZ` show nothing. | C3, C6-6, C5-q-78 | The relationship is read from the referencing side only: `get` derives `consumed` from the invoice's `hivdijbekszam`, the storno number from the `SS`'s `hivszamlaszam`. |

## Corrective invoices

| Behaviour | Verified how | Design consequence |
|---|---|---|
| An `HS` is accepted under the base's order number with `<hivszamlaszam>` = base, `<gazdEsemAzon>` = base id, negative totals (`szlahu_bruttovegosszeg=-1270`). | B7-create-corrective, C1-6, C5-2 | External id `{namespace}:{order}:corrective:{correction_id}`; no 71/152 path. |
| Once an `HS` exists: storno of the base → 221; an `HS` netting to zero does not free the order number (152 on the next `SZ`). | B7, C5-3 | `correct_invoice` on a reversed base → `conflict{base_reversed}`; storno of a corrected base → `rejected{221}` from the server; the order's invoice kind is terminal. |

## Credit entries (`setPayments`)

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Replace** semantics by default: 100 then 200 leaves `[200]`; `additiv=true` appends (`[200, 50]`, outstanding 1020). `szlahu_kintlevoseg` header and `<kintlevoseg>` body agree and equal gross − Σ. | D7 | `set_payments` default is replace; never auto-retried (`max_attempts(1)` run). |
| Five entries accepted; the query returns them in **non-submission order** (`20,40,10,30,50`). A sixth is refused by the crate before sending (server code unknown). | D7-credit-5amounts | `<kifizetesek>` order is not meaningful. |
| Credit on a **reversed** invoice → 463 "Sztornózó vagy sztornózott számlához nem tartozhat kifizetettségi információ." — body only, no headers. | D8-credit-on-reversed | Type 463; the wording implies the same code for a credit on the `SS` (untested). |

## Error codes and header presence

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Header presence is **per operation**: create (152, 73), storno (14, 221, 352) and delete-proforma (335) set `szlahu_error_code` + `szlahu_error`; query (7) and credit (463) are **body-only**. | A6, B3/B5/B7, C6, D1, D8 | The crate must always parse `<hibakod>`; never detect errors from headers alone. |
| Code 7's text — "Hiányzó adat: számla xml (ismeretlen számlaszám, rendelésszám vagy külső azonosító)." — covers unknown number, order number *or* external id, and also a consumed proforma. | D1-query-*, C2-5 | 7 is "not on the query surface", not "never existed". |
| Codes 14, 73, 221, 352, 463 were not named in the crate at probe time (parsed as `Unknown`). | error.rs review | Type them; none is retryable. |
| The `szlahu_id` header is the **document id** (= `alap/id` = `gazdEsemAzon`), different for every document. The supplier id is `szallito/id` (972720 on this account, identical on 10/10 queries) and appears **only in query bodies** — create responses have no `<szallito>`. | C3, D9 | The account's `supplier_id` pin (optional in the single-account shape, required in the multi-account shape — it is the only server-side account identity) is checked against `szallito/id` on every document found under our external ids; a not-found probe (`check_account`) cannot cross-check it; any text calling `szlahu_id` the supplier id is wrong. |
| Success headers on create/storno/credit: `szlahu_szamlaszam`, `szlahu_id`, `szlahu_kintlevoseg`, `szlahu_vevoifiokurl`, …; delete success sets none. | A1, D1, D3 | — |

## Latency

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Queries 0.66–1.0 s (10-sample median 858 ms; first of a session 4.8 s); creates 1.8–5.5 s; replays and errors 0.7–0.95 s (one replay 1.8 s); storno that creates 2.3–2.8 s, echo 0.7–1.1 s; credits 0.7–1.3 s; delete 0.7–1.0 s. | D9-lat-1…10; A, B, C, D logs | The 180 s budget of the create closure (three 60 s calls: leading query, create, re-query) stands; `inactivity_timeout 4m`, `abort_timeout 3m`. |
| One create **stalled ≥ 57 s** with no response and issued nothing (checked by order-number query); every other call returned within 0.7–5.5 s. Not re-sent. | A4d-2, A4d-q | The re-check must wait longer than client timeout (60 s) plus stall: the handlers' `initial_interval` (a crash) and the issue policy's `initial_delay` (a lost reply, re-executing the create step) are both `2m`, never below ~90 s. |
| Code 56 could not be triggered: a malformed (`nem-email-cim`) and an undeliverable buyer e-mail with `sendEmail=true` both returned plain success, `notification_delivery_failed=false`; the malformed address was stored on the document. | D6 | 56-without-number leaves the create step's outcome open → an immediate external-id re-query, then `Unconfirmed` (the run policy re-executes the step) when nothing is there — safe either way. |

## Test-account caveats

| Caveat | Why it matters |
|---|---|
| Everything above is one TEST account, one day, roughly 75 document-creating calls in four sessions. | Nothing here is a documented guarantee. |
| Every document is `<teszt>true</teszt>`; `szallito/id` is 972720. | The account's `mode` (default `live`) is validated against `<teszt>` on every document found under our external ids; the live account has a different supplier id and `teszt=false`. In multi-account mode each account carries its own `mode` and `supplier_id`. |
| E-invoicing is enabled (`<eszamla>1</eszamla>` on all but `D`/`SL`). | 352 (kelt must be today) on storno may be an e-invoice rule; behavior on paper-invoice accounts is unknown. |
| The test account did not produce 56 for bad addresses. | Either test accounts do not send mail or 56 is raised only on synchronous hand-off failures. |
| Other probes were issuing concurrently, so `CTEST-2026-*` numbers are not contiguous. | Irrelevant to the facts; noted so the raw logs are not misread. |

## Still unverified

- Code 56 shape (with/without a number; header form). Low: 56-with-number is a warning; without →
  `Unknown` → re-query.
- `HS`-vs-`HS` under the toggle; whether the replay applies to `HS`, `D`, `ES`, `VS` at all (only
  `SZ`-vs-`SZ`, `SL`-vs-`SL` and `SZ`-from-`D` were exercised). Moderate for correctives; the
  external-id query inside the create step is the working guard.
- Due date, fulfillment date, item fields, address, currency in the replay fingerprint; the "2 days"
  window. Low: the replay is no longer the primary guard.
- 352 on **create**, and on non-e-invoice accounts. Low–moderate: the service does not pin
  `issue_date` unless the caller supplies it.
- Whether "last" in `query --order` is by id or by `kelt` (indistinguishable while kelt must be
  today). Low: the hint is secondary.
- Server code for a sixth credit entry; credit on the `SS` itself (463 expected). Low.
- Storno of `ES`/`VS`/`HS`; a new `VS` after a stornoed `VS`; an `SZ` beside a live `ES`. Moderate:
  `storno_invoice` accepts `ES`/`VS`/`HS`, and `create_final` with `reissue: true` after a reversed
  `VS` sends a new one; the `SZ`-beside-`ES` case is refused by the service (`conflict{prepaid_chain}`)
  before sending.
- A second `D` after a consumed `D` (152 expected). Low: the order-number hint in the lookup step sees
  the converting invoice or prepayment first → `conflict{foreign}`.
- Whether a UI-converted `SZ` carries `rendelesszam`/`hivdijbekszam`; which e-mails a UI or Agent
  storno sends. Moderate for foreign detection (an `SZ` without `rendelesszam` is invisible to the
  hint) and the customer-facing narrative; not a safety issue.
- Internal whitespace and NFC handling of order numbers (only edge whitespace and case tested). Low:
  rejected rather than guessed.
- **Credential codes 3, 135, 136, 164** (invalid credentials, browser session active, login blocked,
  multiple accounts): none was observed on the probe account. The worker relies on szamlazz.hu's
  documentation that they are answered **before any write** — so the attempt that sees one has sent
  nothing — and surfaces each as `TerminalError{credentials_rejected}` on every operation. Their
  header form (header + body, or body-only) is likewise assumed from the documentation; the crate
  parses `<hibakod>` either way. Moderate: were a credential code ever returned *after* a document was
  issued, the fault still says "outcome unknown" and the next call's external-id query finds it.
- **Everything on a live account** (`teszt=false`, possibly non-e-invoice): e-mail sending, 56, 352,
  `szallito/id`. Go-live precondition — see below.
- **By-number operations under the wrong scope** (multi-account mode, ADR 0006): what szamlazz.hu answers
  when account A's agent key queries, credits or stornos an invoice *number* that belongs to account B —
  7 (not on this account's query surface) is expected, but a shared number space or a different code is
  possible; only one account was probed. Moderate: `Szamlazz.Order.storno_invoice` verifies first and
  refuses a document whose `teszt` / `szallito/id` are not the resolved account's (`account_mismatch`);
  `Szamlazz.Agent.query`, `set_payments` and `storno` check no account pin — by-number operations act on
  whatever szamlazz.hu returns to that account's key (a found document does carry `teszt` and `szallito/id`;
  the check is simply not made there). The
  worker relies on szamlazz.hu answering 7 across accounts; the caller records the scope as used per order
  (safety contract rule 5) precisely so that the case is never exercised.

## Go-live checklist

Re-run these on the target szamlazz.hu account before enabling the worker — on **every** account a multi-account
deployment serves. Every step issues real,
numbered documents there; agree the test order numbers and their subsequent storno with whoever keeps
the books first. Confirm the toggle "Disable order number repetition" is ON in the account settings
before starting.

| Step | Probe | Expect | Feeds |
|---|---|---|---|
| 1 | A1 — create with an external id, query by it at +0/+2/+10/+60 s | Hit every time; `<teszt>false</teszt>`; note `szallito/id` | The account's `supplier_id` and `mode = live` (per account in multi-account mode), lag ≈ 0 |
| 2 | A4-base — byte-identical resend of a create | Same number, byte-identical response | Toggle ON confirmed; replay guard works |
| 3 | A4c — resend with the buyer name changed only in case/trailing space | 152 naming the trimmed order number | Fingerprint is byte-exact on the buyer name; 152 header shape |
| 4 | A5 — create → storno → byte-identical resend | A **new** invoice; order number reusable | Replay ends at storno (ADR 0003 hazard is real here too) |
| 5 | B1 — query the original by number before and after the storno | `<sztornozott>true</sztornozott>` appears on the original, never on the `SS` | Reversal detection in the lookup step (`outcome: reversed`) |
| 6 | B4 — repeat the storno | Echo of the existing `SS`, no error, no second `SS` | Storno re-send is safe |
| 7 | B6 — storno with an external id, query by it | Returns the `SS` (`hivszamlaszam` = original) | Storno query-first guard |
| 8 | C4 — create with `" ORDER "`, `"ORDER "`, `"order"`; query by each | Padded → replay; lowercase → new document; padded query → 7 | Key normalization (trim, preserve case) |

Record `szallito/id`, `teszt`, `eszamla` and the observed error headers per operation in the
deployment notes; if any expectation fails, stop and revisit the corresponding ADR before go-live.
