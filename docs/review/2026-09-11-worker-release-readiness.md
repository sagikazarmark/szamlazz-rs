# Worker release-readiness hardening

Implemented against `77d53c553c9ecdc86d5fa72ca932c636256ae807`.

## Fixes

- Complete create validation uses the Számla Agent's `to_wire` with fixed dummy
  credentials before document reads, and again after references are resolved.
  Operation/date requirements and XML representability are checked without
  fetching credentials. Invalid requests do not prepare a marker or arm a write.
- Both storno shells validate after deriving the verified original's facts.
  Validation and sending share one request builder. Defensive local-refusal
  mappings on create and storno return `invalid_input` rather than HTTP 200 rejection.
- Recovery pins journal and idempotency retention to 30 days. Discovery tests
  assert both, avoiding server 1.7.8's one-day default cap for keyed calls.
- Automatic reconciliation emits the shared structured credential warning before
  candidate fallback can replace an answer. Uncertainty remains retryable and
  read-only. The warning includes code, namespace and execution correlation,
  without copying credential messages.

## Verification

Passed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1
```

The full real-Restate run passed 30 scenarios (three library scenarios and 27
integration scenarios). The new ingress matrix covers all five create handlers
and both storno shells, asserting `invalid_input`, no arming, absent markers and
no mutation requests. Its initial run reproduced late validation before the fix.
The reconciliation warning test covers a credential answer replaced by a failing
fallback. The real-runtime logging lifecycle loses a create reply, pauses on code
135, resumes to another visible correlated warning, then resumes with positive
evidence and completes without a second send.

Standards and specification reviews reported no remaining findings in this patch.

## Deployment acceptance

Earlier validation can shorten an invalid request's command prefix. Keep retained
invocations on their immutable deployment; review actual prefixes before any
exceptional replay onto this code. The marker schema is unchanged.

The local suite exercises recovery authorization and evidence against a mock
vendor. No deployed host URL or operator access was supplied, so the production
authorization/monitoring drill and seller/scope verification remain required at
deployment. Use `docs/operations/order-recovery.md` and
`crates/restate-szamlazz/examples/verify_seller.rs` with the deployed resolver and
credential store. No vendor-live writes were performed.

Unkeyed Agent credit-entry registration retains its documented weaker guarantee;
this change does not turn it into protected per-invoice synchronization.
