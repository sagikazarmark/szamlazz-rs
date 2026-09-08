# Verified szamlazz.hu Számla Agent behaviour

What the `restate-szamlazz` design relies on that szamlazz.hu does not document, observed against the
live Számla Agent. Every row below was observed on **one szamlazz.hu TEST account, on one day
(2026-09-03)** (the rows marked `P48-*` on the same account on **2026-09-06**, `P73-*` on **2026-09-07**)
on **paper invoices** (`<eszamla>1</eszamla>` on every queried document of the 2026-09-03/06 probes; the account
can issue e-invoices, and does so only when a request says `<eszamla>true</eszamla>`: the P73 rows, whose two
e-invoice originals and two e-invoice stornos are the exceptions, and by their `E-` numbering the P60 stornos; see
*Storno semantics*), every document marked
`<teszt>true</teszt>`, and the account toggle "Disable order number repetition" ON, through the
`szamlazz-agent` crate. Probe ids (`A1`, `B4`, `C2-5`, …) refer to the review record's probe findings A–D
and their raw request/response logs, which are not part of this repository; `P48-P0`…`P48-P7` are the
storno-date probes of issue #48 (supplier id 972720, 13 documents `CTEST-2026-82`…`91`, `D-CTEST-17`);
`P60-H1`…`H5`, `P60-E1`…`E3`, `P60-V1`/`V2` are the line-item rounding probes of issue #60 (2026-09-06, same
account: `CTEST-2026-92`…`99`, each stornoed as `E-CTEST-2026-1`…`8`; `CTEST-2026-92`…`97` in one run, `98`/`99`
in a follow-up); `XPRB-P1`…`P6` are the external-id uniqueness re-check of 2026-09-06 (same account:
`CTEST-2026-102`…`111`, `D-CTEST-18`; every invoice stornoed, the proforma deleted; a scratch harness on
the `szamlazz-agent` crate, outside the repository); `P73-EE`, `P73-EP`, `P73-PE`, `P73-PP` are the `eszamla`
probe of issue #73 (2026-09-07, same account; the letters are the original's and the storno request's form,
**E**-invoice or **P**aper: `E-CTEST-2026-9` → `E-CTEST-2026-10`, `E-CTEST-2026-11` → `CTEST-2026-112`,
`CTEST-2026-113` → `E-CTEST-2026-12`, `CTEST-2026-114` → `CTEST-2026-115`; reproduced the same day by two more runs,
`E-CTEST-2026-13`…`20` and `CTEST-2026-116`…`123`; the `eszamla_semantics` test of
`crates/szamlazz-agent/tests/live.rs`, which runs the same four cases on demand, asserts the observed answers and
prints them as a table). Restate runtime facts live in ADRs 0001, 0002, 0004 and 0005, not here.

Treat these as facts about *that* account. Some may depend on account settings (e-invoice, cash
accounting), and szamlazz.hu may change any of them without notice; the go-live checklist at the end
is the minimum to re-establish them on the target account.

Notation: `SZ` invoice, `D` proforma, `ES` prepayment, `VS` final, `HS` corrective, `SS` storno,
`SL` delivery note (the `<tipus>` codes). "7" etc. are `<hibakod>` values.

## Order numbers and the duplicate toggle

| Behaviour | Verified how | Design consequence |
|---|---|---|
| With the toggle ON, a second document of the same kind under an order number with different content is rejected with 152 "Már létező rendelésszám: {order}. …". The message names the order number only, never the existing invoice number; HTTP 200; headers `szlahu_error_code`/`szlahu_error` set, no `szlahu_szamlaszam`/`szlahu_id`. | A4c-2, A4d-alt, A5-price2000, A6; C1-7 | 71/152 → re-query by external id: a live document of ours → `reconciled`; not ours → `conflict{external_id_collision}`; reversed and ours, or absent → the duplicate is not ours and the order-number query names it → `conflict{duplicate_order_number}` with `existing_number` when the newest document under the order is a live document of our kind, without it otherwise; nothing under the order at all is a contradiction, logged at `warn` and settled the same way without `existing_number`; a refusal the server already gave is not re-sent for. Never `rejected` (except for correctives), never a new document. |
| The check is **per document kind**: `SZ`, `D`, `ES`, `VS`, `SL`, `HS` each accepted the same order number in sequence; a second `SL` (different price) → 152. `SZ`-vs-`SZ` → 152. | C1-1…C1-7, D4-create-sz | Cross-kind exclusivity (plain invoice vs prepayment chain) is the service's own check (`conflict{prepaid_chain}`); 71/152 is intra-kind only. |
| Correctives are exempt: an `HS` was accepted under an order already carried by its base and by five other kinds. `HS`-vs-`HS` not tested. | B7-create-corrective, C1-6 | `correct_invoice` takes no order-number hint (the live base under the order is expected) and a 71/152 its re-query cannot resolve is `rejected`, not a conflict; a new `correction_id` issues a new `HS` by contract; the external-id query is the only guard. |
| Create **trims** leading/trailing whitespace from the order number: `" PRB-C-Case "` and `"PRB-C-Case "` replayed the existing invoice; with a different price → 152 naming the *trimmed* value. | C4-3, C4-4, C4-5 | VO key and every `rendelesszam` are derived from the trimmed bytes. |
| Case is **preserved and significant**: `prb-c-case` created a second invoice next to `PRB-C-Case`; each queryable under its own spelling. | C4-2, queries | No case-fold; two spellings are two orders. |
| Query by order number is **exact**: padded values → 7; case-sensitive. | C4 queries | A padded order number would be creatable-but-unqueryable → trim is mandatory; internal whitespace of any kind, control characters, `:` and non-NFC text are rejected by `OrderKey` (untested server-side, refused rather than guessed, ADR 0002 / #64; the pre-#64 code refused only whitespace *runs*). |
| After a storno the order number is **reusable**, also by a byte-identical request. | A5-replay | A create that finds its document reversed returns `outcome: reversed`; a new document is issued only with `reissue: true`, and it becomes the newest holder of the same external id; ADR 0003, ADR 0005. |
| A corrective netting to zero does **not** free the order number: `SZ`-78 → `HS`-79 (−1270) → new `SZ` → 152. The corrected `SZ` carries no marker of correction. | C5 | Once an `HS` exists the order's invoice kind is terminally occupied; recovery needs a new order number. |
| `query --order` returns the **most recently issued document of any kind** carrying the order number: the `HS` after six kinds; the `SS` right after a storno; the reissued `SZ` after storno + reissue. | C1-q-order, B2, B3, B6, A5-q-order | The hint is a secondary signal (foreign detection, storno-number discovery for `outcome: reversed`), never the sole liveness check, that is the external-id query plus `sztornozott`. |

## Identical-request replay

| Behaviour | Verified how | Design consequence |
|---|---|---|
| A byte-identical resend seconds later returns the **same number** with `sikeres=true`; body and headers are byte-identical to the original (same `szfejguid`, same `szlahu_id`). No replay marker exists. | A4-base-1/2 | `Issued(r)` may be a replay; report it as `issued` either way. |
| Replay is ~0.7–0.95 s (one at 1.8 s); a real issue 1.8–5.5 s. | A4, C4-3 | Latency is a heuristic, never a decision input. |
| Fingerprint **includes**: the (trimmed) order number as gate, the amount (unit price 1001 → 152), the buyer name **byte-exact** (`"próba vevő kft. "`, case and trailing space, → 152). | A4d-alt, A4c-2 | Normalize the buyer name once at validation (trim + NFC) and serialize it identically on every attempt, so the replay is a stable second guard. |
| Fingerprint **excludes**: `szamlaKulsoAzon` (other external id → replay), `keltDatum` (+1 day and −1 day → replay), `megjegyzes` (comment → replay). | A4a-2, A4b-2, A4b-3, A4e-2 | Pinning `issue_date` is not a guard; the service sends it only when the caller supplies it. A replayed document's `kelt` may differ from the request. |
| Untested: due date, fulfillment date, item name/quantity/VAT, buyer address, currency, payment method; the documented "2 days" window. | - | Not relied on; only affects how permissive the replay is. |
| Replay lasts only **while the matching document is live**: after `SZ`-72 → `SS`-73, the byte-identical resend issued `SZ`-74 (new id, real-issue latency); a different-price request then got 152 because 74 holds the order number. | A5 | Replay protection ends at storno. After a reversal the external-id query is the only guard; the lookup step answers `outcome: reversed` for a reversed document and the create step is never reached without `reissue: true` (ADR 0003). |
| Re-converting a consumed proforma under the same order (different external id) → byte-identical replay of the existing `SZ`. | C2-6 | Same replay behavior via the proforma path; the replay stored no external id. |

## External ids (`szamlaKulsoAzon`)

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Not unique**: two `SZ` under different orders with the same external id → both issued, no warning. Re-checked three days later, with the same result, and extended: the same id on a `D` and then an `SZ` under two other orders → both issued (not unique **across kinds** either); the same id on an `SZ` and, in the storno request, on its own `SS` → the `SS` is issued carrying it (not unique between an original and its reversal). szamlazz.hu's documentation describes `szamlaKulsoAzon` only as "the invoice can be identified with this key by the third party system … later the invoice can be queried with this key"; it names no uniqueness rule and no duplicate code, unlike the order number, which has the account toggle and 71/152. | A3-create1/2; XPRB-P1 (`102`, `103`), XPRB-P3 (`D-CTEST-18`, `104`), XPRB-P4 (`105` → `SS` `106`) | Validate every `Found` document (`rendelesszam`, `tipus`, `teszt`), else `conflict{external_id_collision}`. The id is a tag, never a key: nothing about `{namespace}:{order}:{kind}` may assume the server refuses a second holder. |
| Query and PDF by a shared external id return the **latest** holder (last-writer-wins): 50 over 49; 74 over 72; `103` over `102`; the `SZ` `104` over the `D`; the `SS` `106` over its original `105`. | A3-query-ext, A3-pdf-ext, A5-q-ext; XPRB-P1/P3/P4 | The newest holder is exactly the document a create asks about, so `{namespace}:{order}:{kind}` needs no generation suffix; a reissue becomes the newest holder and the stornoed original stays reachable by number and via the storno's `hivszamlaszam`. |
| An external id is **reusable after its holder is reversed**: `SZ` `102` (order A, id *shared*) stornoed → a new `SZ` `108` under the same order A with the same id was issued; query by the id and by order A → `108`. | XPRB-P6 | The *Reissue* path end to end: the lookup step sees the reversed holder, the create step sends with the same id, the new document becomes the newest holder. No new id, no suffix. |
| Read-your-writes lag **≈ 0**: query by external id succeeded 771 ms after the create returned, and at +2 s, +10 s, +60 s; `pdf --external-id` works. | A1-q0/q2/q10/q60, A1-pdf | The 2-minute re-check gap is justified by in-flight requests (see Latency), not by lag. |
| A 110-character id containing `: . _ -` and a unicode id were accepted and queryable. | A2-create-long, A2-create-uni | `{namespace}:{order}:{kind}`, `…:corrective:{correction_id}` and `…:storno:{number}` fit without hashing, and 110 is the worker's **bound** (`ExternalId::MAX_LEN`, #64): the real limit is unknown, a truncated id would make every leading query answer 7, so the parts are bounded (namespace 16, order key / correction id / invoice number 40) to keep the longest shape at 109. |
| **Never echoed**: the query XML has `<rendelesszam>` but no external-id element; create responses carry none either. | A1-q-raw; every query in A–D | The id → number mapping is readable only by querying *with* the id, which is why the id must be derivable from the key alone. |
| Attaches **only on the call that creates**: a replayed create with another id (A4a), a repeat storno with a new id (B4x), a padded replay (C4) stored nothing, query by those ids → 7. | A4a-2, B4x-query-new-extid, C4 `prb-c-case-3` | A first send that lands as a replay of a pre-existing identical document is invisible by external id; only the order-number hint finds it (`conflict{foreign}`). |
| On `xmlszamlast` the external id **attaches to the `SS`**: the storno document is queryable by the storno request's id (`tipus=SS`, `hivszamlaszam` = original); the original keeps its own id. When the storno request carries the *original's* id, the `SS` takes it too and becomes the newest holder: a query by the original's id then returns the `SS`, and the original is reachable only by number (with `sztornozott=true`). | B6-storno-with-extid, B6-query-storno-extid, B6-query-orig-extid; XPRB-P4 | Storno gets a query-first guard under `{namespace}:{order}:storno:{original_number}`: its **own** id, never the original's, or the order's `…:{kind}` id would resolve to the `SS` (`tipus` mismatch → `conflict{external_id_collision}` instead of `reversed`). |
| The external id of a deleted or converted proforma → 7. | D1-query-extid, D4-query-proforma-extid, C2-5 | A proforma 7 by id means deleted *or* consumed; `get` disambiguates via the invoice's or prepayment's `hivdijbekszam`. |

## Reversal signals

| Behaviour | Verified how | Design consequence |
|---|---|---|
| `<sztornozott>` is **absent** (not `false`) before a storno; afterwards the original's `<alap>` gains exactly `<sztornozott>true</sztornozott>` (after `<teszt>`). The `SS` never carries it. Also visible when the original is fetched by its external id. | B1 diff, A5-q-72/73, B6-query-orig-extid | The lookup step detects a UI storno without any state or operator: `sztornozott == Some(true)` → `outcome: reversed`. The agent crate exposes `Option<bool>`; `None` ⇒ live. |
| The `SS` inherits `<rendelesszam>` and carries `<hivszamlaszam>` = original. `<gazdEsemAzon>` of an `SS`/`HS` equals the original's `<id>`; a converted `SZ` inherits the `D`'s id. | B1, A5-q-73, B7-query-corrective, D4-query-sz | The `storno_number` on `outcome: reversed` comes from the hint when the newest document under the order is the matching `SS`, else it is absent. `gazdEsemAzon == original.id` is an optional consistency check. |
| A storno negates the **quantity** (−1), not the unit price; `SS` totals are negative (`szamlabrutto=-1270`, `kintlevoseg=-1270`). | B1 | Storno response validation: `invoice_number ≠ requested ∧ gross_total ≤ 0`; `≤`, not `<`, so that the storno of a 0-HUF invoice (a free ticket; a new number with a gross of `0`, the negation of nothing) is a reversal and not `not_stornoable` (the pre-release audit's finding 7; the zero-total shape itself is unverified, below). |
| A storno **wipes `<kifizetesek>`** from the original (body shrank; `payments=[]`); the `SS`'s `kintlevoseg` is the full negative gross, prior credits not netted. The query XML has no `kintlevoseg` element at all. | B8 | The service does not snapshot payments; a caller that needs them queries before stornoing (`Szamlazz.Agent.query`) and re-registers on the new invoice via `set_payments`. Outstanding is observable only via response headers. |

## Storno semantics

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Repeat storno** of a stornoed invoice → `sikeres=true` echoing the **existing** `SS` (same number, same `szlahu_id`, −1270); no error code; no second `SS`; 741 ms vs 2352 ms for the real storno. | B4-repeat-storno, B4-query-order-after-repeat | Storno is idempotent per original number: re-sending is safe; the storno step is query-first on every execution under the issue policy. A re-executed step whose leading query finds the `SS` sends nothing; one that re-sends gets the echo, not a duplicate. |
| Storno of a **proforma** or a **delivery note** → `sikeres=true` echoing the *requested* number with **positive** totals; the document is unchanged (no `<sztornozott>`, no `SS`). | B5-storno-proforma, B5-storno-delivery-note | Success-shaped no-op. Validate: `invoice_number ≠ requested ∧ gross_total ≤ 0`, else `NotStornoable` → `rejected{not_stornoable}` (the echo keeps the requested number, so a zero-gross echo of a 0-HUF proforma is still the no-op). `tipus` is not in the storno response, confirm `SS` in a follow-up query. |
| Storno of an **`SS`** → 14 "Sztornó és jóváíró számlát nem lehet sem sztornózni, sem jóváírni." | B5-storno-SS | Type 14 in the crate; `rejected{14}`. |
| Storno of an invoice that **has a corrective** → 221 "Ez a számla nem sztornózható (van helyesbítő számlája)." | B7-storno-corrected-orig | The server is the guard: `rejected{221}`; type 221. |
| Storno `keltDatum` other than today → 352 "A számla kelte csak a mai nap lehet: 2026.09.03.." The reversed invoice was a paper one (`eszamla=1`, settled as paper by P73), and that session's stornos were numbered in the paper sequence (`SS`-73 between `SZ`-72 and `SZ`-74, no `E-` prefix; see the P73 rows), so the storno request was paper too: 352 is not an e-invoice rule. | B3-storno-earlier-kelt | Never send `keltDatum` on a storno; type 352. On **create** the same value is not rejected, see the next row. |
| Create `keltDatum` = yesterday → `sikeres=true`, but the issued invoice's `<kelt>` is **today**: the sent date is silently replaced, not rejected. | P48-P5 | 352 on create does not exist on this account; a pinned `issue_date` on a create is a request, not a guarantee (the replay row above already excludes it from the fingerprint). The `InvoiceHeader::issue_date` doc should say "replaced", not only "used when absent". |
| Storno with `teljesitesDatum` **omitted** → the `SS` carries the **original's `telj`** (original `telj` 2026-07-15, original `kelt` and today 2026-09-06 → `SS` `telj` 2026-07-15; `SS` `kelt` today). | P48-P1, P48-P5 | The server default is what NAV requires (the storno must repeat the original's date, ADR 0007). The worker does **not** rely on it: it sends the date explicitly, so a change in the default fails loud (a rejection) rather than silent. |
| Storno with `teljesitesDatum` **equal to the original's `telj`** → accepted silently; `SS` `telj` = that date. | P48-P2 | What `storno_invoice` and `Szamlazz.Agent.storno` send (ADR 0007). |
| Storno with `teljesitesDatum` in **another calendar month** (today vs the original's July), or **40 days in the future** → accepted silently: HTTP 200, `sikeres=true`, no error, no warning header or body element; the `SS` carries the sent date. The szamlazz.hu UI warns on a month mismatch; the Agent API does not. | P48-P3, P48-P4 | The API is no guard against a wrong storno date: the worker is, by never taking one from the caller (ADR 0007). |
| Repeat storno of a reversed invoice with a **different `teljesitesDatum`** and a **new `szamlaKulsoAzon`** → the B4 echo (same `SS` number and `szlahu_id`, −1270); the `SS`'s `telj` is unchanged and the new external id is not stored (query by it → 7). | P48-P6 | Consistent with B4 and A4a: the echo ignores the whole request but the number. |
| `<telj>` was present on every queried document: `SZ`, `SS` and `D` (a proforma created with a July `telj` carried it). The response XSD has `telj` mandatory. | P48-P0…P7 | The crate parses `telj` as `Option<Date>` leniently; an absent one is szamlazz.hu breaking its schema, answered as `unavailable` by the storno handlers (ADR 0007), never defaulted. |
| **`<eszamla>` in a queried document is the vendor annotation's code, not a flag**: an `SZ` created with `<eszamla>true</eszamla>` is queried back as **`3`** and numbered under the account's e-invoice prefix (`E-CTEST-2026-9`, `E-CTEST-2026-11`); one created with `false` is queried back as **`1`** and numbered in the paper sequence (`CTEST-2026-113`, `CTEST-2026-114`). `2` was not observed; `0` is the proforma (every `D` in A–D, P48, XPRB). So every document of the 2026-09-03/06 probes, all `eszamla=1`, was a **paper** invoice: the earlier reading of `1` as "e-invoicing enabled" was wrong. | P73-EE, P73-EP (the e-invoices); P73-PE, P73-PP (the paper ones) | `InvoiceAppearance` (`1` → `Paper`, `2`/`3` → `Electronic`) and the worker's derivation (`InvoiceDocumentExt::e_invoice`: `Paper` → `Some(false)`, `Electronic` → `Some(true)`, else `None`, which `StornoIntent::from_verified` fills with the account default) stand as published; the pre-#73 behaviour-note premise, not the crate, was the wrong side of finding A-13. The number prefix (`E-`) is the account's e-invoice prefix, a hint only. |
| **The `SS` takes the storno request's `eszamla`, not the original's**, and a mismatch is accepted silently, all four cases `sikeres=true`, no error code, no warning, no 352 (each storno also carried `teljesitesDatum` = the original's `telj`). Matching: the e-invoice `E-CTEST-2026-9` (`3`) stornoed with `eszamla=true` → `E-CTEST-2026-10`, `<eszamla>3</eszamla>`; the paper `CTEST-2026-114` (`1`) stornoed with `false` → `CTEST-2026-115`, `1`. Mismatching: the e-invoice `E-CTEST-2026-11` (`3`) stornoed with `false` → **`CTEST-2026-112`, `1`**; the paper `CTEST-2026-113` (`1`) stornoed with `true` → **`E-CTEST-2026-12`, `3`**. | P73-EE, P73-PP (matching); P73-EP, P73-PE (mismatching) | szamlazz.hu is no guard against a storno in the wrong form: the worker is: the storno handlers lift `eszamla` from the verified original (`StornoIntent::from_verified`), never from the caller (`StornoRequest` has no such field), so a reversal is issued in its original's form; the account default applies only to a document whose code the crate does not know. The P60 stornos `E-CTEST-2026-1`…`8` of the paper `CTEST-2026-92`…`99` sit under the e-invoice prefix, which P73 shows is what `eszamla=true` produces, so those were mismatched stornos accepted the same way (a deduction from the numbering; their `<eszamla>` was not queried). |

## Proformas: conversion, auto-linking, deletion

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Converting `D` → `SZ` with `dijbekeroSzamlaszam` under the **shared order number** is not a 152; the `SZ` carries `<hivdijbekszam>`. | C2-3, D4-create-sz | `options.proforma: auto` (default) passes the live proforma found under `{namespace}:{order}:proforma`. |
| After conversion the `D` is **gone**: 7 by number and by external id; delete → 335. | C2-5, D4-delete-converted, D4-query-proforma-* | `get` reports `proforma: {state: consumed, by}` when the proforma is absent under its id while the invoice or prepayment carries `hivdijbekszam`; `delete_proforma` answers `{deleted: true, reason: absent}`. |
| **Auto-linking by order number**: an `ES` issued *without* `dijbekeroSzamlaszam` under the `D`'s order shows `<hivdijbekszam>D-…</hivdijbekszam>` and the `D` became unqueryable. | C1-3, C2 | `proforma: none` is unenforceable on the invoice and the prepayment invoice alike: with a live `D` of ours under `{namespace}:{order}:proforma` the create returns `conflict{proforma_live}`; the caller deletes the proforma or lets `auto` link it. Consumption by an `ES` as well as an `SZ` is derived in `get`. Since #69 `create_prepayment` sends `dijbekeroSzamlaszam` explicitly under `auto`, like `create_invoice`; an `ES` *with* the reference is unverified (below). |
| A **second conversion** from a consumed `D`: same order → replay of the existing `SZ`; different order → a plain `SZ` with the reference **silently dropped** (no `hivdijbekszam`, own `gazdEsemAzon`). | C2-6, D4-create-sz2 | `dijbekeroSzamlaszam` is best-effort and the create response cannot reveal a dropped link. `proforma: {number}` verifies the `D` first; `get` shows the link that actually landed via `referenced_proforma`. |
| An `SZ` referencing an explicitly **deleted** `D` → success, reference silently ignored (5455 ms). | D5 | 7 on the proforma verify ⇒ `conflict{proforma_missing}`. |
| Delete: success is `<xmlszamladbkdelvalasz><sikeres>true</sikeres>` with **no** `szlahu_*` headers. A second delete → 335 "Nincs ilyen díjbekérő (vagy törölték, vagy nem is létezett)." with headers; a never-existed number → the same 335. Delete by order number works. | D1, D2 | `335 ⇒ deleted` is safe because the number was just found under our external id. |
| A credit entry on a `D` is accepted (`kintlevoseg 0`, `<kifizetesek>` readable); a **fully paid `D` deletes without any guard**, taking its payment history with it. | D3 | The paid guard is service-side (`force`); `kifizetesek` is read in the proforma lookup. |
| `<vevo>` in a query is **live partner master data** (overwritten by a later create with the same buyer name), not an at-issue snapshot; `<alap><email>` is per-document. | D6-query-bademail1 | Never compare `<vevo>` to the request; the service compares no payload: a different payload for a live document is `already_issued`. |

## Prepayment and final invoices

| Behaviour | Verified how | Design consequence |
|---|---|---|
| A `VS` issued **without** `elolegSzamlaszam` under the `ES`'s order is linked anyway: `<hivszamlaszam>` = the `ES`. | C6-2, C6-3 | Pass `elolegSzamlaszam` explicitly regardless; the server links by order number. |
| The server does **not** net the prepayment into the final: `VS` gross 1270, `kintlevoseg` 1270. | C6-2 | The caller supplies the negative prepayment line. |
| A second `VS` against a settled `ES` → 73 "A hivatkozott előlegszámla nem beazonosítható. …", with the correct number, under the same or a new order; 73 fires before any 152; headers set. | C6-4, C6-5 | The 1:1 rule is enforced via 73 → `rejected{73}`; the lookup step on `{namespace}:{order}:final` answers `already_issued` first when the final is ours; type 73. |
| References are one-directional: the settled `ES`, converted `D` and corrected `SZ` show nothing. | C3, C6-6, C5-q-78 | The relationship is read from the referencing side only: `get` derives `consumed` from the invoice's `hivdijbekszam`, the storno number from the `SS`'s `hivszamlaszam`. |

## Corrective invoices

| Behaviour | Verified how | Design consequence |
|---|---|---|
| An `HS` is accepted under the base's order number with `<hivszamlaszam>` = base, `<gazdEsemAzon>` = base id, negative totals (`szlahu_bruttovegosszeg=-1270`). | B7-create-corrective, C1-6, C5-2 | External id `{namespace}:{order}:corrective:{correction_id}`; no 71/152 path. |
| Once an `HS` exists: storno of the base → 221; an `HS` netting to zero does not free the order number (152 on the next `SZ`). | B7, C5-3 | `correct_invoice` on a reversed base → `conflict{base_reversed}`; storno of a corrected base → `rejected{221}` from the server; the order's invoice kind is terminal. |

## Credit entries (`setPayments`)

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Replace** semantics by default: 100 then 200 leaves `[200]`; `additiv=true` appends (`[200, 50]`, outstanding 1020). `szlahu_kintlevoseg` header and `<kintlevoseg>` body agree and equal gross − Σ. A replace with *zero* entries was not probed. | D7 | `set_payments` default is replace; a replace with no entries is refused by the crate before the wire (it would clear the payments; #70); never auto-retried by the run (`max_attempts(1)`). `additive: true` is **at-least-once**: a lost reply is `outcome_unknown` telling the caller to query the invoice before re-sending, and the handler's one crash retry waits `initial_interval = 2m` (past the 60 s client timeout) so it cannot re-send while the first send is in flight. |
| Five entries accepted; the query returns them in **non-submission order** (`20,40,10,30,50`). A sixth is refused by the crate before sending (server code unknown). | D7-credit-5amounts | `<kifizetesek>` order is not meaningful. |
| Credit on a **reversed** invoice → 463 "Sztornózó vagy sztornózott számlához nem tartozhat kifizetettségi információ.", body only, no headers. | D8-credit-on-reversed | Type 463; the wording implies the same code for a credit on the `SS` (untested). |

## Error codes and header presence

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Header presence is **per operation**: create (152, 73), storno (14, 221, 352) and delete-proforma (335) set `szlahu_error_code` + `szlahu_error`; query (7) and credit (463) are **body-only**. | A6, B3/B5/B7, C6, D1, D8 | The crate must always parse `<hibakod>`; never detect errors from headers alone. |
| Code 7's text, "Hiányzó adat: számla xml (ismeretlen számlaszám, rendelésszám vagy külső azonosító).", covers unknown number, order number *or* external id, and also a consumed proforma. | D1-query-*, C2-5 | 7 is "not on the query surface", not "never existed". |
| Codes 14, 73, 221, 352, 463 were not named in the crate at probe time (parsed as `Unknown`). | error.rs review | Type them; none is retryable. The classification a document-issuing caller acts on is `ErrorCode::outcome_class()` (#13): `Unknown` = 1, 55, 56 without a number and every code the crate does not know, a new code may be a refusal or a new "issued, but…" code, so the worker re-queries and, with nothing under the external id, faults `outcome_unknown` rather than answer `rejected`; typing the code is what settles it as a refusal. `DuplicateOrderNumber` = 71/152, `NotFound` = 7, `Rejected` = the rest (the credential codes included). |
| The `szlahu_id` header is the **document id** (= `alap/id` = `gazdEsemAzon`), different for every document. `<szallito>` is the **seller party** of the document: the counterpart of `<vevo>`, as szamlazz.hu's own docs define the word: in standard invoicing "the supplier issues the invoice to the buyer", and in *Megbízott számlakibocsátás* the szállító is the **megbízó**, "az a cég, akinek a nevében a számlák készülnek", whose account holds the documents (a delegate issues in it with a dedicated login; no XML field marks it, and an agent key, which belongs to an account, never a user, cannot be used for delegate calls, so a document sent with one is issued "a megbízó saját neve alatt"). The block is the seller as printed on the document (`nev` "TESZT - Cloud Community Hungary Kft.", address, tax number, bank); its `<id>` was 972720 on this account, identical on 10/10 queries on 2026-09-03 and 12/12 on 2026-09-06 (`SZ`, `SS`, `D`), and it appears **only in query bodies**: create responses have no `<szallito>`. szamlazz.hu documents the `<id>` nowhere in three documentation sections (the XSD has it `int`, mandatory, unannotated; the Adatkapcsolat sample comments every neighbour and leaves it blank); the same `szallitoTipus` names the third-party vendor on an incoming invoice; and the Adatkapcsolat re-pushes an outgoing invoice when `<bankszamla>` in its `<szallito>` block changes (editable after issuance on NAV-imported invoices), so the block is per-document state, and whether `<id>` is a stable party-record id or a per-snapshot row is unknown. Its stability across an edit of the seller data was never tested; its value on a NAV-imported (`forras = 34`) or any live-account document never seen. | C3, D9; XPRB-P1…P6 (every query); docs.szamlazz.hu (*Megbízott számlakibocsátás*, *Kimenő számlák*), 2026-09-07 | **The worker holds no account pin** (ADR 0006, account-pin amendment, 2026-09-07). `szallito/id` was an optional `supplier_id` pin from the XPRB probe until then, mandatory in the multi-account shape before that: an undocumented row id of unverified stability whose reference value could only be read off a document *through the configuration it was meant to check* (a swapped key would have pinned the wrong account's id), and whose false positive would strand every order of a pinned account. `teszt` was the `mode` pin: documented and stable, but in the query body only, like `<szallito>`, a create response (`xmlszamlavalasz`, the `szlahu_*` headers) carries neither, so neither check could fire before the first document of a fresh order was issued into whatever account the key opened. Both were dropped rather than keep a tripwire that cannot gate. Which account a key opens, and whether it is a test account, is the operator's go-live check (query a known document under each scope, read `test` and the seller block). The agent crate still parses both in full. Any text calling `szlahu_id` the supplier id, or `szallito/id` an account identity, is wrong. |
| Success headers on create/storno/credit: `szlahu_szamlaszam`, `szlahu_id`, `szlahu_kintlevoseg`, `szlahu_vevoifiokurl`, …; delete success sets none. | A1, D1, D3 | - |

## Latency

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Queries 0.66–1.0 s (10-sample median 858 ms; first of a session 4.8 s); creates 1.8–5.5 s; replays and errors 0.7–0.95 s (one replay 1.8 s); storno that creates 2.3–2.8 s, echo 0.7–1.1 s; credits 0.7–1.3 s; delete 0.7–1.0 s. | D9-lat-1…10; A, B, C, D logs | The 180 s budget of the create closure (three 60 s calls: leading query, create, re-query) stands; `inactivity_timeout 4m`, `abort_timeout 3m`. |
| One create **stalled ≥ 57 s** with no response and issued nothing (checked by order-number query); every other call returned within 0.7–5.5 s. Not re-sent. | A4d-2, A4d-q | The re-check must wait longer than client timeout (60 s) plus stall: the handlers' `initial_interval` (a crash) and the issue policy's `initial_delay` (a lost reply, re-executing the create step) are both `2m`, never below ~90 s. In code, not by convention (#61): every write handler of both services carries `initial_interval = 2m` (`Szamlazz.Agent.storno` included), pinned by the discovery test; and `WorkerConfig::validate` refuses an `issue.initial_delay` under `IssueConfig::MIN_INITIAL_DELAY`, the client's exported `REQUEST_TIMEOUT` (60 s) plus a 30 s margin, so such a deployment does not start. |
| Code 56 could not be triggered: a malformed (`nem-email-cim`) and an undeliverable buyer e-mail with `sendEmail=true` both returned plain success, `notification_delivery_failed=false`; the malformed address was stored on the document. | D6 | 56-without-number leaves the create step's outcome open → an immediate external-id re-query, then `Unconfirmed` (the run policy re-executes the step) when nothing is there, safe either way. |

## Line-item arithmetic and rounding

| Behaviour | Verified how | Design consequence |
|---|---|---|
| The `nettoErtek = nettoEgysegar × mennyiseg` check (259) has a **tolerance**: on a `2 × 1234.25 = 2468.5` / `2 × 1234.5 = 2469` HUF line, a net sent as 2469 (off by 0.5), 2470 (off by 1) and 2471 (off by 2) was accepted and stored as sent; 2474 (off by 5) and 2479 (off by 10) → 259 "A tétel nettó értéke nem megfelelő; nettó érték = nettó egységár x mennyiség. Termék: {name}." with `szlahu_error_code`/`szlahu_error` headers, nothing issued. Whether the bound is absolute (2 ≤ t < 5) or relative (0.08 % ≤ t < 0.2 % of the net) was not separated. | P60-H1…H5 | `Rounding::minor_unit` (half away from zero, so the net differs from `price × qty` by at most half a minor unit) is safely inside; a `LineItem::new` with hand-computed values off by ≥ 5 is `rejected{259}`. |
| szamlazz.hu **rounds every sent monetary value to two decimals itself, each one independently** (`100.005 → 100.01`, so at least half-up on a positive midpoint; half-even it is not), and keeps `nettoEgysegar` verbatim: an EUR line sent as `1 × 100.005` = `100.005 / 27.00135 / 127.00635` was accepted and stored as `nettoegysegar 100.005`, `netto 100.01`, `afa 27`, `brutto 127.01` (per-rate and grand totals the same; `szlahu_nettovegosszeg 100,01`). Sent as `1 × 100.004` = `100.004 / 27.00108 / 127.00508`, it stored `netto 100`, `afa 27`, **`brutto 127.01`**, a document whose stored gross ≠ net + VAT; the gross is not recomputed and 261 did not fire. | P60-E1, P60-E3 | Exact (unrounded) values are never sent by the worker: `Rounding::minor_unit` rounds the net before the VAT and derives the gross from the rounded pair, so what is sent is what is stored and the stored document is consistent. `Rounding::Exact` and `calculated_for_currency`'s non-HUF path can produce the inconsistent document above; both say so. |
| The minor-unit-rounded EUR line (`3 × 33.335` → `100.01 / 27.00 / 127.01`, net off by 0.005 from `price × qty`) was accepted and stored as sent; its storno carried `-100.01` / `-127.01`. | P60-E2 | The worker's non-HUF wire since #60. |
| `afakulcs` accepts `27.00` and `27.0` as well as `27`; every query response renders the rate as a double, `27.0`, whatever was sent. | P60-V1, P60-V2; every P60 query | `VatRate::as_wire`'s normalisation (`27.00` → `27`) is a nicety, not a necessity; it also makes a queried `27.0` round-trip as `27`. |

## Test-account caveats

| Caveat | Why it matters |
|---|---|
| Everything above is one TEST account, three days (2026-09-03: roughly 75 document-creating calls in four sessions; 2026-09-06: the 13 `P48-*` documents, the 8 `P60-*` invoices with their 8 stornos, and the 6 `XPRB-*` documents with their 5 stornos; 2026-09-07: the 4 `P73-*` invoices with their 4 stornos, three times). | Nothing here is a documented guarantee. |
| Every document is `<teszt>true</teszt>`; `szallito/id` is 972720. | Neither is compared with anything by the worker (ADR 0006, account-pin amendment); `Szamlazz.Agent.query` projects `teszt` as `test`, which is what the go-live check reads off a known document. A live account has `teszt=false`. |
| Every probe document is a **paper** invoice (`<eszamla>1</eszamla>` on all but `D`/`SL`) except the four P73 e-invoices, the two originals created with `eszamla=true` and the two stornos sent with it (`E-CTEST-2026-9`…`12`, all `3`), and, by their `E-` numbering, the eight P60 stornos (not queried); the account can issue e-invoices but no probe before P73 asked for one, and P73's two `eszamla=true` stornos (P73-EE, P73-PE) are the only e-invoice stornos whose request flag is on record. | 352 (kelt must be today) was observed on a paper storno, so it is not an e-invoice rule; whether an e-invoice storno has *additional* rules (55 "E-számla aláírása sikertelen" is an e-invoice-only code) is unverified beyond those two accepted stornos. The pre-#73 reading of this row, "e-invoicing is enabled (`eszamla=1`)", was wrong; see *Storno semantics*. |
| The test account did not produce 56 for bad addresses. | Either test accounts do not send mail or 56 is raised only on synchronous hand-off failures. |
| Other probes were issuing concurrently, so `CTEST-2026-*` numbers are not contiguous. | Irrelevant to the facts; noted so the raw logs are not misread. |

## Still unverified

- The storno of a **0-HUF invoice** (every item free): expected a new `SS` with `szamlabrutto=0`, which
  is what `gross_total ≤ 0` accepts; only negative-total stornos (B1) and positive-total echoes (B5)
  were observed. Low: a zero-gross reply that echoed the *requested* number would still be
  `not_stornoable`, and the next call's verify sees `sztornozott` either way.
- Code 56 shape (with/without a number; header form). Low: 56-with-number is a warning; without →
  `Unknown` → re-query.
- `HS`-vs-`HS` under the toggle; whether the replay applies to `HS`, `D`, `ES`, `VS` at all (only
  `SZ`-vs-`SZ`, `SL`-vs-`SL` and `SZ`-from-`D` were exercised). Moderate for correctives; the
  external-id query inside the create step is the working guard.
- Due date, fulfillment date, item fields, address, currency in the replay fingerprint; the "2 days"
  window. Partly settled by szamlazz.hu's own documentation (*Order number and duplicate checking*,
  2026-09-06): the replay requires the buyer name, the gross total, `keltDatum`, `fizetesiHataridoDatum`
  and `teljesitesDatum` to match, and the earlier invoice to be at most 2 days old, so the three dates
  are in the fingerprint and the window is 2 days. Not observed here; and the documented `keltDatum`
  clause sits oddly beside A4b (a ±1-day `keltDatum` replayed), which the P48-P5 replacement of the sent
  date by today explains. Low: the replay is no longer the primary guard.
- Whether `szallito/id` is **stable across edits of the seller data** (company name, address, bank
  account in Settings): if szamlazz.hu snapshots a new seller record per edit, documents issued before
  and after an edit would carry different ids on the same account. Cheap to settle on the test account
  (edit the seller address, issue one document, compare to 972720). **None** for the worker since the
  account-pin amendment (ADR 0006): nothing reads the value. Worth settling only if a pin on the seller
  block is ever reconsidered, and then the seller **tax number**, not this id, is the candidate.
- 352 on **create** does not exist on this account (P48-P5: the date is silently replaced by today);
  whether a live account rejects, replaces or *keeps* a non-today `keltDatum` is unverified, and whether
  an **e-invoice** create does (P48-P5 was a paper create). Low–moderate: the service does not pin
  `issue_date` unless the caller supplies it, and a caller that does must not read the response as
  confirmation of the date.
- An explicit storno **`teljesitesDatum` on a live account** (accepted on the test account on paper
  stornos, P48-P2, and on the two e-invoice stornos of P73). Moderate: `storno_invoice` and
  `Szamlazz.Agent.storno` send it on every storno (ADR 0007); a rejection would surface as
  `rejected{code}` with nothing issued, and would block stornos until addressed; go-live step 9 is the
  check.
- Whether "last" in `query --order` is by id or by `kelt` (indistinguishable while kelt must be
  today). Low: the hint is secondary.
- Server code for a sixth credit entry; credit on the `SS` itself (463 expected). Low.
- **A replacing credit-entry request with zero entries** (`additiv=false`, no `kifizetes`): the schema allows it
  (`xmlszamlakifiz.xsd` has `kifizetes` `minOccurs="0"`) and the replace semantics of D7 imply it clears the
  invoice's payments, but the call was never sent. Low: the crate refuses it before the wire
  (`RequestError::EmptyCreditEntryReplace`, #70), so "clear all credit entries" is not offered until a probe
  says what the server does.
- **A foreign-currency document without an exchange rate** (`penznem` ≠ HUF, no `arfolyamBank` / `arfolyam`):
  the schema has both optional (`xmlszamla.xsd` lines 118–119; the comment ties them to the automatic MNB rate)
  and the docs tie the rate to VAT display, so an `AAM` invoice, a proforma or a delivery note in EUR may not need
  one for VAT purposes; whether szamlazz.hu refuses, defaults to the MNB rate or issues without a rate is unverified.
  Low: the crate demands an `ExchangeRate` on every foreign-currency document of every kind
  (`RequestError::MissingExchangeRate`; case-insensitive on the currency since #70) and offers
  `ExchangeRate::automatic_mnb()` as the way through, so nothing is refused that the caller cannot send; relaxing
  the check for `D` / `SL` waits for a probe.
- Storno of `ES`/`VS`/`HS`; a new `VS` after a stornoed `VS`; an `SZ` beside a live `ES`; **a storno of a
  settled `ES`** (one with a `VS`; a 221-like refusal is plausible) and **an `SZ` beside a live `VS`** (the
  repetition toggle is per kind, so the server is not expected to refuse). Moderate: `storno_invoice`
  accepts `ES`/`VS`/`HS`, and `create_final` with `reissue: true` after a reversed `VS` sends a new one;
  the `SZ`-beside-`ES` and `SZ`-beside-`VS` cases are refused by the service before sending
  (`conflict{prepaid_chain}`; the latter from the final invoice's exclusivity row, #62), whatever the
  server would do. Go-live steps 10 and 11 are the checks.
- A second `D` after a consumed `D` (152 expected). Low: `create_proforma` looks up `…:invoice` and
  `…:prepayment` first → `conflict{order_invoiced, existing_number}` when the converting document is ours;
  when it is another channel's the order-number hint in the lookup step sees it → `conflict{foreign}`.
- An `ES` sent **with** `dijbekeroSzamlaszam` (and a `VS` with it): the XSD lists the element beside
  `elolegszamla` and `vegszamla` as independent optional elements, and the server links an `ES` to the `D`
  by order number without it (C1-3), but only the `SZ` was ever sent *with* the reference (C2-3). Since #69
  `create_prepayment` under `auto` sends it for a live `D` of ours. Expected: accepted, `hivdijbekszam` on
  the `ES` as in C1-3. Moderate: a refusal would surface as `rejected{code}` with nothing issued and block
  the `D` → `ES` flow (`none` is `conflict{proforma_live}`) until the proforma is deleted or the worker
  drops the reference again; go-live step 14 is the check.
- Whether a UI-converted `SZ` carries `rendelesszam`/`hivdijbekszam`; which e-mails a UI or Agent
  storno sends. Moderate for foreign detection (an `SZ` without `rendelesszam` is invisible to the
  hint) and the customer-facing narrative; not a safety issue.
- Internal whitespace and NFC handling of order numbers (only edge whitespace and case tested). Low:
  rejected rather than guessed, since #64 by the `OrderKey` alphabet itself (no internal whitespace of any
  kind, no `:`, NFC), not only by the prose.
- The `szamlaKulsoAzon` length limit (110 accepted; 120/160/200 untested, probe 5 of the 2026-09-06
  review). Low: the worker bounds every composition at 110 (#64).
- **Line-item rounding, what remains** (P60 settled the rest, see *Line-item arithmetic and rounding*): whether
  the 259 tolerance is absolute (between 2 and 5 HUF) or relative (between 0.08 % and 0.2 % of the net), only a
  2469 HUF base was probed; whether szamlazz.hu rounds a **0-decimal currency other than HUF** (JPY, ISK) to 0 or
  to 2 places, and whether it rounds a fractional **HUF** value at all (every HUF probe sent whole forints). Low:
  the worker's discrepancy is at most half a minor unit on every currency, and a whole number is a valid
  two-decimal value.
- **Credential codes 3, 135, 136, 164** (invalid credentials, browser session active, login blocked,
  multiple accounts): none was observed on the probe account. The worker relies on szamlazz.hu's
  documentation that they are answered **before any write** (so the attempt that sees one has sent
  nothing), and surfaces each as `TerminalError{credentials_rejected}` on every operation. Their
  header form (header + body, or body-only) is likewise assumed from the documentation; the crate
  parses `<hibakod>` either way. Moderate: were a credential code ever returned *after* a document was
  issued, the fault still says "outcome unknown" and the next call's external-id query finds it.
- **Everything on a live account** (`teszt=false`): e-mail sending, 56, 352. Go-live precondition, see
  below.
- **`<eszamla>2`**: the vendor annotation lists it beside `3` as an e-invoice; P73 saw only `3` for an
  invoice created with `eszamla=true`. Which documents (an `SS`? an incoming invoice? an older signing
  scheme?) carry `2` is unknown. Low: `InvoiceAppearance::Electronic` keeps both, and the worker treats
  both as an e-invoice.
- **By-number operations under the wrong scope** (multi-account mode, ADR 0006): what szamlazz.hu answers
  when account A's agent key queries, credits or stornos an invoice *number* that belongs to account B;
  7 (not on this account's query surface) is expected, but a shared number space or a different code is
  possible; only one account was probed. Moderate: since the account-pin amendment (ADR 0006) no handler
  compares a found document with the account; B's document reaching A's key, were szamlazz.hu to answer it,
  is acted on as A's; the right key under the right scope is the go-live check's job, and the caller records
  the scope as used per order (safety contract rule 5) precisely so that the case is never exercised. What
  is undetectable in any design is the collision: a wrong-scope request naming a number that also exists on
  the resolved account acts on *that* account's document. The worker does not rely on 7 across accounts for
  safety (7 is `not_found`), and what remains unverified is only whether the number spaces can overlap at
  all.

## Go-live checklist

Re-run these on the target szamlazz.hu account before enabling the worker, on **every** account a multi-account
deployment serves. Every step issues real,
numbered documents there; agree the test order numbers and their subsequent storno with whoever keeps
the books first. Confirm the toggle "Disable order number repetition" is ON in the account settings
before starting.

| Step | Probe | Expect | Feeds |
|---|---|---|---|
| 1 | A1: create with an external id, query by it at +0/+2/+10/+60 s; **read the `<szallito>` block** (name, tax number) | Hit every time; `<teszt>false</teszt>`; the seller is the company this account (this scope, in multi-account mode) is meant to issue for | lag ≈ 0; the right key under the right scope, live as meant; the worker checks neither (ADR 0006, account-pin amendment), this step does |
| 2 | A4-base, byte-identical resend of a create | Same number, byte-identical response | Toggle ON confirmed; replay guard works |
| 3 | A4c: resend with the buyer name changed only in case/trailing space | 152 naming the trimmed order number | Fingerprint is byte-exact on the buyer name; 152 header shape |
| 4 | A5: create → storno → byte-identical resend | A **new** invoice; order number reusable | Replay ends at storno (ADR 0003 hazard is real here too) |
| 5 | B1, query the original by number before and after the storno | `<sztornozott>true</sztornozott>` appears on the original, never on the `SS` | Reversal detection in the lookup step (`outcome: reversed`) |
| 6 | B4: repeat the storno | Echo of the existing `SS`, no error, no second `SS` | Storno re-send is safe |
| 7 | B6, storno with an external id, query by it | Returns the `SS` (`hivszamlaszam` = original) | Storno query-first guard |
| 8 | C4, create with `" ORDER "`, `"ORDER "`, `"order"`; query by each | Padded → replay; lowercase → new document; padded query → 7 | Key normalization (trim, preserve case) |
| 9 | P48-P2: create an invoice whose `teljesitesDatum` is in a **previous month**, storno it with `teljesitesDatum` = that date, query the `SS` by number | `sikeres=true`, no error; the `SS`'s `<telj>` equals the original's, its `<kelt>` is today | The storno date the worker sends is accepted on this account (ADR 0007); a rejection here blocks every storno |
| 10 | Storno of a settled `ES`: create an `ES` under a fresh order, a `VS` settling it (`elolegSzamlaszam`), then storno the `ES`; query the `VS` by number | Either a refusal (221-like, `sikeres=false`, headers set) or a new `SS` with the `VS` still live, `<sztornozott>` absent | Whether the state "`VS` live, `ES` reversed" is reachable at all, and the code if it is refused (type it); the final invoice's exclusivity row (#62) is right either way |
| 11 | `SZ` beside a live `VS`, on the step-10 order (or a fresh `ES` → `VS` pair), send a plain `SZ` under the same order number, with the toggle ON | Expected: accepted (the repetition toggle is per kind), a live `SZ` and a live `VS` on one order; record any 71/152 instead | The server does not refuse cross-kind double billing, so `exclusivity-final` (`conflict{prepaid_chain}`) is the only guard (#62); storno the `SZ` and `VS` afterwards |
| 12 | P60-H1/E1, one HUF line whose rounded net differs from `nettoEgysegar × mennyiseg` by 0.5 (`2 × 1234.25`, `nettoErtek=2469`); one EUR line sent via `LineItem::new` with a three-decimal net (`1 × 100.005`, `afaErtek=27.00135`, `bruttoErtek=127.00635`); query both by number | Both `sikeres=true`; the HUF line stored as sent; the EUR line stored as `netto 100.01`, `afa 27`, `brutto 127.01` with `nettoegysegar 100.005` | The 259 tolerance covers the worker's half-minor-unit discrepancy on this account (a 259 here means unit prices must be whole units); szamlazz.hu rounds each value to two decimals independently, so the worker's per-step `Rounding::minor_unit` is what keeps the stored document consistent |
| 13 | P60-V1: create with `<afakulcs>27.00</afakulcs>` (via `VatRate::Other("27.00")`; the crate's `Percent` renders `27`) | `sikeres=true`; the query returns `afakulcs 27.0` | `VatRate::as_wire`'s normalisation stays a nicety on this account; a rejection here means a caller sending `Other("27.00")` is `rejected` with nothing issued |
| 14 | `D` → `ES` with the reference: create a `D` under a fresh order, then an `ES` under the same order **with** `dijbekeroSzamlaszam` = the `D` (`InvoiceKind::Prepayment { proforma_number }`); query the `ES` by number and the `D` by number | `sikeres=true`; the `ES` shows `<hivdijbekszam>` = the `D`; the `D` is 7 | What `create_prepayment` sends under `auto` for a live proforma of ours (#69) is accepted on this account; a refusal here (record the code) blocks the `D` → `ES` flow (`none` is `conflict{proforma_live}`) until the proforma is deleted first |
| 15 | P73-EE / P73-PP, create one `SZ` with `<eszamla>true</eszamla>` and one with `false`; query both by number; storno each with `eszamla` **matching** the original's; query both `SS` by number (`eszamla_semantics` in `crates/szamlazz-agent/tests/live.rs` runs all four P73 cases, the two mismatching stornos too, and prints the table; on an account without the e-invoice feature it fails on the first create with the account's code, record it) | The e-invoice is `<eszamla>3</eszamla>` (or `2`), the paper one `1`; each `SS` carries its original's code; no 352 | `InvoiceAppearance` and the worker's storno `eszamla` derivation (`Paper` → `false`, `Electronic` → `true`) hold on this account, so a reversal is issued in its original's form; a `1` on the e-invoice or a `3` on the paper one means the code set differs here, stop and revisit the enum before any storno |

Record the seller name and tax number, `teszt`, the step-15 `eszamla` codes, the observed error headers per operation, the step-9 `telj`, the
step-10/11 answers, the step-12 stored EUR values and the step-14 answer in the deployment notes; if any expectation fails, stop and
revisit the corresponding ADR before go-live.
