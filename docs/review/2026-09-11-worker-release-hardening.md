# Worker release-review fixes

Implemented against `eec57fcf3036d93cd68c9cfc017338cd3020e7dd` on 2026-09-11.

## Changes

- Protected corrective creation checks the marker's base when the armed leading query first finds a
  holder. Missing or wrong base returns `external_id_collision` without sending. The same predicate in
  post-send reconciliation retains uncertainty until matching evidence appears.
- `OrderKey` and mutation `InvoiceNumber` require XML 1.0 text, including rejection of U+FFFE/U+FFFF.
  Invalid identities fail as `invalid_input` before the prologue. Discovery documents the number constraint.
- `get` classifies each answered fault before starting another read, preserving credential rejection,
  its warning and vendor code instead of allowing a later outage to hide them.
- README, rustdoc and design guidance distinguish protected Order permission/reconciliation from
  unmanaged Agent storno's issue run policy and unkeyed credit-entry uncertainty.

The existing-target ownership lookup retains its idempotency contract. The marker schema is unchanged.
Early fault returns change `get` command prefixes; keep existing invocations on their immutable deployment
and review actual prefixes before exceptional replay.

## Verification

All passed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1
RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test -p restate-szamlazz --all-features --locked --test e2e e2e_release_ -- --ignored --test-threads=1
```

The full real-Restate run passed 28 scenarios. Final review then added the negative post-send corrective
scenario; the focused run passed all four new regressions, giving 29 distinct passing real-Restate scenarios
across these runs. The final workspace test run includes doctests. Initial regressions reproduced the three
code findings before correction; test-fixture and schema-expectation corrections were completed before the
final passing runs. Final standards and specification reviews reported no remaining findings in this scope.

No vendor-live writes were performed. The changes add local validation and evidence classification rather
than changing ordinary emitted business requests.

## Deployment acceptance still required

No deployed host URL or operator access was supplied. Before production use, run the recovery drill through
that host's actual authorization boundary and verify seller/scope mapping with the deployed resolver and
credential store. Follow [order recovery](../operations/order-recovery.md) and `examples/verify_seller.rs`.
The local mocked-vendor suite does not establish those deployment facts.

Operation-specific guarantees remain important: protected Order mutations retain uncertainty and guard
later writes; unmanaged Agent storno relies on vendor idempotence; interrupted Agent credit-entry registration
can repeat an additive entry or an older replacement. The latter is not a protected per-invoice protocol.
