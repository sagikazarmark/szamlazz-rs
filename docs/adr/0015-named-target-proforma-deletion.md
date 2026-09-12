# Explicit named-target proforma deletion

Accepted 2026-09-12, #225. A caller may know a manual or legacy proforma's number without it occupying
the worker's external-id slot. `delete_proforma` adds explicit `mode: named_target`, verifying that exact
number in the resolved account and requiring proforma type and the current Order key. Omitted mode remains
`namespace_owned`. This preserves existing ownership expectations while sharing the protected Order write
protocol; silently falling back from an empty slot would change what old commands authorize.

Both modes retain `expected_number` and the credit-entry guard (`force` bypasses that guard only). Named
mode never queries the namespace slot or order-number hint; a coexisting namespace proforma is untouched.
Wrong order/type is `target_changed`, exact-number query 7 is `absent`, and unanswered or contradictory
identity is `unavailable`. Before sending, the shared Gateway refreshes the pinned number/internal ID,
type, order and credit entries. A local association cannot replace vendor-reported order association.

## State and recovery

Keep the version-1 marker shape: `operation: {type: delete, number}` already retains all deletion recovery
intent. Its `external_id` remains the proforma slot for correlation, not proof that the number occupies it.
Adding a selection mode to state would require a schema migration without changing any recovery decision:
queries cannot prove deletion, and recovery never reselects or sends. The owning invocation retains its
mode in its input and the pinned record in its journaled verify. Existing markers remain readable; no
automatic migration is needed. Older protected deployments can still fail closed on uncertainty and
settle the exact deletion through their existing audited recovery contract.

Interruption, lost replies, pause, cancel and kill retain uncertainty. Only the acknowledged one-use
permit can send; replay cannot renew it. Absence alone cannot settle a send. Authorized recovery accepts
audited positive completion of the exact deletion or attestation that the exact request did not execute
and cannot execute later. Positive settlement is not permission to delete a different proforma.

JSON omission preserves the old behavior; new `mode` fields must reach a deployment that understands them.
Rust struct literals need the new field. Keep unfinished invocations on their original immutable deployment;
exceptional replay requires reviewing actual inputs, branch decisions and journal commands (ADR 0009).
Retain mode and number across logical retries, including after retention expiry. A different number or mode
is a new business decision after settlement, never an automatic response to a conflict or old `absent`.

## Provider evidence and limits

Primary sources checked 2026-09-12:

- [Deletion XML and XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml) explicitly show
  `<fejlec><szamlaszam>D-43</szamlaszam></fejlec>` for deleting by number, with order number as an alternative.
  No external ID, internal-record-ID condition, order/type precondition or atomic compare-and-set is offered.
  The [downloaded XSD](../../fixtures/upstream/agent/request-xsd-2026-09-11/download/xmlszamladbkdel.xsd)
  and its acquisition/checksum metadata corroborate the request shape.
- [Query XML and XSD](https://docs.szamlazz.hu/agent/querying_xml/xml) document number selection independently
  of optional external ID. Order-number selection returns the **last** invoice under that order, so it is
  not sufficient evidence for an exact recorded target.
- Existing direct-provider observations [D1–D4](../szamlazz-hu-behaviour.md#proformas-conversion-auto-linking-deletion)
  establish deletion replies, repeated/never-existing deletion code 335, paid-proforma deletion and consumed
  proforma absence by number and external ID. They do not establish atomicity or completion from absence.

No new provider guarantee is needed for legacy versus absent external IDs: selection uses the documented
number and checks reported document facts. No live mutation was performed for this change. Mocked tests
establish worker decisions and send counts, not provider atomicity, number non-reuse guarantees, or deletion
proof from an empty query. Outside writers can race verification and deletion; deployment coordination
remains necessary. `get` observes only namespace-owned documents and cannot reconstruct a legacy target's
consumption history.
