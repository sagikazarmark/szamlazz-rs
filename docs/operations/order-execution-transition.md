# Selecting and changing Order execution

`WorkerConfig.order_execution` is deployment configuration, never caller input,
account configuration, SDK protocol negotiation, or a native/WASM compile default.

| Setting | Writes | Hosting |
|---|---|---|
| `protected` (default) | Existing acknowledged, execution-local send permission; all Order handlers including corrective issuance | Bidirectional execution for normal write progress |
| `replay_enabled` | Fresh checks before unfinished/unrecorded resubmission; all current Order writes except corrective issuance | RequestResponse or bidirectional execution, on native or Workers |

The mode does not affect Agent handlers. Both modes retain recorded Order
uncertainty, support exact-marker operator recovery and block new mutations behind
any unresolved marker. An empty query, timeout, cancellation or kill is never
settlement. Replay-enabled positive completion does not prove uniqueness or exclude
delayed old execution. Confirm provider per-type duplicate-order checking on every
resolved account; `check_account` does not verify it. Storno/deletion and unkeyed
Agent writes retain their operation-specific evidence and retry rules in the
[outcome table](../design/request-response-outcomes.md).

## Fresh deployment

1. Choose the namespace and append-only scope/account mapping. Confirm the actual
   deployed resolver/store mapping using the independent seller verification
   procedure, and confirm provider duplicate-order settings.
2. Set `order_execution` explicitly when selecting replay-enabled execution. Existing
   omitted configurations and `WorkerConfig::new` remain protected. Validate config.
3. Build a new immutable endpoint. Register SDK request-identity public keys and
   keep scope selection behind the authenticated ingress gateway. The Workers
   example selects replay-enabled execution without `test-util`.
4. Register the endpoint with Restate and run `check_account` under each scope.
   Confirm returned scope/account/namespace and permitted operations before admitting
   work. Correctives are rejected in replay-enabled execution before resolution or
   provider I/O; use a separately reviewed supported process for corrections.

## Changing mode for an existing service

Changing only a config file is a protocol change. Do not overwrite the endpoint
of a retained deployment or resume/restart old invocations on a different mode.
The mode is not a mutable per-invocation toggle or a credential-rotation mechanism.
Every release needs an immutable endpoint, even when the mode stays the same.
For Workers use release-specific Worker names/URLs and keep each old Worker's code
and configuration available; a Restate deployment pin cannot protect a URL whose
Worker is replaced in place.

1. **Keep the old code and configuration available.** Preserve its immutable URI,
   signing configuration and credential references for retained invocations. Deploy
   candidate code at a new URI; do not yet direct producers to it.
2. **Quiesce every producer.** Block new billing mutations at the authenticated
   gateway while keeping authorized observation/recovery available. Stop internal
   SDK callers too, and inspect already-enqueued/delayed sends. Service privacy
   alone does not stop internal callers or detached delayed work.
3. **Settle old work under its old contract.** Observe unfinished invocations and
   unresolved markers across all scopes. Repair/resume on the original deployment
   or use [audited recovery](order-recovery.md) after deliberately stopping the owner.
   For protected writes replay grants no fresh permission; for replay-enabled writes
   an unfinished unrecorded run can resubmit after fresh checks. Recorded uncertainty
   stays read-only in both. Kill releases a lock, not a marker or external effect.
4. **Inventory after quiescence and settlement.** Run:

   ```sh
   cargo xtask check-order-migration --admin-url "$RESTATE_ADMIN_URL"
   ```

   Exit 1 means unfinished work or Order state remains, exit 2 means observation
   failed. Both block the switch. Inspect queued invocations and delayed sends too;
   the checker cannot establish that external requests cannot execute later. Exit 0
   is a clean observed inventory, not provider non-execution proof.
5. **Switch registration to the new immutable deployment.** Do not force old
   invocation replay onto it. Preserve namespace, external ids and scope mapping.
   Verify `check_account` per scope and the configured execution mode. Resume
   producers only once the supported operation matrix is understood by callers.
6. **Rollback uses the same discipline.** Quiesce and settle work from the newer
   mode before directing new invocations back. Never use rollback to rearm an old
   request or erase an unsupported marker.

## Marker compatibility is not exceptional-replay permission

Current code understands known legacy and replay-enabled markers for observation
and recovery regardless of configured mode. A marker never chooses a send path;
its presence blocks new admission. Unknown contracts remain inspectable where the
observation contract permits and cannot authorize recovery. Preserve the complete
marker JSON, including omitted/null members and integer precision.

The serialized prepare discriminator prevents a legacy prepare result from decoding
as replay-enabled intent, but this is not a general deployment-changing replay
guarantee. Before exceptional replay review the actual journal prefix, command
order/names, serialization, inputs and branch decisions against the candidate code
under ADR 0009. Even a same-mode code change requires that review. Keep completed
results/invocations on normal immutable routing for their retention period.

## Pre-release API migration

Replace `Order::experimental_request_response()` with
`config.order_execution = OrderExecution::ReplayEnabled` before `config.validate()`.
Remove the no-op Agent method. Remove the library `test-util` dependency from hosts;
test observers and unchecked config are not production facilities. Hand-written
`WorkerConfig` literals need the new field or `..WorkerConfig::new(namespace)`.
No existing marker tokens or durable step names are renamed by this API promotion.
