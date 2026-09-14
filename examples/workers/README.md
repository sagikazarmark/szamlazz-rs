# Actual Order/Agent on Cloudflare Workers (#247)

An **experimental**, explicitly selected endpoint for initial ordinary invoices and
reads. It embeds the real services, including input/money validation, account
resolution, fresh guards, unresolved markers and read-only reconciliation.
It does not approve additional mutations, marker recovery or production migration.

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

## Why Fetch instead of reqwest for the default WASM Gateway?

We first built actual Order/Agent with reqwest's WASM backend. A workerd regression
test observed its `307` forwarding the credential-bearing POST to the redirect
destination. Reqwest 0.13.5 does not expose a WASM redirect-policy setter. Therefore
only the default WASM exchange uses a small Fetch adapter at the existing
`AgentRequest::to_wire` / `parse(RawResponse)` seam. Native Gateway remains reqwest.
Direct `Gateway::open_with_http` still accepts a supplied reqwest client; that
expert hook retains caller-owned transport policy and is not used by this endpoint.

The default WASM exchange:

- Makes one POST with `redirect: manual`; never follows a redirect or retries.
- Uses an AbortController and the shared `REQUEST_TIMEOUT` (60 seconds) covering
  headers **and full body**. Dropping the exchange aborts it; provider effects may
  already exist. Incomplete bodies never become successful parsed responses.
- Passes Fetch's header entries to the shared parser. **Approved experimental
  restriction:** Fetch combines repeated non-cookie headers. Any `szlahu_*` value
  containing a literal comma is therefore refused as an inconclusive exchange,
  before it could manufacture a combined document number or verdict. This also
  refuses legitimate literal-comma headers, including comma-decimal fallback
  metadata. URL-encoded `%2C` is decoded only later by the shared parser. Native
  first-header behavior cannot be reproduced losslessly through Fetch.
- Deliberately persists **no session cookie**. Every XML exchange includes its
  resolved credentials. No ambient browser jar, cross-account cookie reuse, or
  reconstruction of cookies from comma-joined headers is assumed. The fake provider
  checks two exchanges per execution and separate credentials without Cookie headers;
  actual vendor session behavior remains the documented reauthentication premise.
- Adapts JS futures with thread-checking `SendWrapper` locally. No JS future or
  response escapes the host exchange; native Send bounds remain unchanged.

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

`npm test` runs workerd checks for redirects, stalled complete-body deadline,
transfer abortion, explicit credential resubmission/no cookie persistence, and
unsigned discovery refusal. The Rust test uses real Restate ingress to exercise:

- Signed discovery/invocation; unsigned direct calls refused.
- Scope/account isolation with the same Order and ingress key on two scopes.
- Issuance and completed replay with credential acquisition unavailable.
- JS-backed resolver/store deadlines, retry sleep and marker timestamps.
- Runtime replacement while a provider answer is in flight: a visible holder
  settles without a second send.
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
