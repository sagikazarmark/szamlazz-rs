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
`E-CTEST-2026-13`…`20` and `CTEST-2026-116`…`123`). Current source navigation (2026-09-11):
`scenarios::invoice_lifecycle` in [`tests/live.rs`](../crates/szamlazz-agent/tests/live.rs)
covers matching paper appearance; `electronic_original_paper_storno` and
`paper_original_electronic_storno` in [`tests/probes.rs`](../crates/szamlazz-agent/tests/probes.rs)
cover the two mismatches. These are not a four-case P73 rerun or new execution evidence.
Restate runtime facts live in ADRs 0001, 0002, 0004 and 0005, not here.

**Current design interpretation (2026-09-12):** the observations retain their original dates;
the Design consequence column describes current policy, not additional vendor evidence. External-id
queries and observed identical-request replay do not protect an uncertain create from duplication.
Protected Order consumes one durably armed permission, retains uncertainty and reconciles read-only
([protocol](design/order-write-protocol.md)). Expert Gateway consumers own that durability and use
the shared intent-aware `Gateway::reconcile` interface ([ADR 0014](adr/0014-gateway-as-an-expert-orchestration-interface.md)).
Unmanaged storno retains a separate observed-repeat policy. No delay or empty query proves a prior
request finished or cannot execute later.

**Later invoice-chain evidence:** the [2026-09-11 #218 acceptance record](testing-218-acceptance.md)
reports the explicit proforma-to-prepayment journey passing, followed by final issuance and cleanup
stornos of final then prepayment. The local chain was `D-CTEST-3` → `E-CTEST-2026-5` →
`E-CTEST-2026-6`, with cleanup `E-CTEST-2026-7`/`8`. This supplements the earlier implicit-link
observations; it establishes neither a final's explicit proforma reference nor reversal of a
prepayment while its final remains live. It is retained execution evidence, not a new run of today's source.

**Later evidence:** `CLEAR-populated` and `CLEAR-empty` below were run on an
operator-confirmed test account on **2026-09-11**. Both queried originals carried
`teszt=true`; continuity with the historical account above was not established.
Their run labels, document numbers, captured output and cleanup results are in
[the clearing evidence](research/2026-09-11-credit-clearing-live.md).

**Receipt evidence (2026-09-11):** [the dated receipt record](research/2026-09-11-receipts-live.md)
captures the first executed receipt lifecycle and automatic-MNB probes, plus
email first-send and delayed empty-block resend acknowledgements. Prefix 337
specified at most five uppercase letters/digits; new `RSPRB` was accepted without
UI registration. Immediate resend returned 153 requiring 15-second notification
spacing. All three issued originals were verified reversed. These receipt
observations are separate from the invoice observations below. The operator
confirmed both emails arrived; exact content/attachment equality and NAV
reporting were not verified.

Treat each observation as a fact about its recorded account and date. Some may depend on account settings (e-invoice, cash
accounting), and szamlazz.hu may change any of them without notice; the go-live checklist at the end
lists target-account probes; the current testing policy selects which acceptance and investigative cases to run.

Notation: `SZ` invoice, `D` proforma, `ES` prepayment, `VS` final, `HS` corrective, `SS` storno,
`SL` delivery note (the `<tipus>` codes). "7" etc. are `<hibakod>` values.

## Order numbers and the duplicate toggle

| Behaviour | Verified how | Design consequence |
|---|---|---|
| With the toggle ON, a second document of the same kind under an order number with different content is rejected with 152 "Már létező rendelésszám: {order}. …". The message names the order number only, never the existing invoice number; HTTP 200; headers `szlahu_error_code`/`szlahu_error` set, no `szlahu_szamlaszam`/`szlahu_id`. | A4c-2, A4d-alt, A5-price2000, A6; C1-7 | Protected Order retains the conclusive sole-send 71/152 as `conflict{duplicate_order_number}` (correctives: `rejected`); optional diagnostics may add `existing_number`, but a found holder never promotes the refusal to success. Public `create_once` may report a settled diagnostic holder, including `Reconciled`, using its pre-send classification rules. In both paths failed diagnostics preserve the original refusal, and no branch re-sends for it. Uncertain sends use separate intent-aware reconciliation. |
| The check is **per document kind**: `SZ`, `D`, `ES`, `VS`, `SL`, `HS` each accepted the same order number in sequence; a second `SL` (different price) → 152. `SZ`-vs-`SZ` → 152. | C1-1…C1-7, D4-create-sz | Cross-kind exclusivity (plain invoice vs prepayment chain) is the service's own check (`conflict{prepaid_chain}`); 71/152 is intra-kind only. |
| Correctives are exempt: an `HS` was accepted under an order already carried by its base and by five other kinds. `HS`-vs-`HS` not tested. | B7-create-corrective, C1-6 | `correct_invoice` takes no order-number hint (the live base under the order is expected); a conclusive sole-send 71/152 is `rejected`. A new `correction_id` names a distinct issuance intent, not permission to bypass an unresolved write. The protected marker/arm protocol prevents another send after uncertainty; external-id ownership and the intended corrective base constrain read-only settlement. |
| Create **trims** leading/trailing whitespace from the order number: `" PRB-C-Case "` and `"PRB-C-Case "` replayed the existing invoice; with a different price → 152 naming the *trimmed* value. | C4-3, C4-4, C4-5 | VO key and every `rendelesszam` are derived from the trimmed bytes. |
| Case is **preserved and significant**: `prb-c-case` created a second invoice next to `PRB-C-Case`; each queryable under its own spelling. | C4-2, queries | No case-fold; two spellings are two orders. |
| Query by order number is **exact**: padded values → 7; case-sensitive. | C4 queries | A padded order number would be creatable-but-unqueryable → trim is mandatory; internal whitespace of any kind, control characters, `:` and non-NFC text are rejected by `OrderKey` (untested server-side, refused rather than guessed, ADR 0002 / #64; the pre-#64 code refused only whitespace *runs*). |
| After a storno the order number is **reusable**, also by a byte-identical request. | A5-replay | A create that finds its document reversed returns `outcome: reversed`. Replacement requires settled prior uncertainty and `options.reissue: {expected_number}` naming that particular reversed holder; absence or a changed holder before arming is `conflict{target_changed}`. The replacement uses the same external id (ADR 0012). |
| A corrective netting to zero does **not** free the order number: `SZ`-78 → `HS`-79 (−1270) → new `SZ` → 152. The corrected `SZ` carries no marker of correction. | C5 | Once an `HS` exists the order's invoice kind is terminally occupied; recovery needs a new order number. |
| `query --order` returns the **most recently issued document of any kind** carrying the order number: the `HS` after six kinds; the `SS` right after a storno; the reissued `SZ` after storno + reissue. | C1-q-order, B2, B3, B6, A5-q-order | The hint is a secondary signal (foreign detection, storno-number discovery for `outcome: reversed`), never the sole liveness check, that is the external-id query plus `sztornozott`. |

## Identical-request replay

| Behaviour | Verified how | Design consequence |
|---|---|---|
| A byte-identical resend seconds later returns the **same number** with `sikeres=true`; body and headers are byte-identical to the original (same `szfejguid`, same `szlahu_id`). No replay marker exists. | A4-base-1/2 | `Issued(r)` may be a replay; report it as `issued` either way. |
| Replay is ~0.7–0.95 s (one at 1.8 s); a real issue 1.8–5.5 s. | A4, C4-3 | Latency is a heuristic, never a decision input. |
| Fingerprint **includes**: the (trimmed) order number as gate, the amount (unit price 1001 → 152), the buyer name **byte-exact** (`"próba vevő kft. "`, case and trailing space, → 152). | A4d-alt, A4c-2 | Buyer normalization (trim + NFC) makes request projection consistent. Observed replay is not durable protection and never authorizes repeating an uncertain create. |
| Fingerprint **excludes**: `szamlaKulsoAzon` (other external id → replay), `keltDatum` (+1 day and −1 day → replay), `megjegyzes` (comment → replay). | A4a-2, A4b-2, A4b-3, A4e-2 | Pinning `issue_date` is not a guard; the service sends it only when the caller supplies it. A replayed document's `kelt` may differ from the request. |
| Untested: due date, fulfillment date, item name/quantity/VAT, buyer address, currency, payment method; the documented "2 days" window. | - | Not relied on; only affects how permissive the replay is. |
| Replay lasts only **while the matching document is live**: after `SZ`-72 → `SS`-73, the byte-identical resend issued `SZ`-74 (new id, real-issue latency); a different-price request then got 152 because 74 holds the order number. | A5 | Observed replay ends at storno. Order requires `options.reissue: {expected_number}` and a matching reversed holder before arming; after consuming permission it retains uncertainty until settlement. An external-id query alone cannot exclude a delayed create. |
| Re-converting a consumed proforma under the same order (different external id) → byte-identical replay of the existing `SZ`. | C2-6 | Same replay behavior via the proforma path; the replay stored no external id. |

## External ids (`szamlaKulsoAzon`)

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Not unique**: two `SZ` under different orders with the same external id → both issued, no warning. Re-checked three days later, with the same result, and extended: the same id on a `D` and then an `SZ` under two other orders → both issued (not unique **across kinds** either); the same id on an `SZ` and, in the storno request, on its own `SS` → the `SS` is issued carrying it (not unique between an original and its reversal). szamlazz.hu's documentation describes `szamlaKulsoAzon` only as "the invoice can be identified with this key by the third party system … later the invoice can be queried with this key"; it names no uniqueness rule and no duplicate code, unlike the order number, which has the account toggle and 71/152. | A3-create1/2; XPRB-P1 (`102`, `103`), XPRB-P3 (`D-CTEST-18`, `104`), XPRB-P4 (`105` → `SS` `106`) | Validate ownership by `rendelesszam` and `tipus`, else `conflict{external_id_collision}` before sending; `teszt` is not an account pin (ADR 0006). Post-send reconciliation additionally checks expected issuance intent and retains uncertainty on a mismatch. Nothing about `{namespace}:{order}:{kind}` assumes the server refuses a second holder. |
| Query and PDF by a shared external id return the **latest** holder (last-writer-wins): 50 over 49; 74 over 72; `103` over `102`; the `SZ` `104` over the `D`; the `SS` `106` over its original `105`. | A3-query-ext, A3-pdf-ext, A5-q-ext; XPRB-P1/P3/P4 | The newest holder is exactly the document a create asks about, so `{namespace}:{order}:{kind}` needs no generation suffix; a reissue becomes the newest holder and the stornoed original stays reachable by number and via the storno's `hivszamlaszam`. |
| An external id is **reusable after its holder is reversed**: `SZ` `102` (order A, id *shared*) stornoed → a new `SZ` `108` under the same order A with the same id was issued; query by the id and by order A → `108`. | XPRB-P6 | The *Reissue* path end to end: the lookup step sees the reversed holder, the create step sends with the same id, the new document becomes the newest holder. No new id, no suffix. |
| Read-your-writes lag **≈ 0**: query by external id succeeded 771 ms after the create returned, and at +2 s, +10 s, +60 s; `pdf --external-id` works. | A1-q0/q2/q10/q60, A1-pdf | These reads followed a completed create reply. They bound neither visibility nor execution of a request with a lost answer; no re-check interval makes absence settlement. |
| A 110-character id containing `: . _ -` and a unicode id were accepted and queryable. | A2-create-long, A2-create-uni | `{namespace}:{order}:{kind}`, `…:corrective:{correction_id}` and `…:storno:{number}` fit without hashing, and 110 is the worker's **bound** (`ExternalId::MAX_LEN`, #64): the real limit is unknown, a truncated id would make every leading query answer 7, so the parts are bounded (namespace 16, order key / correction id / invoice number 40) to keep the longest shape at 109. |
| **Never echoed**: the query XML has `<rendelesszam>` but no external-id element; create responses carry none either. | A1-q-raw; every query in A–D | The id → number mapping is readable only by querying *with* the id, which is why the id must be derivable from the key alone. |
| Attaches **only on the call that creates**: a replayed create with another id (A4a), a repeat storno with a new id (B4x), a padded replay (C4) stored nothing, query by those ids → 7. | A4a-2, B4x-query-new-extid, C4 `prb-c-case-3` | A first send that lands as a replay of a pre-existing identical document is invisible by external id; only the order-number hint finds it (`conflict{foreign}`). |
| On `xmlszamlast` the external id **attaches to the `SS`**: the storno document is queryable by the storno request's id (`tipus=SS`, `hivszamlaszam` = original); the original keeps its own id. When the storno request carries the *original's* id, the `SS` takes it too and becomes the newest holder: a query by the original's id then returns the `SS`, and the original is reachable only by number (with `sztornozott=true`). | B6-storno-with-extid, B6-query-storno-extid, B6-query-orig-extid; XPRB-P4 | Storno discovery uses `{namespace}:{order}:storno:{original_number}`: its **own** id, never the original's, or the order's `…:{kind}` id would resolve to the `SS` (`tipus` mismatch → `conflict{external_id_collision}` instead of `reversed`). Protected reversal evidence also queries the matching original freshly; this discovery handle alone is not write protection. |
| The external id of a deleted or converted proforma → 7. | D1-query-extid, D4-query-proforma-extid, C2-5 | A proforma 7 by id means deleted *or* consumed; `get` disambiguates via the invoice's or prepayment's `hivdijbekszam`. |

## Reversal signals

| Behaviour | Verified how | Design consequence |
|---|---|---|
| `<sztornozott>` is **absent** (not `false`) before a storno; afterwards the original's `<alap>` gains exactly `<sztornozott>true</sztornozott>` (after `<teszt>`). The `SS` never carries it. Also visible when the original is fetched by its external id. | B1 diff, A5-q-72/73, B6-query-orig-extid | The lookup step detects a UI storno without any state or operator: `sztornozott == Some(true)` → `outcome: reversed`. The agent crate exposes `Option<bool>`; `None` ⇒ live. |
| The `SS` inherits `<rendelesszam>` and carries `<hivszamlaszam>` = original. `<gazdEsemAzon>` of an `SS`/`HS` equals the original's `<id>`; a converted `SZ` inherits the `D`'s id. | B1, A5-q-73, B7-query-corrective, D4-query-sz | The `storno_number` on `outcome: reversed` comes from the hint when the newest document under the order is the matching `SS`, else it is absent. `gazdEsemAzon == original.id` is an optional consistency check. |
| A storno negates the **quantity** (−1), not the unit price; `SS` totals are negative (`szamlabrutto=-1270`, `kintlevoseg=-1270`). | B1 | Reply-only reversal heuristic: `invoice_number ≠ requested ∧ gross_total ≤ 0`. Zero is an intentional comparison policy covered by synthetic controls, not evidence of live zero-total acceptance. A changed number with missing/positive gross requires queried evidence. Protected Order's evidence checks both the matching `SS` and a freshly queried, matching reversed original; unmanaged storno retains its separate type/reference verification and echo policy (ADR 0007). |
| A storno **wipes `<kifizetesek>`** from the original (body shrank; `payments=[]`); the `SS`'s `kintlevoseg` is the full negative gross, prior credits not netted. The query XML has no `kintlevoseg` element at all. | B8 | The service does not snapshot payments; a caller that needs them queries before stornoing (`Szamlazz.Agent.query`) and re-registers on the new invoice via `set_credit_entries`. Outstanding is observable only via response headers. |

## Storno semantics

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Repeat storno** of a stornoed invoice → `sikeres=true` echoing the **existing** `SS` (same number, same `szlahu_id`, −1270); no error code; no second `SS`; 741 ms vs 2352 ms for the real storno. | B4-repeat-storno, B4-query-order-after-repeat | Public unmanaged `Gateway::storno` and `Szamlazz.Agent.storno` retain the query-first observed-repeat policy (the service's issue policy). Protected Order storno instead consumes one permission and reconciles read-only after uncertainty. This observed echo is not a universal vendor guarantee or permission to repeat a protected write. |
| Storno of a **proforma** or a **delivery note** → `sikeres=true` echoing the *requested* number with **positive** totals; the document is unchanged (no `<sztornozott>`, no `SS`). | B5-storno-proforma, B5-storno-delivery-note | Unmanaged storno classifies a same-number echo as `NotStornoable` → `rejected{not_stornoable}`. Protected Order refuses those types before sending; a same-number echo against its verified stornoable original retains uncertainty. A changed number with nonpositive gross is the immediate reversal heuristic; missing/positive gross requires queried evidence, not rejection. An unnumbered acknowledgement is also inconclusive (ADR 0007). |
| Storno of an **`SS`** → 14 "Sztornó és jóváíró számlát nem lehet sem sztornózni, sem jóváírni." | B5-storno-SS | Type 14 in the crate; `rejected{14}`. |
| Storno of an invoice that **has a corrective** → 221 "Ez a számla nem sztornózható (van helyesbítő számlája)." | B7-storno-corrected-orig | The server is the guard: `rejected{221}`; type 221. |
| Storno `keltDatum` other than today → 352 "A számla kelte csak a mai nap lehet: 2026.09.03.." The reversed invoice was a paper one (`eszamla=1`, settled as paper by P73), and that session's stornos were numbered in the paper sequence (`SS`-73 between `SZ`-72 and `SZ`-74, no `E-` prefix; see the P73 rows), so the storno request was paper too: 352 is not an e-invoice rule. | B3-storno-earlier-kelt | Never send `keltDatum` on a storno; type 352. On **create** the same value is not rejected, see the next row. |
| Create `keltDatum` = yesterday → `sikeres=true`, but the issued invoice's `<kelt>` is **today**: the sent date is silently replaced, not rejected. | P48-P5 | 352 on create does not exist on this account; a pinned `issue_date` on a create is a request, not a guarantee (the replay row above already excludes it from the fingerprint). The `InvoiceHeader::issue_date` doc should say "replaced", not only "used when absent". |
| Storno with `teljesitesDatum` **omitted** → the `SS` carries the **original's `telj`** (original `telj` 2026-07-15, original `kelt` and today 2026-09-06 → `SS` `telj` 2026-07-15; `SS` `kelt` today). | P48-P1, P48-P5 | The server default is what NAV requires (the storno must repeat the original's date, ADR 0007). The worker does **not** rely on it: it sends the date explicitly, so a change in the default fails loud (a rejection) rather than silent. |
| Storno with `teljesitesDatum` **equal to the original's `telj`** → accepted silently; `SS` `telj` = that date. | P48-P2 | What `storno_invoice` and `Szamlazz.Agent.storno` send (ADR 0007). |
| Storno with `teljesitesDatum` in **another calendar month** (today vs the original's July), or **40 days in the future** → accepted silently: HTTP 200, `sikeres=true`, no error, no warning header or body element; the `SS` carries the sent date. The szamlazz.hu UI warns on a month mismatch; the Agent API does not. | P48-P3, P48-P4 | The API is no guard against a wrong storno date: the worker is, by never taking one from the caller (ADR 0007). |
| Repeat storno of a reversed invoice with a **different `teljesitesDatum`** and a **new `szamlaKulsoAzon`** → the B4 echo (same `SS` number and `szlahu_id`, −1270); the `SS`'s `telj` is unchanged and the new external id is not stored (query by it → 7). | P48-P6 | Consistent with B4 and A4a: the echo ignores the whole request but the number. |
| `<telj>` was present on every queried document: `SZ`, `SS` and `D` (a proforma created with a July `telj` carried it). The response XSD has `telj` mandatory. | P48-P0…P7 | The crate parses `telj` as `Option<Date>` leniently; an absent one is szamlazz.hu breaking its schema, answered as `unavailable` by the storno handlers (ADR 0007), never defaulted. |
| **`<eszamla>` in a queried document is the vendor annotation's code, not a flag**: an `SZ` created with `<eszamla>true</eszamla>` is queried back as **`3`** and numbered under the account's e-invoice prefix (`E-CTEST-2026-9`, `E-CTEST-2026-11`); one created with `false` is queried back as **`1`** and numbered in the paper sequence (`CTEST-2026-113`, `CTEST-2026-114`). `2` was not observed; `0` is the proforma (every `D` in A–D, P48, XPRB). So every document of the 2026-09-03/06 probes, all `eszamla=1`, was a **paper** invoice: the earlier reading of `1` as "e-invoicing enabled" was wrong. | P73-EE, P73-EP (the e-invoices); P73-PE, P73-PP (the paper ones) | `InvoiceAppearance` (`1` → `Paper`, `2`/`3` → `Electronic`) and the worker's derivation (`FoundDocument::e_invoice`: `Paper` → `Some(false)`, `Electronic` → `Some(true)`, else `None`, which `StornoIntent::from_verified` fills with the account default) stand as published; the pre-#73 behaviour-note premise, not the crate, was the wrong side of finding A-13. The number prefix (`E-`) is the account's e-invoice prefix, a hint only. |
| **The `SS` takes the storno request's `eszamla`, not the original's**, and a mismatch is accepted silently, all four cases `sikeres=true`, no error code, no warning, no 352 (each storno also carried `teljesitesDatum` = the original's `telj`). Matching: the e-invoice `E-CTEST-2026-9` (`3`) stornoed with `eszamla=true` → `E-CTEST-2026-10`, `<eszamla>3</eszamla>`; the paper `CTEST-2026-114` (`1`) stornoed with `false` → `CTEST-2026-115`, `1`. Mismatching: the e-invoice `E-CTEST-2026-11` (`3`) stornoed with `false` → **`CTEST-2026-112`, `1`**; the paper `CTEST-2026-113` (`1`) stornoed with `true` → **`E-CTEST-2026-12`, `3`**. | P73-EE, P73-PP (matching); P73-EP, P73-PE (mismatching) | szamlazz.hu is no guard against a storno in the wrong form: the worker is: the storno handlers lift `eszamla` from the verified original (`StornoIntent::from_verified`), never from the caller (`StornoRequest` has no such field), so a reversal is issued in its original's form; the account default applies only to a document whose code the crate does not know. The P60 stornos `E-CTEST-2026-1`…`8` of the paper `CTEST-2026-92`…`99` sit under the e-invoice prefix, which P73 shows is what `eszamla=true` produces, so those were mismatched stornos accepted the same way (a deduction from the numbering; their `<eszamla>` was not queried). |

## Proformas: conversion, auto-linking, deletion

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Converting `D` → `SZ` with `dijbekeroSzamlaszam` under the **shared order number** is not a 152; the `SZ` carries `<hivdijbekszam>`. | C2-3, D4-create-sz | `options.proforma: auto` (default) passes the live proforma found under `{namespace}:{order}:proforma`. |
| After conversion the `D` is **gone**: 7 by number and by external id; delete → 335. | C2-5, D4-delete-converted, D4-query-proforma-* | `get` reports `proforma: {state: consumed, by}` when the proforma is absent under its id while the invoice or prepayment carries `hivdijbekszam`; `delete_proforma` answers `{deleted: true, reason: absent}`. |
| **Auto-linking by order number**: an `ES` issued *without* `dijbekeroSzamlaszam` under the `D`'s order shows `<hivdijbekszam>D-…</hivdijbekszam>` and the `D` became unqueryable. | C1-3, C2 | `proforma: none` is unenforceable on the invoice and the prepayment invoice alike: with a live `D` of ours under `{namespace}:{order}:proforma` the create returns `conflict{proforma_live}`; the caller deletes the proforma or lets `auto` link it. Consumption by an `ES` as well as an `SZ` is derived in `get`. Since #69 `create_prepayment` sends `dijbekeroSzamlaszam` explicitly; the September 11 #218 acceptance record supplies later explicit-link journey evidence, separate from this implicit-link observation. |
| A **second conversion** from a consumed `D`: same order → replay of the existing `SZ`; different order → a plain `SZ` with the reference **silently dropped** (no `hivdijbekszam`, own `gazdEsemAzon`). | C2-6, D4-create-sz2 | `dijbekeroSzamlaszam` is best-effort and the create response cannot reveal a dropped link. `proforma: {number}` verifies the `D` first; `get` shows the link that actually landed via `referenced_proforma`. |
| An `SZ` referencing an explicitly **deleted** `D` → success, reference silently ignored (5455 ms). | D5 | 7 on the proforma verify ⇒ `conflict{proforma_missing}`. |
| Delete: success is `<xmlszamladbkdelvalasz><sikeres>true</sikeres>` with **no** `szlahu_*` headers. A second delete → 335 "Nincs ilyen díjbekérő (vagy törölték, vagy nem is létezett)." with headers; a never-existed number → the same 335. Delete by order number works. | D1, D2 | A conclusive sole-send 335 is `AlreadyGone`, not proof that this request deleted the proforma. Protected deletion checks the caller's expected number, retains its marker after a lost or inconclusive answer, and never re-sends to obtain 335. Subsequent absence cannot distinguish deletion, consumption or a hidden holder; `Gateway::reconcile` cannot settle it. Independent audited recovery is required when the acknowledgement did not settle the write (ADRs 0012–0013). |
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
| An `HS` is accepted under the base's order number with `<hivszamlaszam>` = base, `<gazdEsemAzon>` = base id, negative totals (`szlahu_bruttovegosszeg=-1270`). | B7-create-corrective, C1-6, C5-2 | External id `{namespace}:{order}:corrective:{correction_id}`; no order-number hint. A conclusive 71/152 is still handled as a rejection; the exemption observed here is not an exhaustive vendor rule. |
| Once an `HS` exists: storno of the base → 221; an `HS` netting to zero does not free the order number (152 on the next `SZ`). | B7, C5-3 | `correct_invoice` on a reversed base → `conflict{base_reversed}`; storno of a corrected base → `rejected{221}` from the server; the order's invoice kind is terminal. |

## Credit entries (`setPayments`)

| Behaviour | Verified how | Design consequence |
|---|---|---|
| **Replace** semantics by default: 100 then 200 leaves `[200]`; `additiv=true` appends (`[200, 50]`, outstanding 1020). `szlahu_kintlevoseg` header and `<kintlevoseg>` body agree and equal gross − Σ. A replace with *zero* entries was not part of D7. | D7 | `set_credit_entries` defaults to replace and refuses an empty replacement through `RegisterCreditEntry` (`RequestError::EmptyCreditEntryReplace`, #70). The Számla Agent client separately offers `ClearCreditEntries` for explicit empty replacement (later evidence below). The worker run uses `max_attempts(1)`, suppressing deliberate run retries, not crash-driven re-execution before journaling. An uncertain additive/replace/clear answer requires reconciliation before repeating; a delay alone does not settle an earlier send. |
| **Explicit empty replacement** (`additiv=false`, zero `kifizetes`) clears a queried 100 HUF entry and succeeds on a separately created already-empty invoice. Both acknowledgements returned the expected invoice number and outstanding `3136`, equal to original gross; both post-clear queries returned `[]`. Originals and cleanup stornos were verified. | CLEAR-populated / CLEAR-empty, 2026-09-11: `CTEST-2026-13` → `CTEST-2026-14`, `CTEST-2026-15` → `CTEST-2026-16`; [full evidence](research/2026-09-11-credit-clearing-live.md) | `ClearCreditEntries` has execution evidence on this test account. No raw response channels were captured and no universal number-echo guarantee follows. An uncertain clear is still reconciled before any deliberate repeat. |
| Five entries accepted; the query returns them in **non-submission order** (`20,40,10,30,50`). A sixth is refused by the crate before sending (server code unknown). | D7-credit-5amounts | `<kifizetesek>` order is not meaningful. |
| Credit on a **reversed** invoice → 463 "Sztornózó vagy sztornózott számlához nem tartozhat kifizetettségi információ.", body only, no headers. | D8-credit-on-reversed | Type 463; the wording implies the same code for a credit on the `SS` (untested). |

## Error codes and header presence

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Header presence is **per operation**: create (152, 73), storno (14, 221, 352) and delete-proforma (335) set `szlahu_error_code` + `szlahu_error`; query (7) and credit (463) are **body-only**. | A6, B3/B5/B7, C6, D1, D8 | The crate must always parse `<hibakod>`; never detect errors from headers alone. |
| Code 7's text, "Hiányzó adat: számla xml (ismeretlen számlaszám, rendelésszám vagy külső azonosító).", covers unknown number, order number *or* external id, and also a consumed proforma. | D1-query-*, C2-5 | 7 is "not on the query surface", not "never existed". |
| Codes 14, 73, 221, 352, 463 were not named in the crate at probe time (parsed as `Unknown`). | error.rs review | They are now named rejection codes. `ErrorCode::outcome_class()` distinguishes refused writes from uncertainty: 1, 55, 56 without a number and unknown codes leave the outcome open. Protected Order retains its marker and reconciles read-only/pause; an empty query is not a refusal. `DuplicateOrderNumber` = 71/152, `NotFound` = 7; credential codes are a subset of `Rejected`. Query retryability is a separate classification, never permission to re-send a create. |
| The `szlahu_id` header is the **document id** (= `alap/id` = `gazdEsemAzon`), different for every document. `<szallito>` is the **seller party** of the document: the counterpart of `<vevo>`, as szamlazz.hu's own docs define the word: in standard invoicing "the supplier issues the invoice to the buyer", and in *Megbízott számlakibocsátás* the szállító is the **megbízó**, "az a cég, akinek a nevében a számlák készülnek", whose account holds the documents (a delegate issues in it with a dedicated login; no XML field marks it, and an agent key, which belongs to an account, never a user, cannot be used for delegate calls, so a document sent with one is issued "a megbízó saját neve alatt"). The block is the seller as printed on the document (`nev` "TESZT - Cloud Community Hungary Kft.", address, tax number, bank); its `<id>` was 972720 on this account, identical on 10/10 queries on 2026-09-03 and 12/12 on 2026-09-06 (`SZ`, `SS`, `D`), and it appears **only in query bodies**: create responses have no `<szallito>`. szamlazz.hu documents the `<id>` nowhere in three documentation sections (the XSD has it `int`, mandatory, unannotated; the Adatkapcsolat sample comments every neighbour and leaves it blank); the same `szallitoTipus` names the third-party vendor on an incoming invoice; and the Adatkapcsolat re-pushes an outgoing invoice when `<bankszamla>` in its `<szallito>` block changes (editable after issuance on NAV-imported invoices), so the block is per-document state, and whether `<id>` is a stable party-record id or a per-snapshot row is unknown. Its stability across an edit of the seller data was never tested; its value on a NAV-imported (`forras = 34`) or any live-account document never seen. | C3, D9; XPRB-P1…P6 (every query); docs.szamlazz.hu (*Megbízott számlakibocsátás*, *Kimenő számlák*), 2026-09-07 | **The worker holds no account pin** (ADR 0006, account-pin amendment, 2026-09-07). `szallito/id` was an optional `supplier_id` pin from the XPRB probe until then, mandatory in the multi-account shape before that: an undocumented row id of unverified stability whose reference value could only be read off a document *through the configuration it was meant to check* (a swapped key would have pinned the wrong account's id), and whose false positive would strand every order of a pinned account. `teszt` was the `mode` pin: documented and stable, but in the query body only, like `<szallito>`, a create response (`xmlszamlavalasz`, the `szlahu_*` headers) carries neither, so neither check could fire before the first document of a fresh order was issued into whatever account the key opened. Both were dropped rather than keep a tripwire that cannot gate. Which account a key opens, and whether it is a test account, is the operator's go-live check (query a known document under each scope, read `test` and the seller block). The agent crate still parses both in full. Any text calling `szlahu_id` the supplier id, or `szallito/id` an account identity, is wrong. |
| Success headers on create/storno/credit: `szlahu_szamlaszam`, `szlahu_id`, `szlahu_kintlevoseg`, `szlahu_vevoifiokurl`, …; delete success sets none. | A1, D1, D3 | - |

## Latency

| Behaviour | Verified how | Design consequence |
|---|---|---|
| Queries 0.66–1.0 s (10-sample median 858 ms; first of a session 4.8 s); creates 1.8–5.5 s; replays and errors 0.7–0.95 s (one replay 1.8 s); storno that creates 2.3–2.8 s, echo 0.7–1.1 s; credits 0.7–1.3 s; delete 0.7–1.0 s. | D9-lat-1…10; A, B, C, D logs | Observed latencies are not deadlines or external-send bounds. Per-call timeouts, retry exhaustion and execution suspension/abort have separate meanings (ADR 0004); protected reconciliation is a separate read-only run. |
| One create **stalled ≥ 57 s** with no response; the subsequent order-number query found no document. Every other call returned within 0.7–5.5 s. Not re-sent. | A4d-2, A4d-q | The historical reading “issued nothing” exceeded that observation: an empty query cannot exclude delayed execution. The retained issue-policy delay floor (`REQUEST_TIMEOUT` plus 30 s) spaces unmanaged storno executions; it is not create retry safety. Protected Order retains uncertainty and never renews permission because a timeout or delay elapsed. |
| Code 56 could not be triggered: a malformed (`nem-email-cim`) and an undeliverable buyer e-mail with `sendEmail=true` both returned plain success, `notification_delivery_failed=false`; the malformed address was stored on the document. | D6 | 56-without-number leaves issuance uncertain. Public `create_once` performs an immediate intent-aware reconciliation read, then returns `Unconfirmed` without sufficient evidence. Protected Order journals uncertainty and continues read-only reconciliation/pause with its marker retained; neither path authorizes another create send. |

## Line-item arithmetic and rounding

| Behaviour | Verified how | Design consequence |
|---|---|---|
| The `nettoErtek = nettoEgysegar × mennyiseg` check (259) has a **tolerance**: on a `2 × 1234.25 = 2468.5` / `2 × 1234.5 = 2469` HUF line, a net sent as 2469 (off by 0.5), 2470 (off by 1) and 2471 (off by 2) was accepted and stored as sent; 2474 (off by 5) and 2479 (off by 10) → 259 "A tétel nettó értéke nem megfelelő; nettó érték = nettó egységár x mennyiség. Termék: {name}." with `szlahu_error_code`/`szlahu_error` headers, nothing issued. Whether the bound is absolute (2 ≤ t < 5) or relative (0.08 % ≤ t < 0.2 % of the net) was not separated. | P60-H1…H5 | `Rounding::minor_unit` (half away from zero, so the net differs from `price × qty` by at most half a minor unit) is safely inside; a `LineItem::new` with hand-computed values off by ≥ 5 is `rejected{259}`. |
| szamlazz.hu **rounds every sent monetary value to two decimals itself, each one independently** (`100.005 → 100.01`, so at least half-up on a positive midpoint; half-even it is not), and keeps `nettoEgysegar` verbatim: an EUR line sent as `1 × 100.005` = `100.005 / 27.00135 / 127.00635` was accepted and stored as `nettoegysegar 100.005`, `netto 100.01`, `afa 27`, `brutto 127.01` (per-rate and grand totals the same; `szlahu_nettovegosszeg 100,01`). Sent as `1 × 100.004` = `100.004 / 27.00108 / 127.00508`, it stored `netto 100`, `afa 27`, **`brutto 127.01`**, a document whose stored gross ≠ net + VAT; the gross is not recomputed and 261 did not fire. | P60-E1, P60-E3 | Exact (unrounded) values are never sent by the worker: `Rounding::minor_unit` rounds the net before the VAT and derives the gross from the rounded pair, so what is sent is what is stored and the stored document is consistent. `Rounding::Exact` can produce the inconsistent document above and says so (the 0.3 `calculated_for_currency`, whose non-HUF path was exact, is gone in 0.4). |
| The minor-unit-rounded EUR line (`3 × 33.335` → `100.01 / 27.00 / 127.01`, net off by 0.005 from `price × qty`) was accepted and stored as sent; its storno carried `-100.01` / `-127.01`. | P60-E2 | The worker's non-HUF wire since #60. |
| `afakulcs` accepts `27.00` and `27.0` as well as `27`; every query response renders the rate as a double, `27.0`, whatever was sent. | P60-V1, P60-V2; every P60 query | `VatRate::as_wire`'s normalisation (`27.00` → `27`) is a nicety, not a necessity; it also makes a queried `27.0` round-trip as `27`. |

## Test-account caveats

| Caveat | Why it matters |
|---|---|
| The original A–D/P48/P60/XPRB/P73 observations cover one TEST account, three days (2026-09-03: roughly 75 document-creating calls in four sessions; 2026-09-06: the 13 `P48-*` documents, the 8 `P60-*` invoices with their 8 stornos, and the 6 `XPRB-*` documents with their 5 stornos; 2026-09-07: the 4 `P73-*` invoices with their 4 stornos, three times). Later September 11 evidence is linked separately above; account continuity is not inferred. | Nothing here is a documented guarantee. |
| The original probes report `<teszt>true</teszt>` and `szallito/id` 972720. | Neither is compared with anything by the worker (ADR 0006, account-pin amendment). The separate go-live seller check uses the deployed resolver/store and a direct Számla Agent query to compare test mode, seller name and tax number with independent expectations; `Szamlazz.Agent.query` projects `test` but omits the seller block. |
| Every probe document is a **paper** invoice (`<eszamla>1</eszamla>` on all but `D`/`SL`) except the four P73 e-invoices, the two originals created with `eszamla=true` and the two stornos sent with it (`E-CTEST-2026-9`…`12`, all `3`), and, by their `E-` numbering, the eight P60 stornos (not queried); the account can issue e-invoices but no probe before P73 asked for one, and P73's two `eszamla=true` stornos (P73-EE, P73-PE) are the only e-invoice stornos whose request flag is on record. | 352 (kelt must be today) was observed on a paper storno, so it is not an e-invoice rule; whether an e-invoice storno has *additional* rules (55 "E-számla aláírása sikertelen" is an e-invoice-only code) is unverified beyond those two accepted stornos. The pre-#73 reading of this row, "e-invoicing is enabled (`eszamla=1`)", was wrong; see *Storno semantics*. |
| The original test account did not produce 56 for bad addresses. | The cause is unknown. Later receipt email delivery was confirmed on the September 11 test account; the absence of 56 here does not establish that test accounts cannot send mail. |
| Other probes were issuing concurrently, so `CTEST-2026-*` numbers are not contiguous. | Irrelevant to the facts; noted so the raw logs are not misread. |

## Still unverified

- The storno of a **0-HUF invoice** (every item free): expected a new `SS` with `szamlabrutto=0`, which
  is what `gross_total ≤ 0` accepts; only negative-total stornos (B1) and positive-total echoes (B5)
  were observed. A zero-gross reply echoing the *requested* number is `not_stornoable` only on
  unmanaged storno; protected Order retains uncertainty and reconciles read-only.
- Code 56 shape (with/without a number; header form). Low: 56-with-number is a warning; without →
  `Unknown` → re-query.
- `HS`-vs-`HS` under the toggle; whether the replay applies to `HS`, `D`, `ES`, `VS` at all (only
  `SZ`-vs-`SZ`, `SL`-vs-`SL` and `SZ`-from-`D` were exercised in those probes). Corrective protection
  is the marker/arm protocol with intent-aware settlement, not presumed vendor replay or an empty query.
- Due date, fulfillment date, item fields, address, currency in the replay fingerprint; the "2 days"
  window. Partly settled by szamlazz.hu's own documentation (*Order number and duplicate checking*,
  2026-09-06): the replay requires the buyer name, the gross total, `keltDatum`, `fizetesiHataridoDatum`
  and `teljesitesDatum` to match, and the earlier invoice to be at most 2 days old, so the three dates
  are in the fingerprint and the window is 2 days. Not observed here; and the documented `keltDatum`
  clause sits oddly beside A4b (a ±1-day `keltDatum` replayed), which the P48-P5 replacement of the sent
  date by today may explain. Replay is not the worker's durable write protection.
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
- **A universal credit-entry acknowledgement guarantee:** CLEAR-populated/CLEAR-empty now establish
  successful empty replacement on both states for the tested account. Whether every successful v2
  registration/clear guarantees a nonblank reported invoice number remains unresolved; the schema
  makes it optional across successes and failures. See the
  [vendor clarification](research/2026-09-11-agent-vendor-clarification.md). These probes do not settle
  concurrent-write behavior or permit repeating a clear after an uncertain answer.
- **A foreign-currency document without an exchange rate** (`penznem` ≠ HUF, no `arfolyamBank` / `arfolyam`):
  the schema has both optional (`xmlszamla.xsd` lines 118–119; the comment ties them to the automatic MNB rate)
  and the docs tie the rate to VAT display, so an `AAM` invoice, a proforma or a delivery note in EUR may not need
  one for VAT purposes; whether szamlazz.hu refuses, defaults to the MNB rate or issues without a rate is unverified.
  Low: the crate demands an `ExchangeRate` on every foreign-currency document of every kind
  (`RequestError::MissingExchangeRate`; case-insensitive on the currency since #70) and offers
  `ExchangeRate::automatic_mnb()` as the way through, so nothing is refused that the caller cannot send; relaxing
  the check for `D` / `SL` waits for a probe.
- Storno of `HS`; a new `VS` after a stornoed `VS`; an `SZ` beside a live `ES`; **a storno of a
  settled `ES`** (one with a `VS`; a 221-like refusal is plausible) and **an `SZ` beside a live `VS`** (the
  repetition toggle is per kind, so the server is not expected to refuse). Moderate: `storno_invoice`
  accepts `ES`/`VS`/`HS`, and `create_final` with `options.reissue: {expected_number}` may replace the named reversed `VS`;
  the `SZ`-beside-`ES` and `SZ`-beside-`VS` cases are refused by the service before sending
  (`conflict{prepaid_chain}`; the latter from the final invoice's exclusivity row, #62), whatever the
  server would do. The #218 record supplies later cleanup-storno evidence for `VS` then `ES`, not
  for reversing the `ES` while its `VS` is still live. Go-live steps 10 and 11 investigate those separate cases.
- A second `D` after a consumed `D` (152 expected). Low: `create_proforma` looks up `…:invoice`,
  `…:prepayment` and `…:final` first (#62) → `conflict{order_invoiced, existing_number}` when the converting document is ours;
  when it is another channel's the order-number hint in the lookup step sees it → `conflict{foreign}`.
- A `VS` sent **with** `dijbekeroSzamlaszam`: the XSD lists the element independently of the kind
  flags, but there is no retained execution evidence here for that combination. Explicit `D` → `ES`
  is no longer an entirely unverified path: the September 11 #218 acceptance record reports it passing.
  That test-account result is not a universal account guarantee or a rerun of newer assertions.
  Step 14 remains the target-account check for the explicit prepayment link.
- Whether a UI-converted `SZ` carries `rendelesszam`/`hivdijbekszam`; which e-mails a UI or Agent
  storno sends. Moderate for foreign detection (an `SZ` without `rendelesszam` is invisible to the
  hint) and the customer-facing narrative. Order protection does not exclude writes from other channels.
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
  multiple accounts): none was observed on the original probe account. Their classification as
  pre-effect refusals is documentation-based, not new execution evidence. A code on a write can
  refuse that exchange; on a reconciliation query it only blocks verification and cannot settle the
  earlier write. The service surfaces credential faults (the account probe reports rejection as data),
  while protected reconciliation retains uncertainty. Header/body shape remains unverified here;
  the crate parses `<hibakod>` either way.
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

This historical probe catalog records target-account expectations, not an instruction to execute
every probe for every release. Use [the current testing policy](testing.md#manual-vendor-live-acceptance)
to select acceptance and investigative work. The deployed resolver/seller checks remain required
per account and after credential rotation. Selected mutation probes issue numbered documents;
agree their order numbers and cleanup first and confirm the order-number repetition setting.
Any deliberate repeat below presupposes a conclusive earlier completion; it is not recovery from
an uncertain write. No probes were executed for this September 12 documentation update.

| Step | Probe | Expect | Feeds |
|---|---|---|---|
| 1 | A1: create with an external id, query by it at +0/+2/+10/+60 s; **read the `<szallito>` block** (name, tax number) | Hit every time; `<teszt>false</teszt>`; the seller is the company this account (this scope, in multi-account mode) is meant to issue for | lag ≈ 0; the right key under the right scope, live as meant; the worker checks neither (ADR 0006, account-pin amendment), this step does |
| 2 | A4-base, byte-identical resend of a completed create | Same number, byte-identical response | Observed replay on this account; not uncertain-write protection |
| 3 | A4c: resend with the buyer name changed only in case/trailing space | 152 naming the trimmed order number | Fingerprint is byte-exact on the buyer name; 152 header shape |
| 4 | A5: create → storno → byte-identical resend | A **new** invoice; order number reusable | Replay ends at storno (ADR 0003 hazard is real here too) |
| 5 | B1, query the original by number before and after the storno | `<sztornozott>true</sztornozott>` appears on the original, never on the `SS` | Reversal detection in the lookup step (`outcome: reversed`) |
| 6 | B4: repeat the completed storno | Echo of the existing `SS`, no error, no second `SS` | Re-check the unmanaged observed-repeat assumption; no universal concurrency guarantee |
| 7 | B6, storno with an external id, query by it and query the original freshly | Returns the `SS` (`hivszamlaszam` = original), and the matching original reports reversed | Storno discovery plus paired reversal evidence |
| 8 | C4, create with `" ORDER "`, `"ORDER "`, `"order"`; query by each | Padded → replay; lowercase → new document; padded query → 7 | Key normalization (trim, preserve case) |
| 9 | P48-P2: create an invoice whose `teljesitesDatum` is in a **previous month**, storno it with `teljesitesDatum` = that date, query the `SS` by number | `sikeres=true`, no error; the `SS`'s `<telj>` equals the original's, its `<kelt>` is today | The storno date the worker sends is accepted on this account (ADR 0007); a rejection here blocks every storno |
| 10 | Storno of a settled `ES`: create an `ES` under a fresh order, a `VS` settling it (`elolegSzamlaszam`), then storno the `ES`; query the `VS` by number | Either a refusal (221-like, `sikeres=false`, headers set) or a new `SS` with the `VS` still live, `<sztornozott>` absent | Whether the state "`VS` live, `ES` reversed" is reachable at all, and the code if it is refused (type it); the final invoice's exclusivity row (#62) is right either way |
| 11 | `SZ` beside a live `VS`, on the step-10 order (or a fresh `ES` → `VS` pair), send a plain `SZ` under the same order number, with the toggle ON | Expected: accepted (the repetition toggle is per kind), a live `SZ` and a live `VS` on one order; record any 71/152 instead | Order's `lookup-final` prerequisite refuses a known live final with `conflict{prepaid_chain}` (#62); the unresolved-write marker guards earlier invisible writes. Server acceptance of this particular combination remains to be observed; storno known probe documents afterwards |
| 12 | P60-H1/E1, one HUF line whose rounded net differs from `nettoEgysegar × mennyiseg` by 0.5 (`2 × 1234.25`, `nettoErtek=2469`); one EUR line sent via `LineItem::new` with a three-decimal net (`1 × 100.005`, `afaErtek=27.00135`, `bruttoErtek=127.00635`); query both by number | Both `sikeres=true`; the HUF line stored as sent; the EUR line stored as `netto 100.01`, `afa 27`, `brutto 127.01` with `nettoegysegar 100.005` | The 259 tolerance covers the worker's half-minor-unit discrepancy on this account (a 259 here means unit prices must be whole units); szamlazz.hu rounds each value to two decimals independently, so the worker's per-step `Rounding::minor_unit` is what keeps the stored document consistent |
| 13 | P60-V1: create with `<afakulcs>27.00</afakulcs>` (via `VatRate::Other("27.00")`; the crate's `Percent` renders `27`) | `sikeres=true`; the query returns `afakulcs 27.0` | `VatRate::as_wire`'s normalisation stays a nicety on this account; a rejection here means a caller sending `Other("27.00")` is `rejected` with nothing issued |
| 14 | `D` → `ES` with the reference: create a `D` under a fresh order, then an `ES` under the same order **with** `dijbekeroSzamlaszam` = the `D` (`InvoiceKind::Prepayment { proforma_number }`); query the `ES` by number and the `D` by number | `sikeres=true`; the `ES` shows `<hivdijbekszam>` = the `D`; the `D` is 7 | What `create_prepayment` sends under `auto` for a live proforma of ours (#69) is accepted on this account; a refusal here (record the code) blocks the `D` → `ES` flow (`none` is `conflict{proforma_live}`) until the proforma is deleted first |
| 15 | P73-EE / P73-PP, create one `SZ` with `<eszamla>true</eszamla>` and one with `false`; query both by number; storno each with `eszamla` **matching** the original's; query both `SS` by number. `scenarios::invoice_lifecycle` in the [Agent live suite](../crates/szamlazz-agent/tests/live.rs) covers paper; the [worker live suite](../crates/restate-szamlazz/tests/live.rs) covers the electronic original's derived storno form. The [#218 record](testing-218-acceptance.md) supplies earlier execution evidence, not proof that every newer assertion ran. The two mismatches remain separate [probes](../crates/szamlazz-agent/tests/probes.rs). | The e-invoice is `<eszamla>3</eszamla>` (or `2`), the paper one `1`; each `SS` carries its original's code; no 352 | Re-check `InvoiceAppearance` and the worker's derivation on the target account; record unexpected codes before changing the enum or mutation policy |

Record the seller name and tax number, `teszt`, the step-15 `eszamla` codes, the observed error headers per operation, the step-9 `telj`, the
step-10/11 answers, the step-12 stored EUR values and the step-14 answer in the deployment notes; if any expectation fails, stop and
revisit the corresponding ADR before go-live.
