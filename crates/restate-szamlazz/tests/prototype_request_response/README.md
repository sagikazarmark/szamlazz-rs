# THROWAWAY: RequestResponse issuance (#247)

**Question:** can narrow open-run replay provide useful RequestResponse progress,
and what do we give up compared with the current arm/send protocol?

This is an executable experiment, not an approved contract or production mode.
No separate permission Virtual Object is used. No real credentials or provider
account are contacted by `request_response_prototype`. The separately selected
`vendor::vendor_overlap` **does issue test-account documents**; see
[VENDOR.md](VENDOR.md) for its actual findings and retained cleanup work. Always
use the exact test filter in the commands below rather than running every ignored
test in this target. Open [walkthrough.html](walkthrough.html) for a small
illustrative walkthrough; [observed.json](observed.json) contains runtime evidence.

## Run

From the repository root, with a Restate **1.7.8** binary:

```sh
RESTATE_SERVER_BIN=/absolute/path/to/restate-server \
PROTOTYPE_REPORT_PATH="$PWD/crates/restate-szamlazz/tests/prototype_request_response/observed.json" \
cargo test --locked -p restate-szamlazz --test prototype_request_response request_response_prototype -- --exact --ignored --nocapture
```

Omit `PROTOTYPE_REPORT_PATH` to leave the recorded evidence untouched. Use an
absolute path: Cargo runs the binary from the package directory. The server
gate launches fresh scratch storage and random loopback ports; the prototype
refuses to skip when no server binary is supplied. An assertion failure fails
the experiment. The final report is written only after successful checks and
shutdown. The test is ignored by default and its name does not select it into
the normal `e2e_` suite.

The local run used the official x86_64 Linux musl release archive with its SHA-256
verified. SDK 0.12.0, shared core 7.0.3, protocol v7, vqueues enabled.

## What actually runs

- A real Restate server and two independent SDK endpoint instances behind one
  loopback host. Successive HTTP exchanges alternate between the instances.
- Fully buffered SDK input **and output**, with discovery advertising
  `ProtocolMode::RequestResponse`. There is no streaming acknowledgement channel.
- The actual unmodified `Order.create_invoice` as the strict baseline, and actual
  `Agent.check_account` as the ordinary durable-read control.
- An experimental `NarrowPrototype.issue` shell. It reuses the real document
  builder, Számla Agent HTTP serialization/parsing and Gateway's intent-aware
  read-only reconciliation. It does **not** reuse `Gateway::create_once` to grant
  speculative replay permission contrary to that method's public contract.
- A marker before an awaited durability barrier; a fresh query before each
  executing write; `max_attempts(1)` on that write; uncertain results recorded as
  data; read-only reconciliation with pause; marker retained across kill.
- A fake provider with independently controlled query visibility, document
  count, repeat suppression, refusal and unsuccessful HTTP answers.

The narrow shell is intentionally not the full Order workflow: it omits its
prologue, prerequisites/exclusivity, full request contract, and recovery handlers.
The corrective case builds and submits real corrective XML with a named base,
but does not verify that base through the full Order prerequisite path.

## Observed results (2026-09-14)

| Scenario | Provider sends | Fake documents | Result |
|---|---:|---:|---|
| Actual strict Order, no injected failure | 0 | 0 | Pauses in reconciliation with marker retained |
| Narrow, normal RequestResponse suspensions | 1 | 1 | Completes; clears marker |
| Execution interrupted after send; document visible | 1 | 1 | Replacement finds it; completes |
| Same interruption; document invisible, no deduplication | 2 | 2 | Completes with second number; clears marker |
| Corrective, invisible first document | 2 | 2 | Same exposure, with corrective XML |
| Invisible first document, fake deduplication enabled | 2 | 1 | Completes with original number |
| Uncertain answer recorded; document invisible | 1 | 1 | Pauses; repeated resume only queries; completes when visible |
| Uncertain answer recorded; provider produced nothing | 1 | 0 | Remains paused; kill retains marker; new invocation is blocked |
| First send unrecorded, second send rejected | 2 | 1 | Refusal remains uncertain; finds first document later |
| Old execution held before send; replacement completes first | 2 | 2 | Old execution sends after completion and marker clearance |

Every completed narrow scenario is called again with its original ingress
Idempotency-Key; the recorded response replays with no extra provider send.
Send counts and document counts are independently checked. Observations include
execution/closure worker labels, markers, named journal runs and whether the
write completion exists in the server journal.

### How interruption is injected

For post-send interruption, the HTTP reply has arrived at the client, but the
write closure is held before returning its result. The host cancels and joins
the SDK response task and returns HTTP 503 without forwarding its buffered
protocol output. Restate re-executes the unfinished run. This models a lost
execution/result; it is not a provider timeout or a deliberate run-policy retry.

For overlap, the first SDK task is held **after its absent query, before send**.
The host returns 503 but deliberately retains that old task. Restate runs the
replacement; after its completion the lab releases and joins the old task.
This demonstrates that Restate journal progress cannot fence arbitrary external
work. It is controlled endpoint overlap, **not** a real multi-process crash or
Restate leader-election test. In real hosts, cancellation propagation and the
already-in-flight provider request matter independently.

The unknown-answer cases return HTTP 500; whether a document exists is controlled
separately. They model inconclusive HTTP answers, not a physical TCP response loss.

## What we learned

1. The current guard's RequestResponse liveness failure is now observed on the
   actual Order, not just inferred from SDK source.
2. Worker replacement and ordinary RequestResponse suspension are not inherently
   duplicate sends: completed results replay; fresh queries can settle visible
   writes whose completion was lost.
3. **Provider deduplication is decisive when effects remain invisible.** This
   experiment switches that behavior explicitly. Neither setting measures what
   szamlazz.hu does under concurrent requests.
4. `max_attempts(1)` does not prevent unfinished-write replay after interruption.
5. Narrow replay still has blocked, never-issued work after recorded uncertainty.
   Removing strict arming does not eliminate the need for operator recovery.
6. A later refusal cannot safely clear uncertainty about an earlier send. This
   shell conservatively treats all non-issuance write answers as uncertain;
   production would need a deliberate evidence policy for local validation,
   vendor refusals, guards, reissues and cancellation.
7. Finishing on positive evidence under narrow replay **accepts residual duplicate
   risk**: one matching document does not exclude another or a delayed old send.
   The successful response and marker clearance must not claim uniqueness.

## Decision boundary

This supports a RequestResponse-compatible narrow policy as a feasible direction,
not a decision to ship it. It neither estimates duplicate frequency nor establishes
provider atomic deduplication. Before adopting it, explicitly accept the residual
risk, define operation-specific settlement, and confirm the vendor duplicate-check
requirements. Correctives remain a distinct concern.

Not covered: workerd/WASM, signed requests/scopes, real provider concurrency,
cross-kind transitions, storno/deletion, complete recovery authorization,
multi-process/leader failover or deployment-changing replay. Those belong to
implementation validation after the policy decision, not claims of this prototype.

No production behavior was changed. The two added dev-dependencies serve only
this buffered native prototype. No permanent policy enum was introduced.
