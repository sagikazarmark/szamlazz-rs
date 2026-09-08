# Review 06, Test coverage and test quality

Repository: `/home/laborant/szamlazz-rs2` @ `0e4238c` (main, PR #56 merged).
Scope: every test target in the workspace, the e2e harness, the journal-compatibility fixtures, CI (dagger), and the upstream fixture corpus. No files in the repository were modified.

---

## (a) Test run summary

### 1. The documented full run

```
cargo test --workspace --all-targets --all-features --locked
```

| Binary | Passed | Failed | Ignored | Wall time |
|---|---|---|---|---|
| `restate-szamlazz` lib (unit) | 126 | 0 | 0 | 0.12 s |
| `restate-szamlazz` `tests/gateway.rs` (wiremock) | 63 | 0 | 0 | 1.38 s |
| `restate-szamlazz` `tests/service.rs` | 3 | 0 | **1** (`e2e_order_protocol`, `needs docker`) | 0.28 s |
| `restate-szamlazz-endpoint` bin (unit) | 24 | 0 | 0 | 0.08 s |
| `restate-szamlazz-endpoint` `tests/check_config.rs` | 5 | 0 | 0 | 1.16 s |
| `restate-szamlazz-endpoint` `tests/stop.rs` | 4 | 0 | 0 | 0.04 s |
| `szamlazz-adatkapcsolat` lib | 11 | 0 | 0 | 0.00 s |
| `szamlazz-adatkapcsolat` `tests/archive.rs` | 7 | 0 | 0 | 0.01 s |
| `szamlazz-adatkapcsolat` `tests/fanout.rs` | 6 | 0 | 0 | 0.00 s |
| `szamlazz-adatkapcsolat` `tests/protocol.rs` | 23 | 0 | 0 | 0.18 s |
| `szamlazz-agent` lib | 144 | 0 | 0 | 0.07 s |
| `szamlazz-agent` `tests/client.rs` | 5 | 0 | 0 | 0.10 s |
| `szamlazz-agent` `tests/live.rs` | 0 | 0 | **3** (`requires SZAMLAZZ_AGENT_KEY for a test-mode account`) | 0.00 s |
| `szamlazz-cli` bin | 0 | 0 | 0 | - |
| `szamlazz-ipn` lib | 13 | 0 | 0 | 0.00 s |
| **Total** | **434** | **0** | **4** | **10.2 s wall** (build cached by a concurrent process; test execution itself ≈ 3.5 s) |

Doctests are excluded by `--all-targets`; run separately (`cargo test --workspace --locked --doc`): 7 passed (restate-szamlazz 1, adatkapcsolat 3, agent 2, ipn 1), 0 failed.

### 2. The ignored / environment-gated tests

| Test | Gate | Behaviour when the gate is closed |
|---|---|---|
| `service.rs::e2e_order_protocol` | `#[ignore = "needs docker"]`; additionally `docker info` must succeed | With `--ignored` on a host without docker it prints `skipping: docker daemon not available` and **returns Ok**, a silent green pass (service.rs:1460-1463). |
| `live.rs::{taxpayer_query, invoice_lifecycle, proforma_lifecycle}` | `#[ignore]`; `SZAMLAZZ_AGENT_KEY` env var | With `--ignored` and no key: `expect()` panics → fails loudly (good). |
| `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` | optional; reuse a running server instead of starting a container | not a skip, an alternative. |

### 3. The e2e suite, run here (docker is available on this host)

```
cargo test -p restate-szamlazz --test service --locked -- --ignored --nocapture
```

- Run 1: **1 passed** (all 36 scenario `pass` lines printed), 42.8 s test time, 1 m 05 s wall including compile.
- Run 2 (re-run for flakiness): **1 passed**, 39.1 s. No flakiness observed in two runs.
- Final scenario reported: `no agent key in 746 journal entries of 73 invocations (28 scoped); positive control found`.

### 4. What CI actually runs (dagger)

`.github/workflows/dagger.yaml` runs `dagger check` with `dagger.toml` → module `github.com/sagikazarmark/daggerverse-beta/rust` @ `ce82b85`, base image `rust:1.98-slim-trixie`. I fetched `rust/main.dang` and `rust/cargo.dang` at that commit. `check` runs every `@check` function with defaults: **`cargo test`** (plain: no `--workspace` flag is needed for a virtual manifest, but **no `--all-features`, no `--all-targets`, no `--locked`**), `cargo build`, `cargo clippy --no-deps`, `cargo doc --no-deps` with `RUSTDOCFLAGS=-D warnings`, `cargo audit` (with `.cargo/audit.toml` ignores), `cargo fmt --check`.

Consequences, reproduced locally with `cargo test --locked -- --list` (feature set identical to CI):

| Target | `--all-features` | CI-equivalent | Why |
|---|---|---|---|
| `szamlazz-adatkapcsolat/tests/archive.rs` | 7 tests | **0 tests** | `#![cfg(feature = "opendal")]`; no workspace member enables `opendal`, so feature unification does not rescue it |
| `szamlazz-adatkapcsolat` lib | 11 | **10** | `archive.rs::timestamped_write_retries_without_replacing_an_existing_version` is `cfg(feature = "opendal")` |
| `szamlazz-agent/tests/client.rs`, `live.rs` | 5 / 3 | 5 / 3 | rescued only because `restate-szamlazz` and `szamlazz-cli` enable `client-reqwest` (resolver 3 unifies across the members built together) |
| `szamlazz-adatkapcsolat/tests/protocol.rs` | 23 | 23 | rescued only because `szamlazz-cli` enables `axum` |
| `restate-szamlazz` schema test | runs | runs | rescued because `restate-szamlazz-endpoint` enables `schemars` |
| `e2e_order_protocol` | ignored | ignored, **and no docker daemon in the dagger container** | never runs in CI |
| `live.rs` | ignored | ignored | never runs in CI (by design) |

So the Restate worker's end-to-end behaviour (the only place the handler decisions of `Szamlazz.Order`/`Szamlazz.Agent` are tested) is verified exclusively on developer machines.

---

## (b) Coverage MAP

Legend for "Assertion strength": **S** = full outcome/shape asserted, including wire body/`expect(n)`; **M** = outcome variant asserted, some fields; **W** = "doesn't panic"/loose; **-** = no test. "Level": U = unit (no I/O), G = `tests/gateway.rs` (wiremock, no Restate), E = `tests/service.rs` e2e (docker, not in CI).

| Critical path | Test(s) | Level | Strength | Gap? |
|---|---|---|---|---|
| **Lookup step: absent** | `lookup_with_nothing_under_the_id_or_the_order_is_absent` gateway.rs:389 | G | S (2 bodies, order of queries) | - |
| Lookup: live → `already_issued` / `conflict{live}` with `reissue` | `lookup_finds_our_live_document_and_takes_no_hint` gateway.rs:407; e2e (i) service.rs:1509, (v) :1757 | G+E | S | handler branch `Live if reissue` only e2e |
| Lookup: reversed + storno number from hint | `lookup_of_our_reversed_document_names_its_storno_from_the_hint` :475, `…has_no_storno_number_when_the_hint_is_not_its_storno` :508; e2e (iv) :1686, (vi) :1786 | G+E | S | - |
| Lookup: collision (order/kind/teszt/supplier) | `lookup_of_an_invalid_document_under_our_id_is_a_collision` :437; `opened_gateway_validates_the_accounts_mode_against_teszt` :2365; `Lookup::classify` unit tests.rs:662; e2e (ix) :2206 | U+G+E | S | - |
| Lookup: foreign (plain / conversion / beside reversed) | `lookup_reports_a_live_invoice_under_the_order_that_is_not_ours_as_foreign` :542; `lookup_hint_ignores_our_documents_non_invoices_and_its_own_failure` :585; e2e (x-a) :2303 | G+E | S | - |
| Lookup: corrective takes no hint | `lookup_of_a_corrective_takes_no_hint` :700; `corrective_with_a_live_base_under_the_order_is_issued` :724 | G | S (`expect(0)` on the hint, body asserts `helyesbitettSzamlaszam`) | handler `correct()` never e2e |
| Lookup: Unanswered vs Api data | `lookup_without_an_answer_is_unanswered_not_data` :624; `lookup_answered_with_another_code_is_data` :670 | G | S | - |
| **Create step: send only when nothing under id** | `create_with_nothing_under_the_id_sends_the_create_and_is_issued` :773 (asserts `szamlaKulsoAzon`, `rendelesSzam`, buyer on the wire) | G | S | - |
| Create: re-executed after lost reply → `Found`, no send | `create_re_executed_after_a_lost_reply_finds_the_document_and_sends_nothing` :807 (`expect(1)`, 4 bodies) | G | S | - |
| Create: send past exactly the lookup's reversed doc | `create_past_the_reversed_document_the_lookup_saw_sends_the_create` :847 | G | M (Issued number) | - |
| Create: never past a reversal the lookup did not see | `create_never_sends_past_a_reversal_the_lookup_did_not_see` :867 (`expect(0)`); e2e (vi-b) :1850 | G+E | S | - |
| Create: lookup's reversed reported live → `LiveAgain` | `create_never_sends_when_the_lookups_reversed_document_is_reported_live` :902; unit `a_settled_create_step_maps_onto_the_response` create.rs:833 | G+U | S | - |
| Create: lost reply, immediate re-query finds it | `create_with_a_lost_reply_whose_re_query_finds_the_document_reversed_is_settled` :925; `create_with_an_open_outcome_is_found_when_the_re_query_sees_the_document` :1083; e2e (vi-c) :1914 (no run failure recorded, 1 create) | G+E | S | - |
| Create: Unconfirmed (500 / 1 / 55 / `szlahu_down` / no number) → retry → `outcome_unknown` | `create_with_an_open_outcome_re_queries_once_and_is_unconfirmed_when_nothing_landed` :1037; e2e (xi) :2513 (500 fault, `retry_count`, `failing_commands == ["create-invoice"]`, both steps journaled) | G+E | S | - |
| Create: failed leading query never sends | `create_never_sends_when_the_leading_query_is_not_a_clean_miss` :965 | G | S | - |
| **71/152**: reconciled / collision / reversed / named / unnamed / contradiction / corrective | `duplicate_order_number_*` :1131–1264 (7 tests, `expect(0)`/`expect(1)` on the hint); e2e (iii) :1620 | G+E | S | - |
| Credential codes 3/135/136/164 on every op | `credential_codes_on_*` :1321, :1352, :1380, :1406, :1541, :1754, :1928, :2152, :2172; probe :1623; taxpayer :2499 | G | S | `credentials_rejected` fault e2e only via unit leak tests (tests.rs:504) |
| **Storno**: verify → lookup → storno (order path) | e2e (iv) :1686, (xviii) :3519 (`verify-SZ-23`, `lookup-storno-SZ-23`, `storno-SZ-23` journaled) | E | S | `Szamlazz.Order.storno_invoice` `conflict{not_managed}`, `account_mismatch`, already-reversed-on-verify, `not_stornoable` by tipus: **no test at any level** (storno.rs:132–150) |
| Storno step: validated reversal / zero gross / echo no-op / 14 & 221 / already reversed / lost reply | gateway.rs :1800, :1831, :1858, :1877, :1900, :1975, :2010, :2051 (body asserts `szamlaKulsoAzon`, `megjegyzes`, `eszamla`, no `keltDatum`) | G | S | - |
| `Szamlazz.Agent.storno`: pins, unpinned supplier, `managed_by_order`, no echo of other order | e2e (xviii-b) :3652 | E | S | - |
| `Szamlazz.Agent.query`: pins / projection / 404 | e2e (xviii-c) :3812; `query_response_projects_a_queried_document` response.rs:1115 | E+U | S | - |
| **Proforma link** (`auto`) & consumed derivation in `get` | e2e (vii) :1969 (`dijbekeroSzamlaszam` on the wire, `consumed`/`by`) | E | S | `proforma: none` → `conflict{proforma_live}`: **untested**; `proforma_missing` (7 on verify): **untested**; consumed-derivation logic (`status()` storno.rs:248) only e2e |
| `options.proforma: {number}`: teszt pin, not_managed, this order's | e2e (vii-b) :2057 (`verify-proforma-D-30` last step, `requests_seen == 2`) | E | S | - |
| `create_proforma` after own invoice → `order_invoiced`; foreign | e2e (x-a) :2303; `exclusivity_table_names_the_other_kinds_and_their_reasons` create.rs:770 | E+U | S | - |
| `create_prepayment`: refuses `options.proforma`, no proforma lookup | e2e (x) :2248 (`expect(0)` on the proforma id); `options_proforma_applies_to_create_invoice_only` create.rs:793 | E+U | S | - |
| `create_final`: `prepayment_missing`, `prepayment_reversed`, `elolegSzamlaszam` | (build.rs `kind_references_and_input_errors` :392 for the reference only) | U | W | **handler never invoked by any test** (create.rs:446) |
| `correct_invoice`: base verify → not_managed / base_reversed / not_found | - |, | - | **handler never invoked by any test** (create.rs:275) |
| `delete_proforma`: absent / collision / paid guard (`force`) / deleted / 335 | `delete_proforma_outcomes` gateway.rs:2088; e2e (x-b) malformed body only | G | S at gateway | handler decisions (storno.rs:157–214, `proforma_paid`, `not_deleted{external_id_collision}`) **untested** |
| **set_payments**: replace/additive, 463, transport, 6 entries | `set_payments_outcomes` :2200 (asserts `<additiv>true</additiv>`, `jogcim`, `osszeg`); `the_set_payments_fault_tells_an_additive_caller_to_query_first` agent.rs:341 | G+U | S | handler never e2e; `run_once`/`max_attempts(1)` pinned only by discovery test tests.rs:175 |
| **query_taxpayer**: stem & full form one step; neither form → 400; 422 pass-through; `valid:false` 200 | `query_taxpayer_request_derives_the_prefix_from_either_form` request.rs:539, `…refuses_every_other_form` :553; agent.rs :381, :395, :439; gateway :2425–2558; e2e (xviii-d) :3876 | U+G+E | S | - |
| **check_account**: sentinel, one query, rejected as data, `scope: null`, unknown_account | gateway :1576–1663; `the_probe_outcome_is_data` agent.rs:465; e2e (xii-b) :2932, (xvii-c) :3451 (full JSON body equality) | G+U+E | S | `scope: null` **under a scoped call** (protocol v7 off) is only described, never provoked; cannot be with the flags on |
| **Prologue**: `unknown_account` (unscoped/unknown) | prologue.rs:193; e2e (xii) :2824, (xvi) :3202, (xvii-c) | U+E | S | - |
| Prologue: unavailable resolver retried, cause never echoed | prologue.rs:230, :243; e2e (xiv) :3044 (3 resolutions, `failing_commands == ["account"]`, 1 `account` entry) | U+E | S | - |
| Prologue: credential store gone / unavailable → terminal 503, 3 fetches | prologue.rs:260 (both variants); e2e (xv) :3113 (unavailable only) | U+E | S | `Gone` only at unit level |
| Prologue: credential rotation between executions; account change does not reach the invocation | e2e (xx) :4058 (byte-identical `account` entry, keys on the wire per execution), (xix) :3964 | E | S | - |
| **Malformed body** → 400 `invalid_input` with serde message, nothing journaled | `a_malformed_body_is_a_structured_invalid_input` tests.rs:305; `body_discovers_exactly_as_json` :365; `request_types_refuse_unknown_fields` request.rs:408; e2e (x-b) :2369 | U+E | S | - |
| **Untrimmed order key** → 400, nothing journaled | `the_order_key_must_arrive_trimmed` tests.rs:788; e2e (x-c) :2451 (`%20` variants) | U+E | S | - |
| **Account pins** on every finding handler | `check_pins` unit tests.rs:839; `document_ext_reads_the_checks_off_a_queried_document` gateway.rs:1748; e2e (vii-b), (xviii-b), (xviii-c) | U+E | S for `Agent.query/storno` and the proforma verify | `storno_invoice` verify and `correct_invoice` verify: **not exercised** |
| **Journaled types additive-only** | `every_variant_of_every_journaled_type_is_pinned` journal.rs:701; `every_pinned_fixture_replays_through_the_current_types` :743; harness tests :904–1010; `Journaled` bound on run helpers support.rs:442/472/504 | U | M | deserialise + re-encode superset only; **no archived shapes exist yet** so the compat test is currently a round-trip of the present shape; no cross-deployment replay under Restate |
| **Policies → `RunRetryPolicy`** | config.rs :875, :893, :917 (Debug-string equality); e2e (xi), (xi-b), (xi-c), (xiv) prove the policy's delay governs | U+E | S | Debug-string pin is brittle to SDK formatting |
| **e2e journal scan** (credentials never journaled) | e2e (xxi) :4148 (746 entries, 73 invocations, 3 sentinel keys, positive control); (i) per-invocation; `assert_not_impl_any!(Credentials/AgentKey: Serialize)` account.rs:435; `credentials_rejected_never_leaks_the_agent_key` tests.rs:504; `account_mismatch_never_leaks_the_agent_key` :614 | E+U | S | scan covers only handlers the run invoked: `set_payments`, `create_final`, `correct_invoice` journals never scanned (their `Transport(String)` variants carry client error text) |
| **Fresh client / JSESSIONID isolation** | `two_gateways_opened_from_two_accounts_share_no_key_and_no_session` gateway.rs:2314 (positive control: acme's 2nd request *does* carry the cookie) | G | S | per-execution freshness (a new client each handler execution) only implied by e2e (xx) |
| **Same order key under two scopes concurrently** | e2e (xvii) :3311 (`tokio::join!`, each key on the wire once) | E | S | - |
| **Two concurrent invocations on ONE key (same scope)** | - |, | - | **no test** (see F-2) |
| Same `Idempotency-Key` under two scopes / replay without a call | e2e (xvii-b) :3386, (ii) :1597 | E | S | - |
| Purged invocation / order Restate has no memory of | e2e (xiii) :2998, (xviii) :3519 | E | S | - |
| Flag day (private → drain → multi) | e2e (xvi) :3202 | E | S | - |
| Read policy: flaky read retried; exhausted read → 503 naming step | e2e (xi-b) :2603, (xi-c) :2686, (xi-d) :2766; `an_exhausted_read_is_a_structured_unavailable` tests.rs:740 | U+E | S | - |
| Discovery: names, handler set, `max_attempts`, timeouts, `journal_retention` | tests.rs:90, :175 | U | S | - |
| Endpoint config loader / `--check-config` / stop signals | config.rs 16 tests, schema.rs:354, check_config.rs 5, stop.rs 4, main.rs:339 (all-digit key byte-exact to wiremock) | U+I | S | - |
| **Agent op request build vs golden** (every op) | `writes_canonical_*_xml` in every `ops/*.rs` against `tests/golden/*.xml` (`include_str!`, exact equality) | U | S | goldens are project-authored; **no test parses/compares the upstream corpus** (see F-7) |
| Agent response parse incl. error headers, header-vs-body precedence, body-only 7/463, 56 with/without number | invoice.rs :1553–1982, storno.rs :251–349, credit_entry.rs :292–389, query_xml.rs :1219–1612, proforma.rs :134–172, taxpayer.rs :389–573, receipt.rs :1193–1333, wire.rs :402–438, client.rs 5 tests | U+I | S | - |
| Text (`valaszVerzio=1`) vs XML response versions | - |: | - | crate hard-codes `valaszVerzio 2` (invoice.rs:803, storno.rs:158, credit_entry.rs:183, query_pdf.rs:87); text responses are not a supported path; non-XML *bodies* are handled (`preserves_critical_text_or_html_error`, `notification_failure_with_non_xml_body_*`) |
| **Adatkapcsolat**: dispatch by root, Ack id/iktatószám, KEY_ERR on wrong key, missing key → 401, handler error → 500, KEY_DEL, fan-out merge, body limit, URL-appended key | protocol.rs 23 tests (:362–:666), ack.rs :260–:309, fanout.rs :88–:157, document.rs :1673–:1742 | I+U | S | only the synthetic `szamla.xml`; upstream `szamla_example.xml`, `szamlavalasz_example.xml`, XSDs unused |
| **IPN parse**: snapshot semantics, comma decimals, date-only, unknown params, sign-aware `is_fully_paid` | ipn lib.rs :282–:400, axum.rs :101–:128 | U | S | - |
| Go-live checklist (8 live probes) | - |, | - | acknowledged as issue #15; `live.rs` covers only create/storno/proforma/taxpayer lifecycles, none of the 8 checklist rows |

---

## (c) Findings

### F-1 (The entire handler layer of the worker is verified only by a docker-gated, ignored test that CI never runs
**Severity:** high **Confidence:** high) verified by fetching the dagger module source at the pinned commit and reproducing CI's `cargo test` feature set locally.
**Location:** `.github/workflows/dagger.yaml`, `dagger.toml`, `crates/restate-szamlazz/tests/service.rs:1457-1463`.
**Evidence:** `dagger check` → `cargo test` in `rust:1.98-slim-trixie` with no docker daemon; `e2e_order_protocol` is `#[ignore = "needs docker"]`. Design §11 itself states handler decisions are tested "end to end or as the pure functions they are extracted into", and `issue()` (create.rs:591-645), `verify_for_storno` (storno.rs:104-152), `delete()` (storno.rs:157), `status()` (storno.rs:224), `correct()` (create.rs:275) are not extracted: they are e2e-only. The e2e is the only test of: `already_issued`, `conflict{live}`, `reversed` with `storno_number`, `reissue`, `outcome_unknown` surfacing, read-policy exhaustion → 503, the prologue's journal shape, scope isolation, and the credential leak scan.
**Why it matters:** a regression in any handler branch merges green. The e2e suite passed twice here in ~40 s, so the cost of running it is low.
**Recommendation:** add a second CI job (plain GitHub Actions runner, which has docker) running `cargo test -p restate-szamlazz --test service --locked -- --ignored`, or use the dagger `Service` API to start `restatedev/restate:1.7.8` beside the test container and pass `RESTATE_ADMIN_URL`/`RESTATE_INGRESS_URL`. Also make the docker-unavailable path fail (or at least `panic!` under an env var like `E2E_REQUIRED=1`) so a misconfigured runner cannot pass silently.

### F-2, No test of two concurrent invocations on one Virtual Object key (the per-key serialisation the exactly-once claim rests on)
**Severity:** high **Confidence:** high, every `tokio::join!` in service.rs (:3331, :3999, :4103) is cross-scope or call+mutation; none races two calls on one `(scope, key)`.
**Location:** `tests/service.rs` (absent scenario); design §3/§4 ("same-key handlers run one at a time, which serializes issuing per order").
**Evidence:** (xvii) proves two *different* VO instances proceed independently. Nothing proves that two `create_invoice` calls with different `Idempotency-Key`s on the same key produce exactly one create on the wire and `issued` + `already_issued`. The glossary's `Order` entry and the untrimmed-key rationale both depend on this property.
**Why it matters:** the safety argument is "the lock plus query-first"; the lock half is asserted by no behavioural test. The static half *is* pinned: the discovery test asserts `handler.ty == None` (exclusive) for every `Szamlazz.Order` handler but `get` (tests.rs:142-143), so an accidental `#[shared]` on a create fails CI. What remains untested is that Restate actually serialises two in-flight invocations on one key end to end, and (more importantly for the protocol) that the *second* caller's lookup sees what the first one issued (the real-world "caller timed out and retried with a new `Idempotency-Key`" path).
**Recommendation:** e2e scenario `two_creates_on_one_key_with_different_idempotency_keys_issue_once`: `create()` responds after a deliberate delay (wiremock `set_delay(2s)`), `holds_after_misses`-style stub that flips on the create hit; `tokio::join!` two `call(...)` with keys `k1`, `k2`; assert one reply `issued`, the other `already_issued`, `create_bodies().len() == 1`, and both `sys_invocation` rows completed. Add the variant "retry-after-timeout while the original is still in flight": first call's create step loses its reply (1 s issue policy), second call arrives during the delay; assert it queues (its `sys_invocation` row shows `status != running` until the first completes) and answers `already_issued` with exactly one create on the wire.

### F-3: `create_final`, `correct_invoice`, `set_payments` are never invoked by any test; `delete_proforma` only with a malformed body
**Severity:** high **Confidence:** high, `grep` of `tests/service.rs` for the handler names: 0 / 0 / 0 / 1 (malformed) occurrences.
**Location:** create.rs:275-344 (`correct`), :446-478 (`prepayment_for_final`), storno.rs:157-214 (`delete`), agent.rs:187 (`set_payments_request`).
**Evidence:** the gateway pieces they use are tested (`corrective_with_a_live_base_under_the_order_is_issued`, `delete_proforma_outcomes`, `set_payments_outcomes`), but the handler decisions are not: `prepayment_missing`, `prepayment_reversed`, `base_reversed`, `not_managed` on the corrective base, `invalid_input` when the base is unknown, `proforma_paid` without `force`, `not_deleted{external_id_collision}`, `{deleted, reason: absent}`, and `set_payments`' single-attempt `run_once` + `outcome_unknown` on transport. The e2e credential-leak scan therefore never sees a `SetPaymentsOutcome::Transport(String)` or `DeleteOutcome::Transport(String)` journal entry, the only journaled variants that carry free-text client error messages.
**Why it matters:** these are the handlers with the least redundancy: one unit test of the exclusivity table (`exclusive_with(Final) == []`) is the only pin on `create_final`'s pre-checks.
**Recommendation:** add e2e scenarios: `create_final` on an order with (a) no prepayment → `conflict{prepayment_missing}`, (b) reversed prepayment → `conflict{prepayment_reversed}`, (c) live prepayment → `issued` with `<elolegSzamlaszam>ES-…</elolegSzamlaszam>` asserted on the create body; `correct_invoice` with (a) base of another order → `conflict{not_managed}`, (b) reversed base → `conflict{base_reversed}`, (c) live base → `issued` with `helyesbitettSzamlaszam`; `delete_proforma` (a) paid without `force` → `not_deleted{proforma_paid}`, (b) with `force` → deleted, (c) absent → `{deleted, reason: absent}`; `set_payments` with a 500 reply → `outcome_unknown` after **one** send (`expect(1)`), and its journal included in the (xxi) scan.

### F-4: `Szamlazz.Order.storno_invoice`'s own verify decisions are untested: `not_managed`, `account_mismatch`, already-reversed-on-verify, `not_stornoable` by `tipus`
**Severity:** medium **Confidence:** high, storno.rs:132-150 has four `Break` arms; e2e (iv)/(xviii) only drive the `Continue` arm; gateway tests exercise `Gateway::storno` but not `verify_for_storno`.
**Location:** `crates/restate-szamlazz/src/service/storno.rs:104-152`.
**Evidence:** `check_pins` is unit-tested in isolation (tests.rs:839) and e2e-tested on `Szamlazz.Agent.storno` and the proforma-by-number verify, but deleting the `check_pins(...)?` call or the `carries_order` guard from `verify_for_storno` would not fail any test. The glossary claims "every document one of its handlers finds by number … must carry this order's number (else `conflict{not_managed}`) and the resolved Account's pins": half of that claim is unverified for the storno handler.
**Recommendation:** e2e scenario on `storno_invoice`: (a) `number_query("SZ-F")` returns another order's live SZ → 200 `conflict{not_managed}`, storno mock `expect(0)`, `runs == [namespace, account, verify-storno-SZ-F]`; (b) `teszt=false` → 409 `account_mismatch`; (c) `sztornozott=true` with the hint returning its SS → `reversed{storno_number}`, nothing sent; (d) a `D` under the order → `rejected{not_stornoable}`.

### F-5 (8 tests of `szamlazz-adatkapcsolat`'s archiver never run in CI
**Severity:** medium **Confidence:** high) `cargo test --locked -- --list` shows `tests/archive.rs: 0 tests` and the lib at 10 instead of 11.
**Location:** `crates/szamlazz-adatkapcsolat/tests/archive.rs:3` (`#![cfg(feature = "opendal")]`), `src/archive.rs:471`; `dagger.toml` (no `allFeatures`).
**Evidence:** no workspace member depends on `szamlazz-adatkapcsolat` with `opendal`, so resolver-3 feature unification does not enable it under CI's plain `cargo test`. The other feature-gated test files are only *accidentally* compiled because `szamlazz-cli` and `restate-szamlazz` happen to enable `client-reqwest`/`axum`/`schemars`; removing the CLI crate would silently drop 31 more tests.
**Recommendation:** in `dagger.toml` configure the rust module's `test` with `allFeatures = true`, `allTargets = true` and `locked = true` (the module exposes them), and add `cargo test --doc` since `--all-targets` excludes doctests. Alternatively, add `[dev-dependencies] szamlazz-adatkapcsolat = { path = ".", features = ["opendal", "axum"] }`-style self-activation.

### F-6, The journal-compatibility test is a self round-trip today; it does not replay a *previous* deployment's journal through the handlers
**Severity:** medium **Confidence:** high, `tests/journal/**` contains exactly one `<variant>.json` per variant and no `<variant>.<n>.json`; the compat test decodes and re-encodes (`journal.rs:788-796`) without any handler involvement.
**Location:** `crates/restate-szamlazz/src/service/journal.rs:743-808`; `tests/journal/`.
**Evidence:** `every_pinned_fixture_replays_through_the_current_types` checks `serde_json::from_str::<T>` then `is_covered_by(fixture, re-encoded)`. Since the generator has just written every fixture from the current types, the compat test cannot currently fail except by a hand edit. It becomes a real guard only after the first archived shape exists, and the process depends on a reviewer not deleting archives (the doc says so). It also does not cover what an SDK upgrade could change (the *envelope* the SDK writes around the `Json<T>` result) nor whether the *step names* (`lookup-{kind}`, `verify-proforma-{number}`, …) stay stable, which a replay depends on as much as the payload does.
**Why it matters:** the failure mode is severe (in-flight invocations killed while holding order keys, ADR 0005), and the guard is presently inert.
**Recommendation:** (1) commit a snapshot of a real `sys_journal` from the e2e run (hex-decoded `raw` of every `Notification: Run` row plus the `Command: Run` names) under `tests/journal/e2e/<handler>.json` and replay those bytes through the current types, that pins both the SDK envelope and the step names; (2) add a unit test that the run-name set is exactly a pinned list (`namespace`, `account`, `exclusivity-*`, `proforma-link`, `prepayment-for-final`, `lookup-*`, `create-*`, `verify-*`, `lookup-storno-*`, `storno-*`, `delete-proforma-*`, `get-*`, `set-payments-*`, `probe`, `taxpayer-*`, `query`); (3) longer term, a two-revision e2e: register revision A, start a create whose reply is lost (1 s policy → 2 m in the test policy is too short; use a wiremock `set_delay` on the create to hold execution 1 in flight), register revision B built from the current tree, and assert the invocation completes on B.

### F-7, The upstream fixture corpus (60 files) is referenced by zero tests; parsers are proven only against hand-reduced synthetic copies
**Severity:** medium **Confidence:** high, `grep -rn upstream --include='*.rs' crates` → 0 hits; the `tests/upstream` symlinks exist in two crates but nothing reads through them.
**Location:** `fixtures/upstream/**`, `fixtures/SOURCES.md`, `crates/szamlazz-agent/tests/upstream`, `crates/szamlazz-adatkapcsolat/tests/upstream`.
**Evidence:** `fixtures/synthetic/agent/szamla_query.xml` is 1 345 B vs the upstream 3 301 B (102 differing lines); the Adatkapcsolat upstream `szamla_example.xml` (133 lines, with `xsi:schemaLocation`, `<adoszam>202 336 0856</adoszam>`, empty `<bankszamla></bankszamla>`, heavy whitespace) differs from the synthetic `szamla.xml` on every line. The SOURCES.md licence note forbids *packaging* the corpus, which `exclude = ["tests/upstream"]` already handles: it does not forbid reading it at test time.
**Why it matters:** the design's stated reason for wiremock over a fake is reviewability against the verified facts; the official example shapes are the closest thing to "the truth" available offline and they are never parsed.
**Recommendation:** add a workspace-only test per crate that reads `tests/upstream/**` at runtime with `std::fs` (not `include_bytes!`, so nothing enters the package), skipping with a message when the symlink target is absent (a published crate), and asserts every `responses/*.xml` parses through its op's parser and every `requests/*.xml` round-trips through the builder's field model (or at least parses as the request type where one exists). For Adatkapcsolat: `Document::parse(szamla_example.xml)` and the `szamlavalasz_example.xml` Ack shape.

### F-8, The e2e journal scan does not cover the by-number `Szamlazz.Agent.set_payments` and the `Transport(String)` journal variants
**Severity:** low **Confidence:** medium, reasoning from which handlers ran (F-3) and which outcome variants embed error text (`DeleteOutcome::Transport`, `SetPaymentsOutcome::Transport`, gateway.rs `Failure::Transport`).
**Location:** `tests/service.rs:4148` (`no_agent_key_in_any_journal_of_the_run`).
**Evidence:** the scan is sound where it looks (hex-decoded `raw`, `completion_failure`, positive control) but the run never journals a transport-error string from the agent client. Today's `reqwest` error `Display` does not include request bodies, so this is a latent rather than actual leak, but it is exactly the kind of assumption the scan exists to test.
**Recommendation:** in the F-3 scenarios, drive `set_payments` and `delete_proforma` into `Transport` (a 500 reply) and let (xxi) see them.

### F-9 (`check_account`'s headline defence) `scope: null` under a scoped call when protocol v7 is off (is never provoked
**Severity:** low **Confidence:** high) the harness asserts the three flags are on (`service.rs:983-989`) and never starts a server without them.
**Location:** `tests/service.rs:950-989`, design §4.
**Evidence:** the glossary says the probe "is the only defence against that case". Its positive behaviour (`scope` echoed) is tested; the negative (`scope: null` while the caller used `/restate/scope/acme/…`) is asserted nowhere. This is a property of the Restate server + SDK, so it can drift with either.
**Recommendation:** one ignored e2e variant (or a second container in the same test) with `RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7=false`: register the multi-account deployment, call `check_account` under `/restate/scope/acme/…`, assert `scope == null` and a 400 `unknown_account` from the create path, documenting that the deploy-time canary really fires.

### F-10, Timing-dependent assertions in the e2e (`watch` window of 4 s) are a flakiness risk on slow runners
**Severity:** low **Confidence:** medium, passed 2/2 here; the risk is structural, not observed.
**Location:** `tests/service.rs:1276-1316` (`watch`: 40 × 100 ms), used by (vi-c), (xi), (xi-b), (xi-c), (xi-d), (xiv).
**Evidence:** `watch` records `retry_count`/`last_failure` only while in flight; assertions like `retries.failing_commands == ["lookup-invoice"]` need the retry to happen inside the 4 s window. With a 1 s policy, three executions plus Restate scheduling fit, but a loaded CI runner or a slow docker pull could push the retry past the window and the assertion would fail with a confusing message. Fixed host ports (18080/19070) also preclude two concurrent runs on one machine.
**Recommendation:** make `watch` stop when the invocation completes (poll `status`) rather than after a fixed count, and pick ephemeral host ports (`-p 0:8080` then `docker port`).

### F-11: The `RunRetryPolicy` mapping is pinned via `Debug` strings
**Severity:** info **Confidence:** high.
**Location:** `config.rs:875-935`.
**Evidence:** `assert_eq!(format!("{:?}", policy), "RunRetryPolicy { initial_delay: 120s, … }")`; the only way to read the SDK's private fields, and it is honest about that. It will break on an SDK `Debug` change with no behavioural regression; the e2e (xi)/(xi-b)/(xiv) delay assertions are the behavioural pin.
**Recommendation:** keep; when the SDK exposes getters, switch. No action needed now.

### F-12, The e2e is one 36-scenario test function; no isolation between scenarios and no per-scenario result reporting
**Severity:** info **Confidence:** high.
**Location:** `tests/service.rs:1457-1505`.
**Evidence:** a failure in scenario k aborts k+1…n; the only progress signal is `eprintln!` (needs `--nocapture`). Scenarios share the mock server via `h.reset()` and the same Restate server, which is by design (the flag day needs continuity), but the phase-1 scenarios could be independent.
**Recommendation:** keep the single server but wrap each scenario in a labelled `Result` collector so one run reports every failing scenario; or split phase 1 into per-scenario `#[tokio::test]`s sharing a `OnceCell` server.

---

## (d) Mutation spot checks (reasoned, no files modified)

| # | Mutation | Caught by | Verdict |
|---|---|---|---|
| 1 | **Create-step send condition**: gateway.rs:1077 `Seen::Live(found) if Some(found.number()) != request.reversed` → `==` | `create_re_executed_after_a_lost_reply_finds_the_document_and_sends_nothing` (gateway.rs:807): with `reversed=None` the inverted guard is false → `LiveAgain`, test expects `Found` | **caught** |
| 1b | gateway.rs:1092 `Seen::Reversed(found) if Some(found.number()) != request.reversed` → `==` | `create_past_the_reversed_document_the_lookup_saw_sends_the_create` (:847) expects `Issued` but gets `Reversed`; `create_never_sends_past_a_reversal_the_lookup_did_not_see` (:867) would send (create mock `expect(0)` fails) | **caught** |
| 2 | **teszt pin**, gateway.rs:388 `self.info.test == expect_test` → `!=` (or dropped) | `lookup_of_an_invalid_document_under_our_id_is_a_collision` "test" case (:437), `opened_gateway_validates_the_accounts_mode_against_teszt` (:2365), unit `a_found_document_must_belong_to_the_resolved_account` (tests.rs:839), and every positive lookup (a matching doc would become a collision) | **caught** |
| 3 | **supplier_id pin**, gateway.rs:390 `expected == seen` → `!=`; or `_ => true` → `_ => false` | "supplier" case of :437; `document_ext_reads_the_checks_off_a_queried_document` (gateway.rs:1755-1758); unit tests.rs:878-892 (unpinned case); e2e (xviii-b) unpinned storno | **caught** |
| 4 | **Untrimmed key**: support.rs:204 `key.trim() != key` → `==`, or the check removed | `the_order_key_must_arrive_trimmed` (tests.rs:788): `order_key("ORD-1")` would fail / `" ORD-1"` would pass; e2e (x-c) | **caught** |
| 5 | **71/152**, gateway.rs:1006 `Found → Reconciled` returned as `Found`; `existing_number` guard `newest.is_live() && tipus == kind` with either conjunct dropped; `Err(NotFound) => None` turned into `Err(Unconfirmed)`; corrective exemption removed | `duplicate_order_number_with_our_live_document_under_the_id_is_reconciled` (:1131); `…has_no_existing_number_when_another_kind_is_newest` (:1208, the "reversed" and "proforma"/"storno" cases split the two conjuncts); `…with_nothing_under_the_order_is_settled_without_a_number` (:1241); `…on_a_corrective_is_rejected_without_an_order_query` (:1264) | **caught** |
| 6 (extra) | Remove `check_pins(...)?` from `verify_for_storno` (storno.rs:138) or from `correct()` (create.rs:320) | - | **not caught** (F-4, F-3) |
| 7 (extra) | Remove `if !found.carries_order(order)` from `verify_for_storno` (storno.rs:132) | - | **not caught** (F-4) |
| 8 (extra) | `ProformaLink::None` branch (create.rs:524) returns `Ok(None)` instead of `conflict{proforma_live}` | - | **not caught** |
| 9 (extra) | `carries_order` drops `.map(str::trim)` (gateway.rs:384) | no test supplies a padded `<rendelesszam>` in a response | **not caught** (low impact: the server trims on create, so a padded value in a response is unlikely) |
| 10 (extra) | `is_foreign` drops `found.is_live()` (gateway.rs:1679) | `lookup_hint_ignores_our_documents_non_invoices_and_its_own_failure` "reversed" case (:585) | **caught** |

Five of the five requested conditionals are pinned by unit/gateway tests that run in CI; the handler-level guards (6–8) are pinned by nothing.

---

## (e) Upstream fixtures not referenced by any test

All 60 files under `fixtures/upstream/` are unreferenced (`grep -rn upstream --include='*.rs' crates` → 0 matches; the basename-level hits for `xmlszamlavalasz.xml`, `szamla_query.xml`, `taxpayer*.xml`, `xmlnyugta*.xml`, `xmlszamladbkdelvalasz*.xml`, `querying_pdf_*.xml` all resolve to the *synthetic* copies under `tests/synthetic/`, not the upstream ones):

- `adatkapcsolat/`: `banktranz.xsd`, `banktranzvalasz.xsd`, `nyugtavalasz.xsd`, `szamla.xsd`, `szamla_example.xml`, `szamlabe.xsd`, `szamlabevalasz.xsd`, `szamlavalasz.xsd`, `szamlavalasz_example.xml`, `xmlnyugtaarchiv.xsd` (10)
- `agent/requests/`: all 12 (`xmlnyugtacreate`, `xmlnyugtaget`, `xmlnyugtasend`, `xmlnyugtast`, `xmlszamla`, `xmlszamladbkdel`, `xmlszamladbkdel_ordernumber`, `xmlszamlakifiz`, `xmlszamlapdf`, `xmlszamlast`, `xmlszamlaxml`, `xmltaxpayer`)
- `agent/responses/`: all 19, notably the three text-format error examples (`credit_entry_text_error.txt`, `generating_invoice_text_error.txt`, `querying_pdf_text_error.txt`, `reversing_invoice_text_error.txt`) (consistent with the crate never requesting `valaszVerzio 1`), and `xmlszamlavalasz_pdf.xml`
- `agent/xsd/`: all 16

Related: the `synthetic/` fixtures **are** all used (14/14). The `golden/` request files (10) are all used by `writes_canonical_*_xml` tests.

---

## (f) What is done well

- **Outcome-as-data at the gateway seam is exhaustively matrixed.** 63 wiremock tests with `expect(n)`/`up_to_n_times(n)` on every selector, wire-body assertions on `szamlaKulsoAzon`, `rendelesSzam`, `dijbekeroSzamlaszam`, `helyesbitettSzamlaszam`, `megjegyzes`, `eszamla`, the absence of `keltDatum`, and `szamlaagentkulcs` per account. Loops over the four credential codes on every operation. `Err(Unanswered)`/`Err(Unconfirmed)` vs `Ok(Api)` is pinned for every read and write.
- **The e2e harness is unusually honest.** It reads Restate's own `sys_journal`/`sys_invocation` (hex-decoded `raw`, journal v2 two-row runs), asserts step names and counts per invocation, distinguishes run retries from handler retries via `last_failure_related_command_name` observed in flight, and has a **positive control** for the leak scan. `create_lands_but_reply_lost` flips on the create *request* rather than a hand-counted query, and the three stub helpers are themselves tested against wiremock alone (service.rs:4222-4366).
- **Negative-space assertions everywhere:** `expect(0)` on creates that must not happen, `requests_seen == before`, `runs.is_empty()` for pre-prologue refusals, `!fault.message.contains(key)` with the key demonstrably on the wire first (tests.rs:590-595, 645-650).
- **Compile-time guards that make a class of bug impossible:** `assert_not_impl_any!(Credentials: Serialize)`, the `Journaled` marker on the run helpers, exhaustive `match` in every fixture pin so a new variant fails to compile until pinned.
- **Contract closure** (`deny_unknown_fields`, `additionalProperties: false`) is tested at three levels: serde unit, discovery manifest, and e2e 400 with serde's message.
- **Configuration is tested end-to-end to the wire:** an all-digit agent key from the environment reaches wiremock byte-exact (main.rs:339); every README/design TOML example loads (config.rs:274).
- **Test naming reads as specification**; each e2e scenario's doc comment states the design section it pins.
- The verified-behaviour table is well mirrored: 152 message shape, per-kind 71/152, corrective exemption, `sztornozott` semantics, zero-gross storno, D/SL echo, 14/221/352, 335, body-only 7/463, header precedence, 56 with/without number, `<additiv>`, six-entry refusal.

---

## (g) Questions I could not resolve

1. ~~Does the discovery test pin exclusive vs shared?~~ Resolved: yes, tests.rs:142-143 asserts `handler.ty == None` (exclusive) for every non-`get` handler, and `Shared` for `get`. F-2 is therefore about the *behavioural* serialisation and the second caller's lookup, not about the handler kind.
2. Is there an intention (issue #15 is referenced) and a target date for automating the go-live checklist as `live.rs` tests? Today the three live tests cover none of the eight checklist rows.
3. Whether the maintainers consider a runtime read of `fixtures/upstream/**` in tests compatible with the "workspace-only reference corpus" licence stance (F-7 assumes yes since `exclude` keeps it out of the package).
4. Whether the dagger module's `test` defaults (`allFeatures=false`, `locked=false`) are deliberate, `README.md:50` documents the full-flag command, so the CI/README divergence looks accidental.
5. How `Szamlazz.Agent.set_payments`'s `max_attempts(1)` on the run interacts with the handler's `initial_interval = 2m` under a real crash: the design describes it, the discovery test pins the attribute, but no test provokes a crash between send and journal write (hard to do without fault injection in the SDK).
