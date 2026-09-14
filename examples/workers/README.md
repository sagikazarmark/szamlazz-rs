# Actual Order/Agent on Cloudflare Workers (#247)

An **experimental**, explicitly selected endpoint for ordinary invoices (including
exact-target reissue and pinned proforma conversion), reads and operator recovery.
It embeds the real services, including input/money validation, account
resolution, fresh guards, unresolved markers and read-only reconciliation.
It does not approve additional mutation types or production migration. The ingress
gateway must authorize operator observation/recovery separately; request signatures
authenticate Restate, not the recovery operator.

## Build and run

Requirements: Rust with `wasm32-unknown-unknown`, Node 22+, npm, `worker-build 0.8.5`.
From this directory:

```sh
cargo install worker-build --version 0.8.5 --locked
npm ci
worker-build --release --no-panic-recovery --locked
```

The toolchain used here needed `--no-panic-recovery`: worker-build's default abort
recovery generation failed with `externref table required for catch wrappers`.
The pinned wasm-bindgen version is recorded in Cargo.toml/lock. This build uses
ordinary panic-abort behavior; a panic does not settle external effects.

`wrangler.toml` points to the generated `build/worker/shim.mjs`. Configure:

- **Secret `SZAMLAZZ_ACCOUNTS`**: JSON for the existing `StaticConfig` contract.
  For example `{"accounts":{"alpha":{"id":"alpha","agent_key":"…"}}}`.
- **Variable `SZAMLAZZ_NAMESPACE`**: permanent deployment namespace, e.g. `billing`.
- **Variable `RESTATE_IDENTITY_KEY`**: the Restate environment's `publickeyv1_…` key.
  Required; the example never silently accepts unsigned SDK calls or discovery.

Use `wrangler dev` / `wrangler deploy` with your chosen Wrangler version. Register
the resulting endpoint with Restate. The local acceptance proxy is HTTP/1.1 and
registers with `use_http_11: true`; the deployed HTTPS route negotiates normally.
The host consumes the entire SDK output during the fetch event; no detached SDK
task or `waitUntil` continuation owns document work.

The caller's trusted ingress gateway authenticates callers, selects the Restate
scope and strips caller-supplied `x-restate-*` headers. Endpoint signatures identify
Restate, not business callers. Enable protocol v7, vqueues and scoped Virtual Objects
on the tested Restate 1.7.8 server. Run `check_account` under every configured scope.

**Provider prerequisite:** enable duplicate-order checking on the intended account.
The worker cannot verify the toggle. Independently verify the deployed account/key
mapping and seller as described in the library's Go-live instructions. The fake
provider tests below do not establish vendor reauthentication or duplicate atomicity.

## Exact interim SDK override

This example is a separate Cargo workspace so its override does not change native
consumers or the workspace's crates.io SDK tests:

```toml
[patch.crates-io]
restate-sdk = { git = "https://github.com/sagikazarmark/restate-sdk-rust", rev = "e882e9e04f9ad9942b4fe7f3788999a521dfa336" }
```

An embedder must put this patch at **its workspace root**, until a suitable SDK
release incorporates [upstream PR #128](https://github.com/restatedev/sdk-rust/pull/128).
It replaces SDK OS clocks with web-time, selects JS entropy backends, and removes
the Tokio-driven WASM input-drain timer. The host must bound/consume the request;
the example enforces a ten-minute JS deadline over the complete input/output
exchange. Expiry drops SDK work and returns 504; it never clears unresolved state
or proves provider rollback. The event lifetime alone is not a wall-clock bound.

Feature graph: restate-sdk defaults are disabled at this repository's workspace
dependency; `rand`, `uuid`, `tracing-span-filter`, `rust_crypto` remain enabled.
`http_server` is added only on native targets. Worker request-identity verification
uses the Rust crypto backend. Runtime-independent Tokio utilities and task-local
spans remain; Tokio socket/signal features are absent from this WASM build.

## One Számla Agent client, reqwest on both hosts

Order/Agent use `szamlazz_agent::Client` on native and Workers. Reqwest selects its
native or WASM/Fetch backend. There is no worker-owned HTTP implementation or error
model. `Gateway::send` only wraps the client future with thread-checking
`SendWrapper` on WASM to satisfy Restate's Send bound; native bounds remain intact.

`szamlazz-agent` applies the shared `REQUEST_TIMEOUT` (60 seconds) to each WASM
request. Reqwest retains its abort guard through **full body** consumption; runtime
tests cover both deadline expiry and dropping a pending exchange. Incomplete bodies
remain client errors, never fabricated complete responses.

Reqwest's default WASM redirect/header behavior is accepted for szamlazz.hu's
non-redirecting endpoint. The prior injected-307 experiment demonstrated behavior
on a fake redirect, not a vendor problem; it does not justify a second transport.
The separate Fetch exchange and its blanket comma-header refusal have been removed.
Header interpretation belongs solely to the shared client/parser, using the values
reqwest supplies (Fetch may combine repeated non-cookie headers). Native redirect
configuration remains unchanged.

Workers persists no session cookies through reqwest's WASM backend. Every XML
request includes its resolved credentials. The fake-provider suite checks two
exchanges per execution and separate account credentials without Cookie headers;
actual vendor behavior remains the documented reauthentication premise.

Execution timers: resolver/store calls keep their ten-second deadline; credential
fetch retries keep their 200 ms sleep. WASM uses JS timers, native uses Tokio.
Marker timestamps use Jiff's `js` feature. Restate durable sleeps remain SDK calls.

## Acceptance checks (fake provider only)

```sh
worker-build --release --no-panic-recovery --locked --features acceptance-tests
npm test
# From the repository root, with node on PATH:
RESTATE_SERVER_BIN=/absolute/path/to/restate-server \
  cargo test --locked -p restate-szamlazz --test workers \
  e2e_workers_signed_scoped_ordinary -- --exact --ignored --nocapture
```

Never deploy the `acceptance-tests` build: it adds an unsigned transport probe and
mock-only policies/control sources. The ordinary example build has none of these.
All test servers and control endpoints bind to loopback. Test identity keys are
generated per run and removed on normal shutdown.

`npm test` runs workerd checks for shared comma-metadata interpretation, stalled complete-body deadline,
transfer abortion, explicit credential resubmission/no cookie persistence, and
unsigned discovery refusal. The Rust test uses real Restate ingress to exercise:

- Signed discovery/invocation; unsigned direct calls refused.
- Scope/account isolation with the same Order and ingress key on two scopes.
- Issuance and completed replay with credential acquisition unavailable.
- JS-backed resolver/store deadlines, retry sleep and marker timestamps.
- Runtime replacement while a provider answer is in flight: a visible holder
  settles without a second send, including reissue and proforma conversion.
- Recorded uncertainty, replacement, read-only pause/resume, later positive
  evidence, active cancellation, and no-effect kill followed by a blocked successor.
- Unsupported mutations refused before provider I/O and journal privacy.
- Explicit document queries preserve buyer/VAT facts and decimals beyond JS integer precision.

The native buffered suite additionally covers invisible first effects with/without
fake deduplication and deliberately surviving old execution. Those remain accepted
risks, not guarantees supplied by this hosting layer. Dagger `ci workers` builds
both release variants and runs the workerd transport plus real-Restate checks.

Remaining #247 gates: released operation capability contract, production settlement
approval, old marker/operator recovery and producer/deployment migration. See
[ADR 0019](../../docs/adr/0019-request-response-ordinary-invoice-replay.md) and the
[outcome table](../../docs/design/request-response-outcomes.md).
