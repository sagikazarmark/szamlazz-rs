# Restate practices relevant to the `restate-szamlazz` audit

Date: 2026-09-10. Scope: primary-source grounding for Rust SDK **0.12.0**, shared core **7.0.3**, and
Restate server **v1.7.8**. This is a source note, not a code-audit verdict. No runtime probes were run.

## Reading rules and version baseline

- **Best practice** = an explicit official recommendation or correctness rule, with its source.
- **Capability** = documented behavior or an implementation fact in the specified release; availability does
  not mean Restate recommends choosing it for this application. **Source-verified** distinguishes implementation
  evidence from a public API promise.
- **Preference / inference** = a project policy or a conclusion drawn from the cited mechanisms, not a vendor
  prescription. An absence below means no recommendation was found in the consulted sources, not proof that none
  exists anywhere.

Repository questions came from [the design](../design/restate-szamlazz.md), the crate
[README](../../crates/restate-szamlazz/README.md), and ADRs
[0004](../adr/0004-kill-not-pause-on-exhausted-retries.md),
[0005](../adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md),
[0006](../adr/0006-account-selection-via-restate-scopes.md), and
[0009](../adr/0009-immutable-deployments-no-journal-compatibility-contract.md): query-first writes, run versus
invocation retries, unjournaled credential fetches, a state-free Order, shared `get`, immutable deployments,
execution logging, and structured faults. Those documents establish intent, not independent Restate evidence.

[Cargo.lock](../../Cargo.lock) resolves `restate-sdk` and `restate-sdk-macros` to 0.12.0 and shared core to 7.0.3.
[compose.yaml](../../compose.yaml) pins server 1.7.8; the
[Dagger CI module](../../.dagger/modules/ci/main.dang) defaults `restateVersion` to 1.7.8. The crate README's
tested combination includes protocol v7 and the experimental `vqueues` / `protocol_v7` /
`scoped_virtual_objects` flags for multi-account mode. A reused server or supplied server binary still needs its
actual version/features checked; the dependency declaration alone does not pin that process.

The official [Rust guide][rust] currently redirects to the SDK rustdoc. SDK citations below use explicit
0.12.0 URLs; the matching published source was read from the local Cargo registry. Server source links use
`v1.7.8`. `docs.restate.dev` pages are **current, unversioned guidance retrieved on this date**. They contain
features and examples for other SDKs, and some default descriptions disagree with the Rust rustdoc. Prefer the
versioned implementation for exact behavior, not a generic “Restate always…” claim.

## 1. `ctx.run`: a durable result, not an atomic external transaction

**Best practice.** Journal non-deterministic results that determine subsequent execution. Rust's
`ContextSideEffects` documents database/HTTP results and random values; after a result is recorded, replay uses
that result rather than executing the closure. It explicitly forbids context actions inside `ctx.run`, including
nested runs, state access, and durable calls. It also says to **immediately await a run before other context
calls**, because interleaving can produce a different journal on replay. This is an SDK 0.12.0 rule, not an
inference from another language's concurrency support. [R1]

**Capability, source-verified.** The Rust implementation executes the closure, serializes its result, and only
then proposes the run completion to Restate. Failure between the external effect and its durable completion can
therefore execute the closure again. The SDK explicitly warns that actual executions can exceed a configured
attempt count. A completed external HTTP request is not necessarily a completed journaled run. [R2, R3]

**Best practice / documented examples.** The official database guide supplies conditional updates and
transactionally recorded idempotency tokens to prevent duplicate external writes. Its examples explain the
failure window between a database update and Restate seeing completion. These mechanisms, not `run` alone,
establish external exactly-once effects. Durable calls *between Restate handlers* are a different mechanism:
Restate records the call and creates the target invocation itself. [D1, D2]

**Application inference.** For the Számla Agent integration, “at-least-once execution of an unfinished run,
with external reconciliation/deduplication” is the useful audit model. A query whose purpose is to detect what
the **previous execution of this write** may have issued must be refreshed when that write executes again.
Putting it in a preceding, already-completed run would replay the old answer. That supports the project's
query-inside-create-closure reasoning, but does not establish szamlazz.hu's read-after-write behavior,
uniqueness rules, or the sufficiency of its delay. Those are separate protocol evidence. [R1–R3]

`max_attempts(1)` suppresses policy-driven retries after a reported failure; it does **not** make an external
write at-most-once across crashes before its result is recorded. Likewise, a finite retry policy can terminate
without a successful external outcome: “at-least-once” describes the re-execution model, not an unconditional
promise that every accepted request eventually succeeds. [R2, R3, D3]

## 2. Retry policies, exhaustion, and timeouts

### Three materially different Rust defaults

| SDK 0.12.0 expression | Behavior |
|---|---|
| `ctx.run(f)` with no `.retry_policy(...)` | Shared-core `RetryPolicy::Infinite`: no explicit next retry delay; the server's invocation retry policy supplies the retry decision/delay. |
| `RunRetryPolicy::new()` | 100 ms initial delay, factor 1, no maximum delay, attempt cap, or duration cap. |
| `RunRetryPolicy::default()` | 100 ms initial delay, factor 2, maximum delay 2 s, no attempt cap, maximum duration 50 s. |

**Capability, source-verified:** these are different choices. Calling `default()` is not equivalent to leaving
the run policy unspecified. [R2, R3, R4]

**Best practice.** Official guidance distinguishes transient infrastructure failures (normally retry) from
permanent/application failures (terminal), and provides run-specific policies for external operations. Run
retries are coordinated by the server through re-invocation, not a long SDK-local sleep loop. [D3]

**Capability, source-verified.** In shared core 7.0.3, a retryable run failure is evaluated against its policy;
a retry sets `next_retry_delay`, while exhaustion proposes a terminal failure with the last error's code and
message. Rust creates code 500 for ordinary retryable closure errors and maps explicit run policies to
`FailAsTerminal`. Thus `ctx.run(...).await` can return a `TerminalError` to handler code for compensation or
application-specific fault mapping. The shared core has a pause-on-run-exhaustion option, but Rust 0.12.0's
public `RunRetryPolicy` does **not** expose it. Do not import another SDK's run options into this audit. [R2–R4]

**Capability, source-verified for server 1.7.8.** `handle_task_error` uses
`next_retry_interval_override.or_else(|| retry_iter.next())`. An SDK-requested run delay therefore does not
advance the invocation-policy iterator. Ordinary worker/transport failures without that override do. With
vqueues, a sufficiently long delay can yield to the scheduler and carry the counters forward. “Every handler
execution spends an invocation attempt” is not an accurate model for this version. [S1]

**Important limit.** Neither run limit is a hard external-send or wall-clock bound. Rust rustdoc explicitly
says both the actual retry count **and real loop duration** can exceed their configured values. The shared core
checks limits after the closure fails, adding its duration; it restores prior retry information only for the
first entry being processed after replay, otherwise beginning with default retry information. `max_duration`
does not interrupt a hung closure. A per-call deadline and the service's execution timeouts answer different
questions. A source note cannot justify calling `max_duration` an unconditional “hard bound.” [R2–R4]

**Capability / best practice.** Inactivity timeout is the wait for progress before asking the SDK to suspend;
abort timeout starts **after** that request and bounds the further wait before forcibly aborting the execution.
Official guidance recommends increasing both for long-running operations. They are not two competing timers
starting with the HTTP call, nor the same thing as a terminal invocation kill. [D3, R5, S2]

**Preference.** The project's issue/read/resolve policy split, attempt numbers, two-minute write intervals,
ten-second resolver/store deadlines, and terminal mapping after credential-fetch retries are application
choices. Restate supports configuring them; its docs do not prescribe these numbers. Current service docs show
50 ms/70 attempts/pause, while Rust rustdoc still describes infinite default retries. This disagreement is a
reason to inspect explicit/effective policies, not to report an unspecified policy as definitely infinite.
At 1.7.8, `ModifyServiceRequest` exposes visibility, retentions, inactivity and abort timeouts, **not invocation
retry-policy fields**; generic current UI/CLI override guidance must not be read as proof of live retry-policy
editing on that server. [D3, D4, R5, S2]

## 3. Cancellation, terminal errors, and kill are different outcomes

**Capability.** Cancellation cooperates with handler code. It is surfaced at an awaited **Restate context
action**, allowing compensation. The official guide says cancellation of an executing run is surfaced after
that run finishes, and cancellation requires a reachable deployment. It does not promise to interrupt arbitrary
network I/O or a credential fetch outside the context immediately. [D3, D5]

**Capability, source-verified for Rust 0.12.0.** Cancellation surfaces as `TerminalError` code **409**, message
`cancelled`, in the run/future implementation. The running closure is polled to completion before the SDK
returns to waiting on its result. A 409 alone is not a universal typed cancellation discriminator: applications
can construct a terminal 409 too, and server 1.7.8 uses 409 for manual kill and other conflicts. [R3, R6, S3]

**Best practice.** Official guidance recommends compensation when needed, and describes manual kill as a last
resort: it stops the invocation tree without running compensation. Already-performed external effects are not
automatically undone. One-way/delayed calls are detached and are not canceled or killed with their originator.
The configuration guide also explicitly allows **kill on invocation retry exhaustion**, with the same warning
about missing compensation. That is a supported policy, not the default recommendation for every service.
[D4, D5]

**Preference / inference.** Translating cancellation during a Számla Agent write into `outcome_unknown` is an
application fault policy, justified only by the possibility that the write landed. Treating an exhausted optional
read as “no extra information” is also an application choice; it does not make swallowing cancellation an official
best practice. Reviewers should distinguish run exhaustion (handler can catch it), cancellation (handler can
cooperate), and invocation-policy exhaustion/kill (handler compensation does not run). [D3–D5, R3]

The choice to kill rather than pause to free an Order for later reconciliation belongs to ADR 0004. Restate's
per-key exclusivity explains the head-of-line blocking while an exclusive invocation remains unfinished; a
shared handler is the documented concurrent access path. Restate does not certify the application's
reconciliation after releasing that key. [R7, D5]

## 4. Credentials and I/O outside the context

**Best practice.** Results that select later durable operations must be stable during replay. The versioning
guide names external operations and changed conditional logic among sources of journal mismatches. The
database guide contrasts a plain read with a durable read specifically because decisions based on the latter
repeat consistently. [D1, D6, R1]

**Capability.** Ordinary async code and externally owned clients/configuration are supported. The official
database guide even demonstrates direct I/O without `run`, explaining which guarantees those examples forgo.
Rust's service docs explicitly support dependencies as struct fields, shared behind an `Arc` across concurrent
invocations. Thus “any I/O outside `ctx.run` is forbidden” is too broad. Equally, none of this makes that I/O
journaled, replay-stable, or automatically governed by a run policy. [D1, R7]

**Preference / inference for this design.** A journaled Account/credential reference followed by a freshly
fetched secret can keep the logical destination stable while obtaining current authentication material.
Captured local values are not all serialized automatically: `run` serializes its **return value**, and failures
can also persist their messages. This explains why returning a secret from a run and merely using a secret
inside one have different persistence consequences. [R1, R3, R6]

No credential-specific official prescription to “fetch outside context on every execution with three local
retries” was found in the consulted sources. That is the project's privacy/rotation/availability trade-off,
recorded in design §4, not a Restate best practice to cite as such. Relevant review questions are:

- Does rotation retain the same logical Account, or can fresh material redirect the pending write?
- Can fetch failure during replay terminate an invocation whose external effect already landed?
- Are the resulting fault, log messages, and journaled values free of the secret?
- Are local waits bounded, given that Restate cancellation is delivered at its own await points?

These questions follow from the cited replay, serialization, and cancellation mechanics; this note does not
answer them by auditing the resolver/store implementation. [R1, R3, R6, D3, D6]

## 5. Virtual Objects can supply serialization without K/V state

**Capability, directly demonstrated.** The Rust guide's Virtual Object example uses `ctx.key()` and no state
operations. More decisively, the official database guide's `keyedDbAccess` object serializes external database
updates by key, with no Restate K/V state, and exposes a shared read that bypasses the exclusive queue. Using an
Order for its per-key serialization alone is therefore a documented use, not a misuse of a “stateful” service
type. [R7, D1]

**Capability.** Rust 0.12.0 infers the handler kind from its context: `ObjectContext` is exclusive and can write
K/V state; `SharedObjectContext` executes concurrently and cannot write that state. “Shared” does not prohibit
all external side effects through ordinary Rust clients. An ordinary Restate Service does not acquire an
Object's per-key serialization merely because a request includes an invoice number. [R7]

**Inference.** A shared `get` can read szamlazz.hu while an exclusive write is running or paused. That gives
availability, not an atomic snapshot of several external reads or a guarantee that it observes the write's final
result. Choosing live external state rather than Restate K/V state is a domain decision; Restate documents
advantages and trade-offs for both. [D1, R7]

## 6. Names and configuration: protocol identity versus style

**Capability.** Rust supports explicit service and handler names through `name = "..."`, and supports
service-wide configuration defaults with handler-level overrides. Repeating policy attributes on every handler
versus centralizing common defaults is a maintainability preference, provided the effective configuration is
the intended one. Neither the `Szamlazz.Order` dotted service name nor snake_case handler names conflict with an
official casing rule established by these sources. [R5, R7]

**Capability / best practice.** Service and handler names are addresses in the invocation contract. Current
versioning guidance says removing/renaming handlers is a breaking interface change and recommends updating
callers before registering with the breaking-change option. Immutable deployment routing does not eliminate
caller-facing compatibility. [D6, D7]

**Capability, source-verified.** `RunFuture::name` is optional and described as mainly for observability, but
shared core 7.0.3 compares `RunCommandMessage` by equality during replay and explicitly diagnoses a changed
`name`. Names are therefore not harmless labels to change while replaying the same journal. They are also not
global deduplication keys: run commands carry completion IDs and occupy the journal sequence. [R2, R8]

**Preference.** Kebab-case `{verb}-{object}-{parameter}`, one vocabulary for lookup steps, and the project's
step-name table are useful project conventions. Restate does not require that grammar or globally unique run
names. A name/sequence table alone cannot prove replay compatibility of serialized values, context-operation
inputs, or control flow. [R2, R8, D6]

## 7. Immutable deployments, retention, and exceptional replay

**Best practice.** Register each release at a unique immutable endpoint; route new invocations to the latest
deployment and keep old code available for invocations pinned to it, retries included. Official guidance says
this removes the need for mid-execution version-compatibility logic. It recommends checking active invocations
with `restate deployment describe <id> --extra`, removing old deployments after draining, and avoiding very
long-running handlers when possible. A unique URL must actually continue serving the old code. [D6]

**Capability / caveat.** The same guide recommends pause-and-resume on a new deployment containing the fix for repairing
in-flight work; the retained journal must replay compatibly. It also documents some safe in-place bug fixes,
while presenting `--force` re-registration as normal local-development convenience. Consequently, ADR 0009's
production prohibition of in-place replacement is a deliberately stricter project rule, not proof that Restate
has no supported repair path. Server 1.7.8's resume source keeps the pinned deployment unless explicitly
overridden and checks the replacement's protocol compatibility. That protocol check cannot prove application
replay compatibility. [D6, S4]

**Important qualification.** Normal immutable rollout, **cross-deployment resume**, and **restart from a
retained journal prefix** are three different cases. The latter is also documented and implemented in 1.7.8;
`restart_as_new` can copy a journal prefix and optionally replace the pinned deployment. It normally inherits the
old pin and clears the original idempotency key. Thus “pause/resume is the only way old journal bytes can be read
by new code” is too absolute unless the deployment runbook also excludes prefix restarts and endpoint reassignment.
These are operator-selected capabilities, not requirements to support arbitrary journal migrations. [D5, D6, S5]

**Capability.** Retaining an old deployment is different from retaining a completed invocation. Idempotency
retention preserves a completed result for deduplication; journal retention preserves execution history for
inspection/restart. For a keyed-idempotent invocation, server 1.7.8 caps journal retention at completion retention.
These completion timers do not supply a deadline by which an in-flight deployment will drain. K/V state, if used,
outlives individual invocations and persists across code versions; the versioning guide separately requires state
schema compatibility. [D4, D6, S6]

**Preference / inference.** Dropping a blanket cross-release journal compatibility contract is consistent with
normal immutable routing, conditional on retaining original deployments and controlling exceptional replay.
It does not remove compatibility questions for public request/response contracts, shared persistent state, or
explicit journal migration. Keeping only privacy scans and a sequence table is the project's assurance choice,
not a Restate requirement. [D6, S4–S6]

## 8. Logging and replay

**Capability / documented setup.** Rust SDK 0.12.0 uses `tracing`; a host must configure a subscriber to emit
logs. The optional `ReplayAwareFilter` suppresses events under the SDK endpoint span when
`restate.sdk.is_replaying` is true. Replay filtering is opt-in, not guaranteed by using `tracing::warn!`.
The SDK endpoint span contains `rpc.system`, `rpc.service`, `rpc.method`, and the replay flag. [R7, R9]

**Official convention.** The lifecycle guide uses `restate.invocation.id` as the cross-system correlation field.
Do not infer that every Rust application event automatically carries it: the 0.12.0 endpoint span just listed
does not declare it. Adding an application execution span with the invocation ID and relevant account/scope
context is compatible with the documented logging model; its exact fields and level policy are project choices.
[D5, R9]

**Inference.** A replay-aware filter reduces replay noise; it is not exactly-once logging, does not deduplicate
logs from re-executed unfinished closures, and is not a durable audit trail. Application-owned errors may also
be serialized into the journal/ingress response, so protecting secrets is not only a subscriber-filter concern.
No consulted source mandates the project's particular paging `warn` or forbids application logging outside a
run. [R3, R6, R9]

## 9. Fault bodies, serialization, and discovery schemas

**Capability, source-verified.** Rust 0.12.0's `TerminalError` has `code: u16` and `message: String`; its default
code is 500. Converting to shared-core `TerminalFailure` sends empty metadata, and converting back retains only
code/message. Current TypeScript/Python terminal-error metadata examples are **not** a Rust 0.12.0 API. [R6, D3]

Server 1.7.8 ingress wraps an invocation failure as JSON with its numeric `code`, string `message`, and
`source: "invocation"`, setting `content-type: application/json` and
`x-restate-error-source: invocation`. The general server error model can additionally carry stacktrace/metadata.
If the application places JSON in the SDK's message, it remains an escaped JSON **string** in this envelope:

```json
{"code":503,"message":"{\"code\":\"unavailable\",\"message\":\"...\"}","source":"invocation"}
```

**Preference.** The nested `Fault` shape is the project's encoding through the SDK's string channel, not
Restate's native typed error body. The outer code is not the project's fault token. Nor does
`source: invocation` prove the application wrote its normal structured fault: cancellation, manual kill, and
invocation retry exhaustion can produce terminal outcomes too. [R6, S1, S3, S7]

**Best practice.** Official HTTP guidance says invocation-origin errors are usually not retryable; ingress/proxy
errors are retried only for transient statuses, with an idempotency key on calls/sends. Reusing a key replays the
completed result during retention rather than executing again. The project's “keep the key after no answer;
reconcile and use a new key after a completed unknown-outcome fault” is an application recovery policy built on
that behavior, not a promise that changing keys alone makes the external write safe. [D7]

**Capability.** Rust's SDK serialization traits support custom byte encodings; `Json<T>` uses `serde_json`.
`PayloadMetadata` separately supplies content types and schemas. With `schemars`, `Json<T>` requires
`T: JsonSchema` and supplies a generated schema; without it, the complex-type schema is `{}`. The versioned SDK
documents this even though the current HTTP guide's short list of rich-schema SDKs omits Rust. [R10, D7]

**Capability, source-verified for 1.7.8.** Discovery schema is not ingress JSON-schema enforcement.
`InputValidationRule::JsonValue` checks the content type and nonempty body, leaving further JSON validation as a
TODO. Unknown-field refusal and semantic validation therefore need the SDK/application decoder; merely
advertising `additionalProperties: false` does not enforce it at this ingress. [S6]

**Capability / documented pattern.** Default Rust input decoding failure becomes terminal 400 with
`Cannot decode input payload: ...` before the handler runs. That is a plain-text **message inside the normal
error envelope**, not necessarily a `text/plain` HTTP response. The error-handling guide explicitly demonstrates
decoding raw input inside a handler to control error handling, including a Rust `Vec<u8>` example. A custom
`Body<T>` retaining the decode verdict while exposing `Json<T>` metadata is a project-specific implementation
of that supported boundary. [R3, D3, S7]

**Capability / inference.** The server's OpenAPI export describes the handler's success output and a generic
`RestateError` for failures. Deriving `JsonSchema` on the success type does not automatically document the
application JSON embedded in `TerminalError.message`. Closed request objects, open response token enums, and
the project's `Fault` conversion policy are contract design choices beyond Restate's serializer requirements.
Schema generation, runtime decoding, and journal replay decoding should be evaluated separately. [R10, S8]

## Audit takeaways

1. **Official correctness rules:** stable replay inputs/control flow, no context calls inside runs, immediate
   awaiting of Rust runs, and an external duplicate-prevention mechanism where repeated effects matter. [R1–R3, D1]
2. **Documented capabilities, not smells by themselves:** state-free Virtual Objects, shared status handlers,
   custom serializers/decoders, named runs, and service/handler retry overrides. [R2, R5, R7, R10]
3. **Application policies needing their own rationale:** kill instead of pause, secret fetch placement and local
   retry limits, domain outcomes as data, structured JSON in terminal messages, and exact naming/logging rules.
   [D3–D6, R5–R7]
4. **Version-sensitive wording to qualify:** “hard” run limits; automatic cancellation of ordinary I/O; unlimited
   default retries; live retry-policy editing; metadata support in Rust errors; ingress schema enforcement; and
   “no journal compatibility needed” outside normal immutable routing. [R2–R6, S2, S4–S6]

## Primary sources

All D-pages were retrieved on 2026-09-10 and are unversioned. R-links fix the published Rust SDK/shared-core
version. S-links fix the server tag. Source section/function names below are the relevant locators.

- **D1** [Databases and Restate](https://docs.restate.dev/guides/databases): direct/durable reads,
  `keyedDbAccess`, conditional updates, and transactional idempotency examples.
- **D2** [Request Lifecycle](https://docs.restate.dev/guides/request-lifecycle): replay and cross-service calls.
- **D3** [Error Handling](https://docs.restate.dev/guides/error-handling): transient/terminal errors,
  run/invocation retries, cancellation, timeouts, raw-input decoding.
- **D4** [Service Configuration](https://docs.restate.dev/services/configuration): retry exhaustion,
  retention, timeouts, and configuration overrides.
- **D5** [Managing Invocations](https://docs.restate.dev/services/invocation/managing-invocations):
  cancellation, kill, resume, restart from prefix, invocation-ID correlation.
- **D6** [Versioning](https://docs.restate.dev/services/versioning): immutable deployments, draining,
  state compatibility, journal mismatch, repair paths, interface changes.
- **D7** [HTTP invocation](https://docs.restate.dev/services/invocation/http): names/paths,
  idempotency retention, error-source-aware retries, OpenAPI.
- **R1** [SDK 0.12.0 `ContextSideEffects`](https://docs.rs/restate-sdk/0.12.0/restate_sdk/context/trait.ContextSideEffects.html):
  `run`, replay, immediate-await warning.
- **R2** [SDK 0.12.0 `context/run.rs`](https://docs.rs/restate-sdk/0.12.0/src/restate_sdk/context/run.rs.html):
  `RunFuture::name`, policy constructors and both limit caveats;
  [rendered `RunRetryPolicy`](https://docs.rs/restate-sdk/0.12.0/restate_sdk/context/struct.RunRetryPolicy.html).
- **R3** [SDK 0.12.0 `endpoint/context.rs`](https://docs.rs/restate-sdk/0.12.0/src/restate_sdk/endpoint/context.rs.html):
  `input` (202–265), `RunFutureImpl` (939–1121), closure execution before completion proposal.
- **R4** Shared core 7.0.3:
  [`retries.rs`](https://docs.rs/crate/restate-sdk-shared-core/7.0.3/source/src/retries.rs), `next_retry`;
  [`vm/context.rs`](https://docs.rs/crate/restate-sdk-shared-core/7.0.3/source/src/vm/context.rs), `infer_entry_retry_info`;
  [`vm/transitions/journal.rs`](https://docs.rs/crate/restate-sdk-shared-core/7.0.3/source/src/vm/transitions/journal.rs), `ProposeRunCompletion`.
- **R5** [SDK 0.12.0 configuration](https://docs.rs/restate-sdk/0.12.0/restate_sdk/configuration/index.html):
  names, defaults/overrides, invocation retry policy, timeouts, programmatic binding options.
- **R6** [SDK 0.12.0 `errors.rs`](https://docs.rs/restate-sdk/0.12.0/src/restate_sdk/errors.rs.html):
  `HandlerError`, `TerminalError`, conversions to/from `TerminalFailure`.
- **R7** [SDK 0.12.0 overview](https://docs.rs/restate-sdk/0.12.0/restate_sdk/): services,
  dependency fields, Virtual Objects, shared handlers, logging.
- **R8** [Shared core 7.0.3 `service_protocol/messages.rs`](https://docs.rs/crate/restate-sdk-shared-core/7.0.3/source/src/service_protocol/messages.rs):
  `command_header_eq` (76–82), `RunCommand: command eq` (308), `RunCommandMessage::write_diff` (821–836).
- **R9** SDK 0.12.0
  [`filter.rs`](https://docs.rs/restate-sdk/0.12.0/src/restate_sdk/filter.rs.html), `ReplayAwareFilter`, and
  [`endpoint/mod.rs`](https://docs.rs/restate-sdk/0.12.0/src/restate_sdk/endpoint/mod.rs.html), `handle_invocation` span (595–601).
- **R10** [SDK 0.12.0 `serde.rs`](https://docs.rs/restate-sdk/0.12.0/src/restate_sdk/serde.rs.html):
  serialization traits, `PayloadMetadata`, `Json<T>` with/without `schemars`.
- **S1** [Server v1.7.8 invocation state machine](https://github.com/restatedev/restate/blob/v1.7.8/crates/invoker-impl/src/invocation_state_machine.rs):
  `handle_task_error`, retry iterator, scheduler retry, `OnMaxAttempts`.
- **S2** [Server v1.7.8 service modification model](https://github.com/restatedev/restate/blob/v1.7.8/crates/admin-rest-model/src/services.rs):
  `ModifyServiceRequest` fields and timeout semantics.
- **S3** [Server v1.7.8 invocation errors](https://github.com/restatedev/restate/blob/v1.7.8/crates/types/src/errors.rs):
  `InvocationError`, cancellation/kill constants, 409 uses.
- **S4** [Server v1.7.8 manual resume](https://github.com/restatedev/restate/blob/v1.7.8/crates/worker/src/partition/state_machine/lifecycle/manual_resume.rs):
  `resolve_pinned_deployment` and `OnManualResumeCommand::apply`.
- **S5** [Server v1.7.8 restart as new](https://github.com/restatedev/restate/blob/v1.7.8/crates/worker/src/partition/state_machine/lifecycle/restart_as_new.rs):
  copied prefix, inherited/replaced deployment pin, cleared idempotency key.
- **S6** [Server v1.7.8 invocation target](https://github.com/restatedev/restate/blob/v1.7.8/crates/types/src/schema/invocation_target.rs):
  `compute_retention`, `InputValidationRule::validate`, schema-validation TODO.
- **S7** [Server v1.7.8 ingress errors](https://github.com/restatedev/restate/blob/v1.7.8/crates/ingress-http/src/handler/error.rs):
  `ErrorResponse`, `ErrorSource`, `fill_builder`, HTTP status and content type.
- **S8** [Server v1.7.8 OpenAPI generation](https://github.com/restatedev/restate/blob/v1.7.8/crates/types/src/schema/metadata/openapi.rs):
  success response versus generic error, `restate_error_json_schema`.

[rust]: https://docs.restate.dev/develop/rust
