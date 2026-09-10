# Bind reissue and deletion intent to an expected document

Status: accepted (2026-09-10, #206).

## Decision

In 0.4, an ordinary create's explicit replacement request is
`options.reissue: {"expected_number": "SZ-A"}`. Omission or null means no
reissue. Boolean values, including the legacy `false`, are malformed bodies.
`delete_proforma` requires `expected_number` alongside optional `force`.
Both numbers use the bounded `InvoiceNumber` contract; both objects are closed.
`correct_invoice` continues to offer no reissue option: a reversed corrective
answers `reversed`, and a new corrective requires a deliberate new correction id.

The caller obtains the number from the original create/reversed response or a
fresh `get` observation and records it with the logical command. Retries keep
that number. A conflict is not permission to substitute the newest number:
doing so is a new business decision. Correctives are not included in `get`;
their repeat `correct_invoice` response reports the existing number.

Ownership validation precedes the expected-number check. A collision retains
`external_id_collision`. For a reissue, an absent target or a different owned
holder (live or reversed) is `conflict{target_changed}`, with `existing_number`
when known. The expected holder live is `conflict{live}`; reversed, it may
proceed through prerequisites. Repeat the precondition at the full lookup.
The create step may send only past that same reversed number; its disappearance
does not authorize an ordinary create. Inside that step absence is
`outcome_unknown`, because the execution may follow an earlier unconfirmed send;
the stateless step cannot prove it is the first execution. It stops without
another send. Before the create step, absence remains `conflict{target_changed}`.
A replacement found by a re-executed
create step retains the existing recovery outcomes (`issued` when live,
`reversed` when reversed), with no further send.

For deletion, a different owned holder is
`{deleted: false, reason: "target_changed"}`, even with `force`. An absent or
consumed target remains `{deleted: true, reason: "absent"}`: there is nothing
under the external id to delete, not proof this command deleted a document.
The matching holder, including an unexpectedly reversed proforma, proceeds to
the existing fresh by-number identity/type/payment guard. Deletion always sends
the pinned expected number, never a subsequently discovered replacement.

After success, the retained Restate completion replays the success. After purge
or expiry, the same intent runs afresh: a reissue sees its replacement and
conflicts; deletion sees absence or conflicts with a replacement. It cannot
reconstruct historical success, and does not claim to.

## Guarantees and limits

External ids recover documents beyond journal retention. Restate's
`Idempotency-Key` deduplicates requests only while their completion is retained.
Expected-document intent prevents an old command from authorizing a mutation
of a replacement after that retention ends. Longer retention is not indefinite
idempotency. None of these observations is a vendor atomic compare-and-set:
external writers can race queries and sends. Nor does an expected number settle
an unresolved earlier send (#205).

Keep the Order stateless. Indefinite command history would cost persistent
storage, lifecycle management and a new source of truth merely to answer an old
command's historical result. An optional precondition with a legacy fallback
would leave the original bug available, so it is rejected.

## Release migration

This is a breaking 0.4 request contract. Callers omit former `reissue: false`,
replace `true` with the object naming the intended original, and add the intended
proforma number to deletion. There is no inference or automatic upgrade of
legacy commands: drain them on their original immutable deployment before
switching callers. Exceptional replay onto this release requires review of the
actual inputs and journal (ADR 0009). Closed request schemas advertise the new
shape. New `target_changed` create conflicts must be handled as a refusal to
send, never automatically retried against the number returned.

This amends ADR 0003's boolean rule and ADR 0005's order-addressed deletion.
