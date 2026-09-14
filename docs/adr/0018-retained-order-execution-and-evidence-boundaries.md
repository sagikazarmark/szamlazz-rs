# Retain Order work and make evidence boundaries explicit

Accepted 2026-09-14, architecture-review adjudication. This amends ADRs 0004–0006's
bounded prerequisite/initialization lifecycle, ADR 0012's final expected-holder
classification, and qualifies ADR 0005's newest-holder premise. Credit-entry
write protection is deferred; its existing unkeyed contract still applies.

Amendment: [ADR 0019](0019-request-response-ordinary-invoice-replay.md) defines
explicit `protected` (default) and `replay_enabled` Order execution. Its
operation-specific replay-risk and settlement rules qualify the arm-dependent
discussion below for replay-enabled execution; correctives remain protected-only.
Neither marker compatibility nor a mode change authorizes resending from an old
retained arm journal.

## Retained business execution

Exclusive Order account resolution and prerequisite reads use the handler's
invocation retry/pause policy, with **no explicit bounded run retry policy**.
An unavailable resolver, unanswered document read (including Számla Agent codes
1/55), or sanitized credential/Gateway initialization failure inside a prerequisite
run remains retryable there. Do not first journal a terminal exhausted run and then
retry the handler: replay would return that terminal completion forever.

After repair, resume the **same invocation on its pinned deployment**, with its
original input and Idempotency-Key. Before arming it can continue toward its first
send. After arming, completed-arm replay grants no permission: retained uncertainty
continues through read-only reconciliation. The single-permit protocol remains the
write boundary, including conservative markers for requests that never sent. This
decision introduces no pre-send rearming/continuation protocol.

The source boundary is `ObjectContext` in `service/support.rs::RunCtx` and
`service/prologue.rs::run_prologue`. Required `run_reading` operations retain work. Shared `get`, Agent calls,
explicit query overrides and **best-effort optional hints even within Order** keep
their bounded run policies. The dedicated `verify-recovery` run also retains bounded
`[read]` and terminal initialization failure; it does not use `run_reading`. Recovery
uses its pinned account, without re-resolution. Its infrastructure failures pause
under its invocation policy. Extending retained prerequisites to this dedicated
operator verification would require a separate lifecycle change.
Account/store calls still have ten-second deadlines; the credential fetch loop still
has three calls with 200 ms pauses (`Gone` ends that loop immediately). The operation
boundary decides whether the sanitized initialization failure is retryable or terminal.
Unknown scopes, invalid account configuration and answered non-retryable vendor codes
retain their existing domain/fault handling; this is not automatic retry of every fault.

The default mutation invocation policy is five executions with 2m→10m delays, then
pause retaining the lock; recovery is three executions with 10s→1m delays, then pause.
The host inspects effective settings and retains pause. Completed prerequisites replay
their original observations; resume does not refresh all earlier vendor facts.

### Provider intervention and request accounting

The provider's [error guidance](https://docs.szamlazz.hu/agent/basics/error-handling#retry-limit),
checked 2026-09-14, says the same request may be sent at most five times, then stop
for human intervention, and forbids retry-until-success loops. Treat pause as an
intervention point: investigate and repair before a deliberate resume, never run an
automatic resume loop or renew keys to reset budgets.

Neither run nor invocation execution thresholds are durable wire-request caps. One
closure may query several selectors; interruption can repeat an open read before
completion is recorded; a resume renews the invocation retry budget. The provider
does not define how those distinct read/reconciliation requests are grouped for the
five-attempt rule. The worker does not implement strict per-request traffic accounting
and does not claim that setting `max_attempts = 5` proves compliance. Operators must
stop repeated unsuccessful traffic and resolve its cause; deployments needing a
strict wire cap must settle the accounting scope with the provider and provide that
admission control. Delay, pause and intervention never settle an uncertain write.

## Provider newest-holder/non-regression assumption

Keep permanent external ids and accept this explicit prerequisite: **an external-id
query used to settle a later issuance does not regress to an older historical holder**.
The provider must return the newest holder, or leave the read inconclusive, rather
than present an older matching document as completion of the new intent. This is an
accepted provider-consistency assumption, **not a documented provider guarantee**.

Evidence inspected in this repository and its primary sources:

- [Recorded direct-provider observations](../szamlazz-hu-behaviour.md#external-ids-szamlakulsoazon):
  A3 returned 50 over 49; A5 returned replacement 74 over 72; XPRB-P1/P3/P4 returned
  the latest holder across orders/kinds, and XPRB-P6 returned replacement 108. These
  were one test account on September 3/6. The note explicitly says the raw A–D logs
  and scratch XPRB harness are outside the repository; retained summaries cannot be
  independently replayed as raw evidence here.
- The provider's [create XML](https://docs.szamlazz.hu/agent/generating_invoice/xml)
  and [query XML](https://docs.szamlazz.hu/agent/querying_xml/xml), checked September 14,
  promise external-id assignment on creation and later querying. The query example
  says **last invoice for order-number selection**, not monotonic external-id reads.
  The repository's [create XSD](../../fixtures/upstream/agent/xsd/xmlszamla.xsd) and
  [query XSD](../../fixtures/upstream/agent/xsd/xmlszamlaxml.xsd) specify optional string
  fields; neither supplies uniqueness, temporal ordering or non-regression semantics.
- A1's successful reads at 771 ms, +2s, +10s and +60s followed a successful reply.
  They establish neither visibility nor ordering after a lost answer or provider recovery.

**Evidence gap and accepted risk:** no retained provider experiment establishes
monotonicity across repeated reissues and failure/recovery. If A was replaced by B,
both are reversed, and an uncertain reissue of B may issue C, historical A matches
order/kind and differs from B. Current reconciliation could falsely settle on A and
clear C's marker while C can still act. Ownership plus “different from B” cannot
distinguish that case. A synthetic historical-holder test can describe the dependence; it cannot prove
the provider premise. No undocumented ordering of invoice numbers or record IDs is
assumed to fix it.

On a suspected regression, **quiesce mutations for the affected scope/Orders and
stop automatic resume**, retain original requests, returned numbers, observations
and incident evidence, and ask provider support to establish holder history and
the exact request's completion/non-execution, including delayed execution. Query
known numbers exactly to investigate; do not treat an older matching holder as new
completion or substitute it into a reissue command. If a marker remains, use the
existing authorized recovery evidence contract only once the exact operation is
settled; otherwise keep it blocked. If false settlement already cleared a marker,
the worker cannot detect or reconstruct that uncertainty: operational exclusion
must remain until the incident is settled. A deployment unable to accept this
premise must defer adoption of this reconciliation model pending stronger
provider-backed evidence/identity, rather than claim the risk is eliminated.

## Contract decisions

- Reissue's **permitted final guard** returns `TargetChanged` for absence or a
  different owned holder, whether live or reversed. It never reports that holder
  as this invocation's issuance. Ownership collision stays collision; the expected
  holder becoming live stays `live`. After a possible send, reconciliation retains
  its separate positive-evidence rules and cannot infer non-execution from absence.
- Deletion distinguishes `outcome: deleted | absent | conflict | rejected`.
  `reason` is a worker conflict; `code` and `message` are a vendor refusal. Code 335
  is reported absence, not a successful deletion acknowledgement. Unknown tokens
  stay unclassified.
- `OrderStatus.collisions` names kinds whose null slots failed ownership validation;
  a colliding proforma is never inferred consumed. Amount-only status arrays are
  `credit_entry_amounts`; full queried records remain `credit_entries`.
- `DocumentInput.paid` preserves omitted/null, false and true. Omission delegates
  to the provider default; explicit false reaches `fizetve=false`, unlike the former
  false-as-omission conversion. It is caller intent, not queried payment evidence.
- `CreateResponse` remains a permissively decoded open wire record. Its `validated()`
  known-case view requires the outcome's identity/reason/refusal payload, leaving
  optional metadata optional and unfamiliar outcomes unclassified. It validates
  completeness, not provider truth or permission to send.
- Deferred storno verification retains a reported notification-failure warning only
  when reconciliation proves the **same candidate**. A different fallback reversal
  inherits no warning. Empty warnings do not prove delivery.
- `check_account` accepts only an expected miss or a valid document as credential
  acceptance; other non-credential codes are inconclusive `Unanswered`. Document
  reads classify codes 1/55 as `Unanswered` before journaling a result. Taxpayer
  `funcCode` answers keep their existing pass-through contract.

## Scope and recovery evolution

One Order remains **one billing unit**: one live document per ordinary kind, one
prepayment/final chain, and separately identified correctives. It is not an aggregate
commercial order with arbitrary installments or split bills. The caller chooses
stable billing-unit keys and owns their association with its commercial order. See
the [adoption checklist](../../crates/restate-szamlazz/README.md#capability-and-adoption-checklist)
for conditional PDF, tax and seller requirements.

Keep recovery's **full exact-marker echo** for now. Its coupling to the closed,
versioned state schema is accepted in exchange for one exact intent comparison.
Clients retain the observed marker as opaque JSON, preserving values, fields and
integer precision, rather than reconstructing it from a subset. This is semantic
JSON echo, not byte-for-byte key ordering. Unknown versions remain inspectable and
fail closed on recovery. Future shape changes need an explicit version/decoder and
state/deployment migration decision; immutable routing alone does not isolate state
across invocations. No token-only or opaque-handle redesign is selected now.

Deploy the changed JSON and retry behavior at a new immutable endpoint; regenerate
clients and apply the [migration notes](../../crates/restate-szamlazz/README.md#architecture-review-migration).
Exceptional replay requires actual-prefix/input review under ADR 0009. Persistent
markers remain version 1; journaled outcome/diagnostic changes are not a general
cross-release replay guarantee.
