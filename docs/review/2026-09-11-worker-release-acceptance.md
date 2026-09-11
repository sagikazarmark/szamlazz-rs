# Worker release hardening and acceptance

2026-09-11. Reviewed working-tree changes based on `5c6d5ea`.

## Changes

- Recovery candidate and issued/reversal attestation numbers use `EvidenceNumber`: exact nonblank XML 1.0
  text, without mutation-input length, whitespace or separator restrictions. Deleted targets retain the bounded
  input type and exact-marker matching. The JSON representation remains a string.
- Caller guidance distinguishes paused Order exhaustion from kill and retains uncertainty for vendor
  credit-entry refusals. Additive registration is explicitly subject to interruption-driven duplicates.
- Transport customization is scoped to direct Gateway consumers. Hosting guidance explains that SDK 0.12
  serving return is not a connection/handler task-completion barrier in an enduring runtime.

## Verification

Passed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1
```

The real-Restate run passed all 25 cases against server 1.7.8, SDK 0.12.0. Recovery regression cases now
exercise long vendor numbers containing spaces, `:` and XML metacharacters through document queries and
audited positive settlement. Recorded operator admission/evidence survives interruption and revocation;
stale targets, invalid evidence and unauthorized recovery remain refused. The fixture's literal XML needed
escaping for the new ampersand; the initial malformed fixture failed closed before being corrected.

Standards and specification reviews of the final code diff reported no remaining findings.

## Vendor-live acceptance

The operator explicitly confirmed `.env` credentials belong to the intended test-mode account with e-invoice,
EUR and order-number uniqueness configured. Credentials were loaded into the process environment without
printing them. Nextest was unavailable, so the documented serial Cargo alternative was used, with zoneinfo
enabled for Budapest civil dates. Each suite was run once, without test retries:

```sh
cargo test -p szamlazz-agent --all-features --features jiff/tzdb-zoneinfo --locked --test live -- --ignored --test-threads=1 --nocapture
cargo test -p restate-szamlazz --all-features --features jiff/tzdb-zoneinfo --locked --test live -- --ignored --test-threads=1 --nocapture
```

All five scenarios passed. Restate ran locally against the real vendor for the two worker journeys.

| Scenario | Order | Documents and cleanup |
|---|---|---|
| Paper HUF invoice, PDF, replacement/additive credit entries, repeated storno | `c29be41e-bb60-4a23-af1d-20cbeef8e39b` | `CTEST-2026-11`, reversed by `CTEST-2026-12`; repeat returned the same reversal |
| Proforma create/query/delete | `e33c3eab-de14-4667-872c-e07316969352` | `D-CTEST-13`, deleted and absence checked |
| Read-only NAV taxpayer lookup | `73a94bf1-503c-46e3-a180-c94dda040070` | Valid taxpayer with populated identity |
| Ordinary e-invoice Order, deduplication, storno and expected-number reissue | `13f0ab71-5abe-40de-8c97-3b7af737e7a8` | `D-CTEST-14` consumed; `E-CTEST-2026-35` reversed by `36`; reissue `37` reversed by cleanup `38` (same prefix/year) |
| EUR proforma/prepayment/final chain | `7e940a11-b5dc-4d1c-8f38-b19bd08e1d7a` | `D-CTEST-15` consumed; `E-CTEST-2026-39` prepayment, `40` final; cleanup reversed final with `41`, prepayment with `42` (same prefix/year) |

Representative worker invocation ids:

- Invoice: `inv_1bGO5xpE2e2C4QC9Q5WbwI1zWWB9wlWi71` (same-key replay returned the same invocation).
- Reissue: `inv_1bGO5xpE2e2C7CxINQ9pb73ODsoGqfBsUo`.
- Prepayment: `inv_14nYFpzZmSeE6X41WCH8GdvrJvq5oaIm2O`.
- Final: `inv_14nYFpzZmSeE1TcWSso2MB2KlQCkGPxxcD`.

All cleanup completed; no unanswered write was reported. These are observed business journeys, not proof
of vendor numbering beyond the tested prefixes or exactly-once effects under arbitrary delays.

## Deployment follow-up

Operator recovery was exercised through the local test host's authorization boundary with mocked vendor
evidence. No deployed host URL/operator identity was supplied. Before go-live, perform the recovery drill
through that host's actual authorization boundary and verify seller/scope mapping using its deployed resolver
and credential store. The local credential-based live run does not establish that deployed mapping.
