# Structured cancellation with write uncertainty

Accepted 2026-09-10 for #203. Invocation cancellation is a caller-visible fact, independent of whether an external write may have landed. Ordinary reads, account resolution and best-effort reads answer a structured `cancelled` fault (HTTP 409); a cancelled create, storno or one-shot write keeps `outcome_unknown` (HTTP 500) and carries `cause: "cancelled"`. This retains the uniform fault contract without confusing intentional cancellation with dependency unavailability. Native propagation on reads was considered; consistent structured decoding across read paths was preferred.

Cancellation is never permission to automatically retry or reissue. A cancelled read sent no write; a cancelled write must be reconciled through `get` or a by-number query before any deliberately renewed operation. Cancellation does not roll back szamlazz.hu. Killing an invocation can bypass handler mappings and remains a native/raw ingress error, not a fabricated application fault. This decision does not change the unresolved-write policy owned by #205.

## Public contract and release

`TerminalCode::Cancelled` has status 409 and is not outcome-unknown. `Fault::cause` is optional and open to future string tokens; `Fault::is_cancelled` distinguishes cancellation from infrastructure faults and returns no classification for unknown codes or causes. A cancelled write remains outcome-unknown through its code. An absent cause does not retroactively classify cancellation in an older deployment's prose.

This is a breaking behavior change for the next breaking release: read/resolver cancellation changes from 503 to 409, best-effort cancellation changes from native text to structured JSON, and writes gain a machine-readable cause. Older open-code callers preserve `cancelled` as unknown and must use the actual HTTP status. Existing stored completions retain their old shape; immutable deployments follow ADR 0009. No journal sequence or retry policy changes are required.

The SDK-generated ingress client needs an error decoder in addition to its success-output types: inspect actual HTTP status and `x-restate-error-source`, then decode the Restate envelope and attempt its inner `Fault`. Preserve native cancellation, kill messages, non-invocation errors and unfamiliar shapes as raw errors. Deriving `JsonSchema` for `Fault` does not include it in the success-output discovery schema.

## Agreed verification seams

Public fault serialization/classification, generated-client error decoding, and real Restate ingress cancellation of resolver reads, ordinary reads, best-effort reads, create/storno and one-shot writes. Tests assert actual status/envelope behavior and possible landed effects, not message parsing as a classifier.
