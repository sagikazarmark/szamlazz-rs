# Gateway supports expert orchestrators with caller-owned durability

Accepted by the owner on 2026-09-12 after the review of `e5fc6066bb0ce747e2f3ec83d2b13b6e9cd02e3b`:
retain `Szamlazz.Order → Gateway ← Szamlazz.Agent`, and explicitly support Gateway as an expert
orchestration building block. Ordinary applications use protected Order; custom orchestrators own
durable admission, exclusion, uncertainty retention and settlement, while sharing Gateway's vendor
interaction and intent-aware read-only evidence rules.

## Provenance and trade-off

The shared Rust module is the original architectural choice (ADR 0001; `2e717b1`, `7b6c11c`,
`f64e36a` / #22). The custom HTTP hook (`d6dce38`) deliberately supports embedders. Neither public
visibility nor that hook establishes a historical downstream custom durable orchestrator: support
for that consumer is approved **now**, not inferred retroactively from new consumer documentation.
#216 / `e41a996` superseded query-first retry safety with durable Order protection; this decision
does not restore the earlier safety assumption.

We accept a larger expert public contract and the risk that a caller can assert permission without
meeting its durable obligations. Narrowing public mutations would remove that supported use case;
routing Order through an issuing service would add another journal, retry policy, timeout coordination
and copied request data. Keeping the module also keeps request projection, answer classification
and evidence interpretation shared rather than redistributed across orchestration owners.

## Permission and evidence

`CreatePermission::grant()` is an explicit caller assertion. Consumption enforces one use of that
value, including when no send occurs; it supplies no durable authorization, arming or protection
across a restart. The caller must retain the exact intent and stable account mapping before sending,
exclude competing mutations, and never manufacture a new permission in an automatic retry/replay
closure. Order continues to own its separate acknowledged marker/arm protocol.

`CreateStepRequest::operation()` supplies the retained `WriteOperation` for
`Gateway::reconcile(ReconciliationRequest)`. This read-only interface is shared with protected Order
recovery: issuance checks order, kind, expected old reissue number and corrective base; reversal
checks the matching storno and a freshly queried, matching reversed original. An exact candidate
constrains the read; it is not silently replaced. Matching issuance may since have been reversed.
Deletion cannot be established by these queries.

The public request borrows `external_id`, `order` and retained `operation`, with an optional exact
`candidate`; it returns `Result<ReconciliationOutcome, Unanswered>`. Create always queries the
external-id holder and, if supplied, requires that holder to match the candidate. Storno queries an
explicit candidate by number; without one it queries the external id and takes the order-number
hint only on absence. A colliding holder does not trigger fallback. Automatic Order recovery may
separately invoke discovery after an inconclusive reply-candidate check; that does not relax the
public request's exact-candidate rule or operator document evidence.

Only `ReconciliationOutcome::Created` and `Reversed` establish positive completion evidence.
Absence, collisions, the old target becoming live, credential/vendor answers preventing verification,
and unanswered reads retain uncertainty. A conclusive sole-send 71/152 refusal remains data even
when optional diagnostics fail. Pre-send decisions and post-send evidence therefore have distinct
settlement rules, using shared intent predicates where applicable.

The caller durably records settlement; positive evidence is not permission for another write.
Renewal needs settlement of the exact earlier request, exclusion of delayed execution and a fresh
business decision retaining expected-document intent. Public unmanaged `Gateway::storno` keeps
its separate observed-repeat policy (ADR 0007); the shared reconciliation interface does not add
Order's send protection to it.

See the [public contract](../../crates/restate-szamlazz/README.md#gateway-and-services) and
[protected protocol](../design/order-write-protocol.md). This decision records no new vendor
observations or test executions.
