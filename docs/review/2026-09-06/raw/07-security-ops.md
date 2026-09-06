# Review 07 — Security and operational readiness

Repository: `/home/laborant/szamlazz-rs2` at `0e4238c` (workspace v0.3.0).
Scope: credential handling, multi-account isolation, inbound receivers, the Restate endpoint binary, container, config/secrets, supply chain, error/observability hygiene, data handling. Read-only review; no cargo commands were run.

## Summary

The credential story is well engineered for a library of this size: the agent key lives in a newtype with redacting `Debug`, no `Serialize`/`Deserialize` anywhere on the `Credentials`/`AgentKey`/`Secret` path, a fresh `reqwest::Client` (own cookie store) per handler execution, a fetch-outside-the-journal prologue, redacted duplicate-key config errors, and a byte-level e2e scan of every Restate journal row. The multi-account resolution fails closed when the scope is missing on a multi-account deployment. The Adatkapcsolat receiver compares its key without early exit and never echoes handler errors. Supply chain basics (lockfile, `--locked`, digest-pinned base images, dependabot, Scorecard, `unsafe_code = forbid`, `cargo-audit` with justified ignores) are in place.

The gaps are mostly operational rather than cryptographic: **the Restate endpoint accepts unsigned requests by default and never warns about it**, which means anyone who can reach port 9080 (the default bind is `0.0.0.0`) bypasses the "gateway sets the scope" boundary that ADR 0006 leans on; the strongest secrecy guarantee (the e2e journal scan) is `#[ignore]`d and not demonstrably run in CI; two of the four GitHub Actions workflows are not SHA-pinned; the Adatkapcsolat router has **no request-body limit by default and tokenizes the whole XML twice before authenticating**; and upstream szamlazz.hu response bodies can flow verbatim into caller-facing 503 fault messages and journaled outcomes. No finding is *critical*; the two *high* items are deployment-posture issues that a cautious operator would already mitigate, but the defaults and docs should not rely on that.

**Overall risk: medium** for a deployment that follows the README to the letter; **high** for one that does not set `identity_keys` and exposes 9080 beyond the Restate server's network.

---

## Findings

### 1. Endpoint accepts unsigned Restate requests by default, with no warning

- **Severity:** high
- **Confidence:** high — read the code path and the SDK verifier.
- **Location:** `crates/restate-szamlazz-endpoint/src/main.rs:184-195`; `~/.cargo/registry/.../restate-sdk-shared-core-7.0.3/src/request_identity.rs:132-134`; README `crates/restate-szamlazz-endpoint/README.md:331`.
- **Evidence:** `if !identity_keys.is_empty() { tracing::info!(... "request identity verification enabled") }` — there is no `warn!` for the empty case. SDK: `if self.keys.is_empty() { return Ok(()); }`. README: "Without `identity_keys` the endpoint accepts unsigned requests."
- **Impact:** ADR 0006 rule 6 ("the scope is routing, not authorization") assumes the *only* way in is the Restate ingress behind an authenticating gateway. Without identity keys, anything that can open a TCP connection to `{bind}:{port}` (default `0.0.0.0:9080`, `EXPOSE 9080` in the Dockerfile) can speak the Restate service protocol directly to `/invoke/Szamlazz.Order/create_invoice` with **any scope it chooses**, since the scope arrives as a protocol header from whoever plays the server. That is a full bypass of the multi-account boundary and of Restate's idempotency/locking, and it would let an attacker issue legal documents on every configured account. The single-account shape is exposed the same way for its one account.
- **Recommendation:** (a) Log at `warn!` on start-up (and in `--check-config`) when `identity_keys` is empty, stating that any client reaching the port can invoke handlers. (b) Consider an explicit opt-out (`allow_unsigned_requests = true`) and otherwise refuse to start without keys, at least in the multi-account shape. (c) Move the Request Identity section of the README into the multi-account/deploy checklist as a hard requirement and mention that the bind address should be the Restate server's network only.

### 2. The end-to-end journal leak scan is `#[ignore]`d and not evidently run in CI

- **Severity:** high
- **Confidence:** medium — the dagger `check` module is remote (`sagikazarmark/daggerverse-beta/rust`); I could not read what it runs, but nothing in the repo passes `--ignored` or provisions Docker for it.
- **Location:** `crates/restate-szamlazz/tests/service.rs:1458` (`#[ignore = "needs docker"]`), `:4148-4201` (the scan); `.github/workflows/dagger.yaml`; `dagger.toml`; `crates/restate-szamlazz/README.md:403`.
- **Evidence:** CONTEXT.md and ADR 0006 both say "`AgentKey::expose()` is one line from journalable; the e2e journal scan is the real guarantee". The scan itself is sound (hex-decodes `raw` of every `sys_journal` row, checks `completion_failure`, has a positive control). But the only invocation path documented is a manual `cargo test -p restate-szamlazz -- --ignored e2e`, and the compile-time guard `assert_not_impl_any!` lives inside `#[cfg(test)] mod tests` (`crates/restate-szamlazz/src/account.rs:424-436`), so it too only fires under `cargo test`, not `cargo build`.
- **Impact:** The guarantee that no agent key is ever journaled is verified only when a developer remembers to run the ignored suite locally with Docker. A regression (e.g. a future `String` field populated from `expose()` in a journaled outcome) would ship undetected.
- **Recommendation:** Run the e2e suite in CI (GitHub Actions has Docker; a dedicated workflow with `cargo test -p restate-szamlazz --test service -- --ignored`), or at minimum a nightly/scheduled job that fails loudly. Move the `assert_not_impl_any!` out of `mod tests` into the module body so it is enforced on every build. Add a unit test that serializes every `Journaled` fixture type with a sentinel key present in the `Account`/`Gateway` and asserts the sentinel is absent (cheap, no Docker).

### 3. Two of four GitHub Actions workflows are tag-pinned, not SHA-pinned

- **Severity:** medium
- **Confidence:** high — read the files.
- **Location:** `.github/workflows/dagger.yaml:8,10`; `.github/workflows/release.yml:59,69,85,119,134,161,178,183,190,208,228,233,240,253,260,293`.
- **Evidence:** `uses: actions/checkout@v7.0.1`, `uses: dagger/dagger-for-github@v8.4.1` (dagger.yaml); `uses: actions/checkout@v6`, `actions/upload-artifact@v7`, `actions/download-artifact@v8` (release.yml). `container.yaml` and `analysis-scorecard.yaml` *are* SHA-pinned. Dependabot explicitly excludes `release.yml` and `dagger*.yaml` (`.github/dependabot.yaml`), so they also receive no automated updates.
- **Impact:** Tag references are mutable; a compromised upstream tag runs arbitrary code in the release job, which holds `contents: write` and publishes binaries and installers. Both files are generated (cargo-dist, dagger-gha), which explains but does not remove the exposure.
- **Recommendation:** Pin by SHA with a `# vX.Y.Z` comment in both files (cargo-dist supports `github-action-commits`/custom templates; the dagger-gha generator can be post-processed), or re-include them in dependabot so tags are at least kept current. The Scorecard "Pinned-Dependencies" check will flag exactly this.

### 4. Adatkapcsolat router: no body limit by default, and the full XML is tokenized twice before authentication

- **Severity:** medium
- **Confidence:** high — read `axum.rs` and `document.rs`.
- **Location:** `crates/szamlazz-adatkapcsolat/src/axum.rs:138-158` (`DefaultBodyLimit::disable()` when no limit given; `router()` and `router_with_resolver()` pass `None`), `:245-260` (preflight runs before the key header is read); `crates/szamlazz-adatkapcsolat/src/document.rs:54-60, 94-125, 156-170` (two full passes: `root_kind` then `validate_element_namespaces`).
- **Evidence:** "Számlázz.hu publishes no maximum size and receipt batches are unbounded, so this router disables axum's default body limit." and `let root = match Document::preflight(&body) { ... }` precedes `headers.get(KEY_HEADER)`.
- **Impact:** An unauthenticated client can post an arbitrarily large body and force it to be buffered in memory (`Bytes`) and tokenized end-to-end twice, before any key check. Memory exhaustion / CPU DoS against a public receiver. The CLI `listen` command (`crates/szamlazz-cli/src/commands/listen.rs:109`) uses the unlimited `router()`, though it binds `127.0.0.1` by default.
- **Recommendation:** Make the default a generous but finite cap (e.g. 64 MiB) and provide `router_without_body_limit` as the explicit opt-out; document the trade-off. Check the `X-Szamlazzhu-Key` header *before* any parsing — the protocol needs the root element only to choose the KEY_ERR ack shape, so on key mismatch parse only far enough to find the root (or, if the header is missing entirely, return 401 without parsing at all). Consider `tower::limit`/`timeout` layers in the README example.

### 5. Upstream szamlazz.hu response bodies can be echoed verbatim into caller-facing faults and journaled outcomes

- **Severity:** medium
- **Confidence:** high — traced the string from parser to fault.
- **Location:** `crates/szamlazz-agent/src/xml.rs:69-71, 77-81`; `crates/szamlazz-agent/src/ops/query_xml.rs:539, 601, 608, 615-617`; `crates/restate-szamlazz/src/gateway.rs:1105, 1251, 1417, 1471, 1510, 1537` (`error.to_string()` into `Unanswered::Transport`, `Unconfirmed::Transport`, `DeleteOutcome::Transport`, `SetPaymentsOutcome::Transport`, `QueryError::Transport`); `crates/restate-szamlazz/src/service/support.rs:185-191` (`read_exhausted` puts `error.message()` into the 503 body).
- **Evidence:** `ParseError::UnexpectedBody(format!("expected {expected_root} in namespace {expected_namespace}, got {local}: {text}"))` — `text` is the whole response body. On read-policy exhaustion `read_exhausted` formats the last `Unanswered` message into the `unavailable` fault returned to the caller and stored as `completion_failure`; `DeleteOutcome::Transport(String)` and `SetPaymentsOutcome::Transport(String)` are journaled data.
- **Impact:** If szamlazz.hu (or a WAF/CDN in front of it) answers with an HTML error page, or a well-formed XML with an unexpected root, that body — potentially kilobytes of HTML, or a document body with buyer data — ends up in (a) the HTTP 503 body the caller receives, (b) `sys_invocation.completion_failure`, visible in the Restate UI, and (c) for delete/set_payments, the journal itself. The agent key is *not* at risk here (it is in the request, never the response), but it is uncontrolled upstream content in error channels.
- **Recommendation:** Truncate and sanitize `UnexpectedBody` (e.g. first 256 bytes, control characters stripped), or carry only a hash/length plus the root element name. In `read_exhausted`/`Unanswered::Transport`, keep the full text for `tracing` and give the caller a short classification.

### 6. `/health` is behind identity verification; no `HEALTHCHECK`, no docs

- **Severity:** medium
- **Confidence:** high — SDK code order is explicit.
- **Location:** `~/.cargo/registry/.../restate-sdk-0.12.0/src/endpoint/mod.rs:242-250`; `Dockerfile` (no `HEALTHCHECK`); `crates/restate-szamlazz-endpoint/README.md` (no mention of `/health`).
- **Evidence:** `if let Err(e) = identity_verifier.verify_identity(&headers, path) { return error_response(...) }` executes before `if parts.last() == Some(&"health")`.
- **Impact:** Once an operator follows finding 1 and sets `identity_keys`, an unsigned Kubernetes `httpGet /health` probe is rejected and the pod flaps; without keys, `/health` works but is unmentioned so operators will not use it. Either way the readiness story is undocumented.
- **Recommendation:** Document `/health` and its interaction with identity keys; recommend a TCP probe (or a signed probe) when keys are set. Optionally add a `HEALTHCHECK` (would need a tiny HTTP client in the slim image, or leave to the orchestrator and say so).

### 7. Container runtime image is Debian slim rather than a distroless/static base

- **Severity:** low
- **Confidence:** high.
- **Location:** `Dockerfile:25-30, 36-44`.
- **Evidence:** `FROM debian:13-slim@sha256:...` then `apt-get install ca-certificates`; runs as `USER 65532:65532`; `CMD ["restate-szamlazz"]`.
- **Impact:** Non-root and digest-pinned is good, but the image still ships a shell, `apt`, and libc tooling that a compromised process could use for lateral movement. reqwest is built with `rustls` + platform verifier (`crates/szamlazz-agent/Cargo.toml:24`), so only CA roots are needed.
- **Recommendation:** `gcr.io/distroless/cc-debian13:nonroot` (or `static` with a musl build) as the final stage; keep the digest pin. Add `--read-only` / `readOnlyRootFilesystem` guidance to the README since the binary needs no writable FS.

### 8. Agent key is available only via inline TOML or environment variable — no file-based secret source

- **Severity:** low
- **Confidence:** high.
- **Location:** `crates/restate-szamlazz-endpoint/src/config.rs:39-56`, `config/sources.rs:36-93`; README `:91-98`.
- **Evidence:** Sources are the config file and `RESTATE_SZAMLAZZ_*` env vars; there is no `agent_key_file` / `*__AGENT_KEY_FILE` convention.
- **Impact:** Environment variables are readable through `/proc/<pid>/environ`, `docker inspect`, `kubectl describe`, and crash dumps; Kubernetes/Docker secrets are most safely consumed as mounted files. The README rightly discourages inline TOML but then steers to env vars as the only alternative.
- **Recommendation:** Support `agent_key_file` (path) per account, or the `_FILE` suffix convention on the env override, and document it as the preferred production path.

### 9. `Endpoint` accepts URLs with userinfo and plain `http`; the endpoint string is journaled and logged

- **Severity:** low
- **Confidence:** high.
- **Location:** `crates/restate-szamlazz/src/account.rs:180-190` (`Endpoint::parse` only checks scheme and host), `:247-248` (doc: "an endpoint URL may carry userinfo"); journaled in `tests/journal/resolution/*.json` (`"endpoint": ...`); logged at start-up `main.rs:162`.
- **Evidence:** `Some("http" | "https") => {}` and no check on `uri.authority().map(|a| a.as_str().contains('@'))`.
- **Impact:** `https://user:pass@host/` would be journaled in the `account` step (visible in the Restate UI for the retention period) and printed in the start-up log. `http://` to a non-loopback host sends the agent key in cleartext. Both are operator errors, but the type is meant to guard exactly this ("an `Account` never carries an endpoint the client cannot post to").
- **Recommendation:** Reject userinfo in `Endpoint::parse`; warn at start-up (and in `--check-config`) when the scheme is `http` and the host is not loopback.

### 10. `credential_ref` and store error text reach the caller in the `unavailable` fault

- **Severity:** low
- **Confidence:** high.
- **Location:** `crates/restate-szamlazz/src/service/prologue.rs:138-149`; `crates/restate-szamlazz/src/account.rs:355-360`.
- **Evidence:** `Fault::unavailable(format!("credentials of account {} could not be fetched ({error}); ...", account.id))` where `FetchError::Gone` displays as `no credentials under reference {credential_ref}`.
- **Impact:** For the static resolver the ref is the account id (harmless). For a database/vault-backed store the ref is likely a secret path (`vault:kv/tenants/acme/szamlazz`), which is then disclosed to the caller in a 503 body and stored as `completion_failure`. CONTEXT.md classifies the ref as "not a secret", but it is still internal topology.
- **Recommendation:** Keep `credential_ref` in the `tracing::warn!` only; give the caller `"credentials of account {id} could not be fetched"` without the ref.

### 11. Duplicate-credentials fan-in check can be defeated by an account that has several agent keys

- **Severity:** low
- **Confidence:** medium — relies on szamlazz.hu allowing multiple agent keys per account ("Számla Agent kulcsok", plural, `credentials.rs:7`), which the code comments themselves state.
- **Location:** `crates/restate-szamlazz/src/account/static_resolver.rs:363-408`; ADR 0006 `docs/adr/0006-...md:125-130`.
- **Evidence:** Load-time uniqueness is on `id`, `supplier_id`, and `(endpoint, agent_key)`. Two scopes configured with two *different* keys of the *same* szamlazz.hu account and two different (one wrong) `supplier_id`s pass every check; `check_account` cannot verify `supplier_id` (README `:177`).
- **Impact:** Rule 1 (one account ⇔ one scope) is then violated silently. Safety still holds in practice because the first found document fails the supplier pin (`Collision`/`account_mismatch`) and szamlazz.hu's order-number-repetition toggle stops concurrent duplicates — but this is the *second* guard, and the ADR presents the load-time check as enforcing "the checkable half".
- **Recommendation:** Say so explicitly in the ADR/README ("two keys of one account are not detected at load; a wrong `supplier_id` is detected on the first found document"). Consider a one-time optional probe at start-up that issues nothing but *queries* a known document number per account to learn `szallito/id` and compare it against config (opt-in, since it needs a number).

### 12. Requests flowing into journal step names and external ids are not length-bounded

- **Severity:** low
- **Confidence:** high.
- **Location:** `crates/restate-szamlazz/src/contract/request.rs:92, 106, 225` (`invoice_number: String`, no validation); `crates/restate-szamlazz/src/service/support.rs:655` (`format!("hint-storno-{number}")`); `crates/restate-szamlazz/src/identity.rs:172, 179` (`{namespace}:{order}:storno:{number}`).
- **Evidence:** `OrderKey` is validated (1–64 bytes, no control chars, `identity.rs`), but `invoice_number` in `StornoRequest`, `CorrectInvoiceRequest`, `SetPaymentsRequest` and the `Selector` is a bare `String`.
- **Impact:** A caller can inject a multi-kilobyte or control-character-laden string into journal entry names, external ids (which szamlazz.hu stores), and log lines. Bounded only by Restate's ingress body limit. The agent crate does reject characters XML 1.0 forbids (`RequestError::InvalidXmlCharacter`), so the XML side is safe.
- **Recommendation:** Add an `InvoiceNumberInput` newtype with the same discipline as `OrderKey` (length cap, printable, trimmed) and reject the rest as `invalid_input` before the prologue.

### 13. Logs and spans do not carry the account id or scope

- **Severity:** low
- **Confidence:** high.
- **Location:** `crates/restate-szamlazz/src/gateway.rs:813-818, 914-921, 1190, 1233, 1271, 1324` (spans carry `external_id`, `kind`, `number`, `prefix`); `crates/restate-szamlazz-endpoint/src/main.rs:65-66` (`tracing_subscriber::fmt()` text only).
- **Evidence:** `tracing::info_span!("gateway.create", external_id = %request.external_id, kind = %request.kind, reversed = ...)` — no `account`/`scope` field; the `credentials_rejected` warning tags `namespace` and `code` only (`support.rs:119-123`).
- **Impact:** In a multi-account deployment a `warn` like "szamlazz.hu rejected the agent credentials" cannot be attributed to an account from the endpoint's logs alone; the operator has to cross-reference the Restate invocation. The `Account.id` and scope are already journaled and deemed safe to show in the UI, so they are safe to log. No JSON log format is offered for log aggregation.
- **Recommendation:** Add `account = %account.id` and `scope` to the prologue's span (and propagate it through the gateway spans); add a `--log-format json` / `LOG_FORMAT` switch using `tracing_subscriber::fmt().json()`.

### 14. `IpnRejection` and `ParseError` echo caller-controlled strings in 400 bodies

- **Severity:** low
- **Confidence:** high.
- **Location:** `crates/szamlazz-adatkapcsolat/src/axum.rs:249, 279` (`error.to_string()` into the 400 body); `crates/szamlazz-adatkapcsolat/src/error.rs:36-37, 42-50` (`UnknownRoot(String)`, `WrongNamespace { root, actual }`); `crates/szamlazz-ipn/src/axum.rs:56-63`.
- **Evidence:** `Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response()`.
- **Impact:** Reflected attacker-controlled text (element names, namespaces, quick-xml position messages) in a `text/plain` response. Not exploitable as XSS given the content type, and useful for the legitimate sender's logs — but it is unauthenticated reflection.
- **Recommendation:** Keep the detailed error in a server-side log; send a fixed short body. At minimum cap the echoed element/namespace length.

### 15. IPN receiver mitigation guidance is thin

- **Severity:** low
- **Confidence:** high.
- **Location:** `crates/szamlazz-ipn/src/lib.rs:17-19, 68-77`; `crates/szamlazz-ipn/README.md:39`.
- **Evidence:** "IPN is unauthenticated by design. Register a URL containing an unguessable path segment, and optionally treat `SOURCE_IPS` as a defense-in-depth signal rather than authentication."
- **Impact:** A forged IPN can mark an invoice as paid in the integrator's system. The README stops at "unguessable path"; it does not tell integrators the one robust mitigation this workspace already provides: treat IPN as a *hint* and confirm the payment status by querying the document (`Szamlazz.Agent.query` / `QueryInvoiceXml`) before acting. The axum extractor inherits axum's default 2 MiB body limit, which is fine.
- **Recommendation:** Add "confirm via a Számla Agent query before acting on the amounts" to the IPN README and module docs; mention rate limiting since deliveries are retried up to 10×.

### 16. `RawResponse` `Debug` includes response headers (may contain `Set-Cookie: JSESSIONID`)

- **Severity:** low
- **Confidence:** high — no production Debug print of `RawResponse` was found, so this is a latent hazard.
- **Location:** `crates/szamlazz-agent/src/wire.rs:141-145` (`#[derive(Debug, Clone)] pub struct RawResponse { headers, body }`); `crates/szamlazz-agent/src/client.rs:191-203` (all response headers copied in).
- **Evidence:** Contrast with `WireRequest`, whose hand-written `Debug` prints only `body_len` and `has_session_cookie` (`wire.rs:39-49`).
- **Impact:** The `JSESSIONID` is a 90-minute session credential for the account. A future `tracing::debug!(?raw)` would leak it and the whole body.
- **Recommendation:** Hand-write `Debug` for `RawResponse` the same way as for `WireRequest` (header names only, body length).

### 17. Configuration `Cli` types hold secrets as plain `String` with derived `Debug`

- **Severity:** info
- **Confidence:** high — no `{:?}` of these structs exists today.
- **Location:** `crates/szamlazz-cli/src/main.rs:10, 16-22` (`#[derive(Debug, Parser)] struct Cli { agent_key: Option<String> }`); `crates/szamlazz-cli/src/commands/listen.rs:20, 30-31` (`adatkapcsolat_key: Option<String>`).
- **Evidence:** Both correctly use `hide_env_values = true` and steer users to env vars.
- **Impact:** A stray `dbg!(&cli)` or clap error path would print the key.
- **Recommendation:** Use `szamlazz_agent::AgentKey` (or `Secret`) as the field type; clap accepts any `FromStr`.

### 18. No zeroization of key material

- **Severity:** info
- **Confidence:** high.
- **Location:** `crates/szamlazz-agent/src/credentials.rs:12, 51-63`; `crates/restate-szamlazz/src/config.rs:316`.
- **Evidence:** `pub struct AgentKey(String);`, `password: String` — no `Zeroize`/`ZeroizeOnDrop`.
- **Impact:** Keys linger in freed heap pages; relevant only for memory-dump attackers. The static resolver necessarily keeps all keys in memory for the process lifetime anyway.
- **Recommendation:** Optional `zeroize` feature on `AgentKey`/`Secret`; low priority.

### 19. Container and release artifacts carry no provenance/signature

- **Severity:** info
- **Confidence:** high.
- **Location:** `.github/workflows/container.yaml`; `.github/workflows/release.yml:67, 127` (`curl ... | sh` installs of cargo-dist and rustup, version-pinned but not checksummed).
- **Evidence:** No `attest-build-provenance`, `cosign`, or `sbom` steps; `permissions: contents: write` on release.
- **Impact:** Consumers cannot verify that `ghcr.io/sagikazarmark/restate-szamlazz:vX` was built from the tagged commit.
- **Recommendation:** Add `actions/attest-build-provenance` (and `sbom: true`/`provenance: true` on `docker/build-push-action`); cargo-dist supports GitHub attestations via `github-attestations = true`.

### 20. Buyer PII and bank account details are journaled; documented in ADRs but not surfaced to operators

- **Severity:** info
- **Confidence:** high.
- **Location:** `crates/restate-szamlazz/src/gateway.rs:50-51` ("A journaled document therefore includes the buyer block szamlazz.hu returned with it"); `docs/adr/0001-...md:39, 71-75`; `crates/restate-szamlazz/src/service/handlers.rs:54-55` (`journal_retention = "3d"`, `idempotency_retention = "30d"`); `tests/journal/resolution/*.json` (`seller.bank_account` in the journaled `Account`).
- **Evidence:** Every issuing handler's input (buyer name/address/tax number) and every `InvoiceDocument` outcome is journaled for 3 days and visible in the Restate UI / SQL API; responses carry no buyer data (`contract/response.rs:560, 596`), so the 30-day idempotency store holds only numbers and totals.
- **Impact:** This is a reasonable design (short retention, no PII in stored completions), but the endpoint README — the operator-facing document — never says "the Restate journal holds buyer PII for 3 days; treat the Restate data volume and UI as in-scope for GDPR".
- **Recommendation:** One paragraph in the endpoint README's operations section; note that `journal_retention` is fixed in code, not configurable.

### 21. `--check-config` tests do not assert the key is absent from output

- **Severity:** info
- **Confidence:** high.
- **Location:** `crates/restate-szamlazz-endpoint/tests/check_config.rs` (no negative assertion on a sentinel key); `main.rs:143-198` prints only `namespace`, `scope`, `id`, `mode`, `endpoint`, `supplier_id`.
- **Evidence:** The unit tests in `account.rs:441-` and `static_resolver.rs:646-662` do assert redaction of `Debug` renderings, but the process-level stdout/stderr of `--check-config` is not scanned.
- **Impact:** Regression guard only.
- **Recommendation:** Give the fixture a sentinel `agent_key` and assert `!stdout.contains(KEY) && !stderr.contains(KEY)` in both the valid and the error-path tests.

### 22. `cargo-deny` is installed in the dev shell but has no `deny.toml`

- **Severity:** info
- **Confidence:** medium — the remote dagger module may run `cargo audit` (advisory-db is pinned in `dagger.lock`), but nothing configures license/ban/source checks.
- **Location:** `devenv.nix:12`; repo root (no `deny.toml`); `.cargo/audit.toml` (two justified ignores).
- **Impact:** License and duplicate-crate policy are unenforced.
- **Recommendation:** Add a minimal `deny.toml` (licenses allow-list, `sources` restricted to crates.io) and run `cargo deny check` in the dagger check.

---

## Verified claims (what is done well)

- **No serde on credentials.** `AgentKey` and `Credentials` implement neither `Serialize` nor `Deserialize` (`credentials.rs`, whole file); `Secret` is `Deserialize`-only (`config.rs:337-367`). `Debug` is redacted on all three (`credentials.rs:27-31, 86-96`; `config.rs:331-335`), and on `WireRequest` (`wire.rs:39-49`). Tests assert redaction of `Account`, `Accounts`, `Gateway`, `StaticConfig`, `StaticResolver`, and every resolver/store error (`account.rs:441-`, `static_resolver.rs:646-662`).
- **Duplicate-credentials error never echoes the key** (`static_resolver.rs:217-223`, test at `:778-813`). Resolver/store `Unavailable` display never echoes the source (`account.rs:320-364`, tests `prologue.rs:230-279`).
- **Fresh client per execution is real.** `Gateway::open` → `Client::builder().build()` → `default_http_client()` constructs a new `reqwest::Client` with its own `cookie_store(true)`, 60 s timeout, and `redirect::Policy::none()` on every call (`gateway.rs:766-772`; `client.rs:99-111, 133-146`). Nothing in `Order`/`Agent` holds a gateway or client; `Execution` is built and dropped per handler (`prologue.rs:21-30`, `support.rs:388-426`).
- **Fails closed when protocol v7 is off in multi-account mode.** `ctx.scope()` → `None` → `Shape::Multi` → `ResolveError::Unscoped` → journaled `Resolution::Unscoped` → `unknown_account` (400), nothing sent (`support.rs:404-418`; `static_resolver.rs:444`; `prologue.rs:63, 75-77`). The single-account shape issuing on its one account under a scoped path is the documented residual, with `check_account` as the canary.
- **Credentials fetched outside the journal every execution**, with a 3×200 ms in-process retry and terminal `unavailable` (`prologue.rs:97-134`).
- **`account_mismatch` names pins, never the key** (`support.rs:232-245`); `credentials_rejected` logs namespace + code only (`support.rs:119-123`).
- **Journal leak scan design is correct**: hex-decodes `raw` (not `entry_json`), covers every row and `completion_failure`, and has a positive control (`tests/service.rs:534-575, 4148-4201`).
- **Adatkapcsolat**: XOR-accumulate key comparison (`axum.rs:332-345`); handler errors answered with a bare 500 (`axum.rs:351-353`); missing header → 401 (keeps szamlazz.hu retrying) rather than KEY_ERR; quick-xml `de::from_str` uses `PredefinedEntityResolver` and never fetches external entities, so XXE and entity-expansion attacks are structurally absent (verified in `quick-xml-0.42.0/src/de/mod.rs:3173, 2467`).
- **Endpoint binary**: SIGTERM/SIGINT handled before bind, SDK connection drain, exit 0, tested (`main.rs:76-101, 109-123`; `tests/stop.rs`); `--check-config` builds the full endpoint without listening and prints no key (`main.rs:143-198`).
- **Container**: multi-stage, digest-pinned `xx`, `rust` and `debian` images, `cargo fetch/build --locked`, numeric non-root `USER 65532:65532`, `STOPSIGNAL SIGTERM`, `.dockerignore` excludes `restate-szamlazz.{toml,json,yaml,yml}` and `.env*`; `compose.yaml` binds Restate ports to `127.0.0.1`.
- **Supply chain**: `Cargo.lock` committed; `unsafe_code = "forbid"`, `clippy::all`/`pedantic` and `unwrap_used` at `warn` (`Cargo.toml:47-54`); no `unwrap`/`expect` in production paths of `gateway.rs`, `service/*.rs` (spot-checked); dependabot for cargo/docker/actions; OpenSSF Scorecard workflow; `.cargo/audit.toml` ignores two advisories with written justification; `dagger.lock` pins the module SHAs and the rust image digest.
- **Config loader is strict** (`deny_unknown_fields`-equivalent at every level, both-shapes refusal, pre-release layout refused) and its error messages name keys and sources, not values (`config/schema.rs`).

## Questions I could not resolve

1. **Does the dagger `check` pipeline run the ignored e2e suite (with Docker) and `cargo clippy -D warnings`/`cargo audit`?** The module is remote (`github.com/sagikazarmark/daggerverse-beta/rust`); nothing in the repo shows the commands. Finding 2 assumes it does not.
2. **Panic containment.** If a handler future panics (e.g. an `expect` in a rarely hit path), does the Restate SDK's `HttpServer`/hyper connection task swallow it and keep the process alive, or does the runtime exit? Not verified in `restate-sdk-0.12.0/src/http_server.rs`; a `catch_unwind` or an explicit panic hook is absent from `main.rs`.
3. **Deep-nesting behaviour of `quick_xml::de`** on the Adatkapcsolat receiver: whether a pathologically nested body causes recursion proportional to depth (stack) or is skipped iteratively. Relevant only together with finding 4.
4. **Whether szamlazz.hu actually allows several active agent keys per account** (finding 11) — the crate docs imply yes; not observed.
5. **What `x-restate-*` headers Restate 1.7.8's ingress forwards under protocol v7** beyond `x-restate-ingress-path`; the ADR's header-stripping rule is defence in depth, but the concrete header list was not re-verified here.
6. **Restate ingress default request-body limit**, which bounds finding 12 — not checked against the 1.7.8 source.
