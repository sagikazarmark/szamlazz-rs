# The storno repeats the original's fulfillment date, and the caller cannot set it

Status: accepted (#48).

`Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno` send `xmlszamlast` with `teljesitesDatum` equal
to the `telj` of the invoice they are reversing (the value the verify step already holds) on every
execution. `StornoRequest` carries no fulfillment date. A verified original without a `telj` is the
`unavailable` fault, raised after the ownership, reversed and not-stornoable answers, and nothing is sent.

## Context

Since 2026-01-01 NAV's guidance on the teljesítési dátum of módosító számlák is in force
(*A teljesítés időpontja a módosító számlán*, nav.gov.hu, 2025-11-28; Áfa tv. 169. § g), 170. § (2)).
For a corrective it must equal the original's unless the original's date was itself wrong; for a storno
(érvénytelenítő számla) there is no carve-out at all:

> Továbbá az úgynevezett érvénytelenítő számlán – ha azon van teljesítési időpont – sem lehetséges az
> eredeti számlán szereplőtől eltérő teljesítési időpontot feltüntetni.

An original that shows no separate date has, by law, `telj == kelt`; szamlazz.hu encodes that as an equal
`telj`, never as an absent one (its query response schema has `telj` mandatory). Non-compliance is a
non-blocking NAV Online Számla warning: 11401 `UNINTENDED_CANCELLATION_DELIVERY_DATE` for stornos, which
fires on a different **day** (the same-**month** rule, 11400, is for correctives) and only detects the
pattern "storno `telj` == storno `kelt` ≠ original `telj`"; a third date passes undetected.

Observed on the test account on 2026-09-06 (`docs/szamlazz-hu-behaviour.md`, storno semantics):

1. With `teljesitesDatum` **omitted**, szamlazz.hu sets the storno's `telj` to the original's `telj`
   (original `telj` in July, `kelt` today → storno `telj` in July). Today's worker is therefore compliant
   by an undocumented server default.
2. An explicit `teljesitesDatum` equal to the original's is accepted silently.
3. An explicit date in **another calendar month**, and one **in the future**, are accepted silently, no
   error, no warning header or element. The szamlazz.hu UI warns; the Agent API does not.
4. A repeat storno of an already reversed invoice echoes the existing storno and ignores the date.
5. `telj` was present on every queried document (`SZ`, `SS`, `D`).

The downstream caller (a Pretix back office) requires the storno's fulfillment date to equal the original's.

## Decision

Send the original's `telj` explicitly, always, and do not let the caller choose it.

- **Explicit rather than the server default.** Fact 1 is one account, one day, undocumented. If szamlazz.hu
  changed it, the default fails *silently*: the worker never reads the storno's `telj`, every storno would
  earn a NAV warning and the caller's requirement would break unnoticed. An explicit date fails *loudly*: a
  rejection is `rejected{code}` with nothing issued. The explicit send is also provable from the wire body,
  which the server default is not.
- **No caller override.** There is no legitimate input: NAV says the storno's date may not differ; the API
  would not catch a wrong one (fact 3); NAV's own check misses a third date. A field whose documentation
  must say "never use this" is a footgun, not flexibility. The escape hatch for an exotic case is the
  szamlazz.hu UI, which at least warns. Adding an optional field later is non-breaking; removing one is not.
- **Missing `telj` is a fault, not a fallback.** An absent `telj` is szamlazz.hu violating its own schema:
  the same class as an API code a read cannot conclude from, and answered the same way (`unavailable`,
  nothing sent, a human looks). Falling back to `kelt` would answer a legal case the wire format cannot
  express; omitting the element would act on an inconclusive check, which the design forbids. This differs
  from `eszamla`, which is lifted from the verified document with the account default as fallback: `eszamla`
  is an open code set for which the account's own default is a legitimate choice, `telj` is a fiscal fact of
  the document for which no default can be right. (The two are alike in one respect: szamlazz.hu enforces
  neither, a storno with the wrong `teljesitesDatum` and one in the wrong form are both accepted silently,
  #48 and #73, so in both the worker, not the server, is the guard.) The fault comes *after* the known
  answers, an already
  reversed or not-stornoable document without a `telj` still gets `reversed` or `rejected{not_stornoable}`
  without sending.
- **No post-storno read.** The storno document is immutable (the vendor's remedy for a wrong storno date is a
  manual technical invoice) and a storno cannot be stornoed (code 14), so a mismatch found afterwards is
  un-actionable by the worker. Fact 2 shows the equal date lands. The go-live checklist verifies it once per
  account instead.

The date is a pure function of the journaled verify result, so every re-execution of the storno step
rebuilds the same request and sends byte-identical bytes; nothing new is journaled.

## Considered options

- **Rely on the server default (send nothing).** Rejected: compliant only while an undocumented behaviour
  holds, and its failure mode is silent.
- **Send when known, omit when the original has no `telj`.** Rejected: the omit branch acts on an
  inconclusive check and relies on unknown server behaviour for a document that is already anomalous.
- **Fall back to the original's `kelt` when `telj` is absent.** Rejected: the law's "no separate date" case is
  encoded by szamlazz.hu as an equal `telj`; an absent one is not that case.
- **Optional caller override (`fulfillment_date` on `StornoRequest`).** Rejected for stornos: no legitimate
  value exists, no server-side guard exists, and NAV's check misses third dates. A same-month constraint on
  the override degenerates to "must equal the original", i.e. no override. Runner-up; revisit only with a
  concrete case NAV's guidance does not cover.
- **Verify the storno's `telj` after issuing.** Rejected: an extra read on every storno for an event the
  worker itself prevents and could not remedy.

## Consequences

- `StornoRequest` is unchanged; callers need no change and make no decision.
- A new cause of the `unavailable` fault: a verified storno original without a `telj`. Same `{code, message,
  order?, kind?, external_id?}` body as every fault; the message names the document.
- `Szamlazz.Agent.storno` checks no document type before sending (it relies on the echo), so a `telj`-less
  proforma or delivery note reaching it would be `unavailable` rather than `rejected{not_stornoable}`.
  Accepted, twice theoretical (fact 5), and noted; aligning the two handlers' pre-checks is a separate
  change if wanted.
- The explicit date is verified on the test account only (`teszt=true`; paper stornos in P48, and the two
  e-invoice stornos of P73: `eszamla=1` in a queried document is *paper*, settled by #73); storno dates are
  account-sensitive (`keltDatum` ≠ today is rejected with 352 there, observed on a paper storno, so not an
  e-invoice rule). The go-live checklist gains a step:
  storno a dated invoice on the target account, query the storno, assert its `telj` equals the original's.
- The test fixtures' documents carry a `telj`, so every verify-based test exercises the happy path; the
  fault path is a fixture without one.
