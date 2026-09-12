# Explicit storno notification recipient

Accepted 2026-09-12, #223.

Support optional `buyer_email` on both worker storno handlers. Official storno
XML and PHP document this input for paper and electronic invoices; omission has
no documented original-recipient inheritance guarantee. The controlled test-account
probe produced verified reversals in all four cases, while the operator reported
mailbox receipt only for explicit recipients. See the [source and execution
record](../research/2026-09-12-storno-email-delivery.md). Test-account routing and
unobserved account settings prevent a production delivery guarantee.

The worker supports one validated ASCII mailbox (`StornoRecipient`), preserving
case and spelling without trimming: unquoted dot-atom local part up to 64 bytes,
DNS labels up to 63 bytes each, 254 bytes overall. Lists, display names, address
literals and internationalized addresses are outside this contract. This is a
deliberately bounded worker input, not the vendor XSD's grammar or mailbox validation.
Omission/null forwards no recipient and promises neither inheritance nor suppression.
The invoice creation `send_email` defaults do not apply: storno has no such wire field.

The invocation input retains the recipient and the request-derived mutation intent
forwards it on every permitted execution. Date and appearance remain derived from
the journaled verified original (ADR 0007). Keep the version-1 unresolved marker:
it records reversal identity; recovery only queries and never sends or needs a
recipient. Adding buyer data to exposed marker state would buy no recovery capability.
The original input/journal remains subject to the deployment's retention/privacy policy.
Unmanaged Agent storno retains its existing query-first issue policy; its execution
may repeat a send after uncertainty, with the same recipient. This is no exactly-once
notification guarantee, and notification failure alone never triggers that policy.

Use the existing open `Warning` vocabulary on `StornoResponse.warnings`. When
operation-specific evidence establishes a numbered reversal with code 56, answer
`reversed` with `notification_delivery_failed`, retaining known issuance. An empty
list means no retained warning, never delivery. Read-only reconciliation and a later
already-reversed observation cannot reconstruct warning history; uncertainty and
contradictory identity retain their existing handling. Never repeat storno to recover
notification. Use the provider's notification action for the existing document.

New JSON input is optional; old requests preserve omission. Older deployments reject
the new field. Rust request/step literals need `buyer_email: None`; the constructor
defaults it. New responses add an open `warnings` array, defaulting empty when decoding
older responses. Keep unfinished invocations on their immutable deployment; exceptional
replay still requires reviewing the actual prefix and input (ADR 0009). No marker
migration or new send permission is introduced.
