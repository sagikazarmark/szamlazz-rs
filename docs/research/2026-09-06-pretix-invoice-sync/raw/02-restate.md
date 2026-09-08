# Restate invocation lifecycle as it bears on failure handling (server v1.7.8)

Sources: docs.restate.dev (current) and `restatedev/restate` at `v1.7.8` (paths relative to the repo; source wins over
docs). ADR 0004 / #87 facts are not re-verified. **V** = verified (citation); **U** = unverified.

## 1. Kill vs pause, precisely

- **Kill on exhausted attempts**: `InvokerEffectKind::Failed(last_error)` → `end_invocation(..., Some(Failure))`.
  Response sinks get the last retryable error; an `Idempotency-Key` makes `completion_retention`
  non-zero, so a `CompletedInvocation` is stored for `idempotency_retention`, the journal for
  `min(journal_retention, idempotency_retention)` (**V**: `crates/invoker-impl/src/invocation_state_machine.rs`
  `handle_task_error`; `crates/invoker-impl/src/lib.rs` `handle_error_event`;
  `crates/worker/src/partition/state_machine/mod.rs` `end_invocation`; `crates/types/src/schema/invocation_target.rs`
  `compute_retention`). `sys_invocation`: `status = completed`, `completion_result = failure`, `completion_failure =
  <text>`; the vqueue entry ends `failed`, not `killed`; `killed` is a manual kill only (**V**: `end_invocation`;
  `crates/storage-query-datafusion/src/context.rs` `SYS_INVOCATION_VIEW`).
- **Pause on exhausted attempts** stores `InvocationStatus::Paused(metadata)` with the response sinks *untouched*:
  nothing is sent (**V**: `lifecycle/paused.rs` `pause_invocation`). `sys_invocation.status = 'paused'`. A waiting
  `/call` **keeps waiting**: no timeout exists in `crates/ingress-http` or `crates/ingestion-client` (**V**:
  `handler/service_handler.rs` `handle_service_call`; only a 5 s shutdown drain in `server.rs`). The bound is the
  client's or a proxy's.
- A **new `/call` with the same key** against `Paused` appends a sink (`handle_duplicated_requests`, ADR #87) and the
  client **blocks until resume completes the invocation** or its own timeout (**V** by source; **U** end to end).
- `/output` on any in-flight invocation, paused included: **HTTP 470** `{"message":"the response is not ready yet"}`
  (**V**: `state_machine/mod.rs` `handle_attach_invocation_request`; `crates/types/src/errors.rs` `NOT_READY 470`).

## 2. `/send` semantics

- `POST /restate/send/{svc}/{key}/{handler}` (scoped: `/restate/scope/{scope}/send/…`) → **202**
  `{"invocationId","status":"Accepted"}` + `x-restate-id`, once the partition acknowledged the append; `?delay=` adds
  `executionTime` (**V**: `handle_service_send`; docs http).
- Duplicate `/send`, same key: the id is SHA-256 of (scope, service, key, handler, idempotency key), so **the same id**,
  `status: "PreviouslyAccepted"` (`is_new_invocation` compares `Source::Ingress(request_id)`, fresh per request)
  (**V**: `crates/types/src/identifiers.rs` `InvocationId::generate`; `handle_duplicated_requests`;
  `types/src/invocation/mod.rs` `Source`).
- Outcome: `GET /restate/attach/{id}` (blocks), `GET /restate/output/{id}` (peeks); by key
  `GET /restate/invocation/{svc}/{key}/{handler}/{idempotency-key}/attach|output`, **but this path form passes
  `scope = None`**; scoped invocations need `POST /restate/attach|output` with
  `{"target":"idempotentInvocation","service","key","handler","idempotencyKey","scope"}` (`POST /restate/lookup`
  resolves the id) (**V**: `handler/path_parsing.rs` `parse_restate_api_verb`; `handler/invocation.rs`
  `convert_to_invocation_query`; `handler/mod.rs` `InvocationTargetRequest`).
- `/output`: 470 in flight; success body with `x-restate-id`; a failure with its own status and
  `x-restate-error-source: invocation`; replays add `idempotency-expires`; expired or unknown → **404**
  `{"message":"not found"}` (**V**: `handle_attach_invocation_request`; `handler/responses.rs`; `handler/error.rs`).
  `/attach` blocks without ingress timeout.

## 3. `restart-as-new`

- `PATCH /invocations/{id}/restart-as-new?from=<index>&deployment=latest|<id>|keep` (`deployment` only with
  `from > 0`); **Completed only**: in flight → `StillRunning`, inboxed/scheduled → `NotStarted`, freed → `NotFound`;
  `journal_retention = 0` → `MissingInput` (**V**: `crates/admin/src/rest_api/invocations.rs`; `lifecycle/restart_as_new.rs` `apply`).
- Copies the journal prefix (input and headers are entry 0), target, scope, retention, and **the original's pinned
  deployment by default, even for `from = 0`** unless `deployment` overrides (**V**: `restart_as_new.rs` 225–230,
  285–311). Sets **`idempotency_key: None`** and empty `response_sinks`: the new invocation is **not reachable under
  the original key**; the original stays `Completed` and keeps replaying the *old* result (**V**: line 308;
  `handle_duplicated_requests` `Completed` arm). Settles the brief's UNVERIFIED.
- Bulk: `POST /internal/invocations_batch_operations/restart-as-new` (not in OpenAPI); CLI
  `restate invocations restart-as-new <service>` (**V**: `rest_api/mod.rs`).

## 4. `resume`

- `PATCH /invocations/{id}/resume?deployment=latest|<id>|keep`; bulk `…/invocations_batch_operations/resume`, CLI
  `restate invocations resume <service>` (**V**: `rest_api/invocations.rs`, `mod.rs`).
- `Paused`/`Suspended` → resumed; `Invoked` with a vqueue id → **a backing-off entry is rescheduled to run now**
  (`vqueue_reschedule_invocation`), a running attempt is a no-op; `Scheduled`/`Inboxed` → `NotStarted`; `Completed`
  → error (**V**: `lifecycle/manual_resume.rs` `OnManualResumeCommand::apply` 123–240, confirms #87). Repinning a
  running attempt is refused; `latest` on an unpinned invocation is a no-op (`resolve_pinned_deployment`). **U**:
  whether resume under vqueues restarts the attempt budget (ADR 0004 saw so pre-vqueues).

## 5. Cancel vs kill (manual)

- **Cancel** on `Invoked`/`Suspended`/`Paused` appends the cancel signal; on `Paused` it **resumes the invocation** so
  the handler sees it (**V**: `lifecycle/cancel.rs`; `entries/notification.rs` test
  `cancel_signal_while_paused_resumes_invocation`). The SDK raises `TerminalError` at the next await; handler code
  replays and may compensate; the deployment must be reachable (docs managing-invocations). Inboxed/scheduled end
  without running. Callers get the handler's terminal result or, if the cancel propagates, **409** `canceled` with
  `x-restate-error-source: invocation` (**V**: `errors.rs` `CANCELED_INVOCATION_ERROR`, `ABORTED = 409`).
- **Kill** on any in-flight state ends it at once without handler code; callers get 409 `killed`; vqueue status
  `killed` (**V**: `state_machine/mod.rs` `on_kill_invocation`).

## 6. Runtime policy override

- `PATCH /services/{name}` (`restate services config edit`) takes **only** `public`, `idempotency_retention`,
  `workflow_completion_retention`, `journal_retention`, `inactivity_timeout`, `abort_timeout`. **The retry policy is
  not runtime-modifiable in 1.7.8**: the brief is wrong here (**V**: `crates/admin-rest-model/src/services.rs`
  `ModifyServiceRequest`; `crates/types/src/schema/metadata/updater/mod.rs` `modify_service`; OpenAPI 1.7.8).
  Changing it means a new deployment revision; the server-wide `[invocation.default-retry-policy]` fills only fields
  the code leaves unset, and the worker sets all (**V**: `schema/metadata/mod.rs` `resolve_invocation_retry_policy`).
- Propagation of what *is* resolvable: re-resolved on every dispatch, active revision at task start, then the
  **pinned deployment's** revision once the attempt reports it, `fast_forward`ed by attempts so far; `modify_service`
  mutates the active revision inside its deployment, so a change reaches invocations pinned to the active deployment
  on their next dispatch, not those on an older one (**V**: `invoker-impl/src/lib.rs` `handle_vqueue_invoke`,
  `handle_pinned_deployment`; `invocation_state_machine.rs` `update_retry_policy_if_needed`; `updater/mod.rs`
  `apply_change_to_active_service_revision`). Overrides last until the next registration (docs).

## 7. Observability for a UI / reconciler

- `sys_invocation` = `sys_invocation_status` (storage) RIGHT JOIN `sys_invocation_state` (invoker memory). Columns:
  `id, target, target_service_name, target_service_key, target_handler_name, scope, idempotency_key, invoked_by,
  restarted_from, pinned_deployment_id, created_at, modified_at, inboxed_at, scheduled_at, running_at, completed_at,
  completion_retention, journal_retention, retry_count, last_start_at, next_retry_at, last_failure,
  last_failure_error_code, last_failure_related_command_{index,name,type}, status, completion_result,
  completion_failure`. `status` ∈ `pending` (inboxed), `scheduled`, `completed`, `suspended`, `paused`, `running`
  (`in_flight`), `backing-off` (`invoked AND retry_count > 0`), `ready`; **no `killed`** (**V**: `SYS_INVOCATION_VIEW`).
- Caveat: under vqueues a retry delay ≥ 2 s (`invocation_yield_threshold`) is `RetryViaScheduler`, the invoker drops
  its status row (`status_store.on_end`) and the entry waits in the vqueue inbox, so `sys_invocation` shows **`ready`
  with NULL `retry_count`/`last_failure`/`next_retry_at`** for what the docs call backing-off. Truthful tables:
  `sys_vqueues` (`stage='inbox'`, `status='backing-off'`, `run_at`), `sys_vqueue_entry_status` (`retry_attempts`,
  `num_errors`, `next_at`), `sys_journal_events` (**V** by source: `handle_task_error`; `lib.rs` `RetryViaScheduler`
  arm; `types/src/config/invocation.rs`; **U** end to end, #87 used 1 s delays).
- `POST /query` on admin port 9070, DataFusion SQL, e.g. `… where target_service_name='Szamlazz.Order' and status in
  ('paused','backing-off')` (**V**: docs introspection). The admin port has **no authentication by design**;
  network-restrict it (**V**: docs server/security). Documented for operators; nothing forbids a reconciler.
- **No webhooks/events** on completion or pause: none in the llms.txt index, `server/monitoring/*` or the admin
  OpenAPI; only OTEL traces and Prometheus metrics push (**U**, a negative).

## 8. Kafka ingress

Per record the subscription appends one `ServiceInvocation` (`Source::Subscription`, no response sink, **no
idempotency key**: dedup is producer id + offset, retention `compute_retention(false)`), then stores the Kafka offset
once the append commits (**V**: `crates/ingress-kafka/src/builder.rs` `InvocationBuilder::create`; `consumer_task.rs`
`run_inner`). From there the invocation is ordinary: the handler's retry policy applies; a kill completes it as a
failure nobody reads; a pause holds the VO key (same-key records inbox behind it, other keys proceed). Kafka never
redelivers, no "retry forever" there; a killed invocation drops the event silently (**V** by source; **U** end to
end). Scope comes from an `x-restate-scope` header behind the experimental `kafka_scope` flag (**V**: `builder.rs`
`extract_scope_limit_key`). The docs say nothing about failure semantics (**U**).

## 9. Retention interplay

Completion sets a `CleanInvocationStatus` timer at `completion_retention`; it runs `OnPurgeCommand`, freeing the status
(**V**: `state_machine/mod.rs` `on_timer`; `lifecycle/purge.rs`). The same key afterwards finds `Free`, **a fresh
execution under the same invocation id** (**V**: `handle_duplicated_requests`; `InvocationId::generate`). Manual
`purge` does the same at once. A reconciler reusing keys re-executes after 30 d (or a purge); `idempotency-expires`
on a replay says when.

## Consequences for the design

- Pause never answers a waiting caller: a forwarder that `/call`s and hits pause times out, and its fixed-key retries
  attach and block again. Pause needs `/send` plus a poller or a human, not a synchronous dumb caller.
- `/send` + `/output` by key is the asynchronous shape: 470 in flight, result or fault after, 404 after retention;
  scoped keys must use the POST-body `/output`.
- `restart-as-new` is not a retry under the caller's key (key dropped, pinned deployment inherited). A "retry"
  button re-calls the handler with a new key.
- The invocation retry policy is a deployment decision in 1.7.8; do not plan to tune kill/pause or attempts live.
- A reconciler on `sys_invocation` must also read `sys_vqueues`/`sys_vqueue_entry_status`, or backing-off writes look
  `ready`; it needs private access to the admin port.
- Kill's stored 500 under a key the caller repeats for 30 d is kill's real cost; the fix is caller-side (rotate, or
  `/output` + `get`) or a reconciler issuing with fresh keys.
- Cancel on a paused invocation resumes it for compensation: the operator's clean exit; kill is not.
- No push from Restate: state reaches the sync app only by polling SQL or `/output`.
