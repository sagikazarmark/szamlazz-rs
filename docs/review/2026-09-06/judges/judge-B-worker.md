# Judge B — Restate worker (resilience, spec coverage, test coverage)

Adjudication of reviewer reports 03 (`03-restate-resilience.md`), 04 (`04-spec-coverage.md`) and 06
(`06-test-coverage.md`) against `/home/laborant/szamlazz-rs2` @ `0e4238c`. Every critical/high/medium finding was
re-read at the cited lines; every DEVIATES / UNDOCUMENTED / CONTRADICTORY checklist row of report 04 plus nine
CONFORMS rows were re-checked against code; about half of the low/info findings were spot-checked. External sources
consulted: `restate-sdk 0.12.0` and `rust_decimal 1.43.0` in the local cargo registry, and the dagger `rust` module
at the pinned commit `ce82b85` (fetched from GitHub). No repository files were modified; no cargo commands were run.

## (a) Summary

**Verdict counts (44 merged findings):**

| Verdict | Count |
|---|---|
| CONFIRMED | 31 |
| CONFIRMED-WITH-CORRECTION | 5 |
| DOWNGRADED | 5 |
| UPGRADED | 0 |
| REFUTED | 1 |
| UNVERIFIABLE | 2 |

Overall: the three reports are factually reliable. Every code claim I checked at the cited line was true; the errors
are in consequences drawn from them (one refuted interleaving, several over-rated severities) and in a few
"undocumented" claims where the behaviour is in fact documented somewhere else. No reviewer found, and I could not
construct, a duplicate-document interleaving under the *verified* szamlazz.hu behaviour: the create/storno steps are
query-first inside the closure, every answer is journaled data, and the 2 m gap is the only load-bearing timing
assumption.

**The five most important confirmed findings in this domain:**

1. **J36 (06 F-1)** — The whole handler layer (`issue`, `verify_for_storno`, `correct`, `delete`, `status`, the
   prologue's durable behaviour, scope isolation, the leak scan) is tested only by the docker-gated `#[ignore]` e2e,
   and CI (`dagger check` → plain `cargo test` in `rust:1.98-slim-trixie`, no docker; verified from the module source)
   never runs it. `tests/service.rs:1460-1463` also returns `Ok` silently when docker is absent.
2. **J2 (03-2)** — The invariant every ADR relies on for the lost-reply case — `issue.initial_delay` (and the handlers'
   `initial_interval`) ≥ 60 s client timeout + observed stall, "never below ~90 s" — is enforced by nothing in
   `WorkerConfig::validate` (`config.rs:98-139`). Correction: the 1 s policy the reviewer cites lives only inside the
   Rust e2e source (`tests/service.rs:753-781`); every shippable TOML (`fixtures/single.toml`, both READMEs) says 2 m.
3. **J3 (03-3)** — `Szamlazz.Agent.storno` (`handlers.rs:379-385`) has no `initial_interval`, so a crash mid-`storno-{n}`
   closure is re-dispatched at the server default (~500 ms) while the first `xmlszamlast` may still be in flight.
   This contradicts `docs/szamlazz-hu-behaviour.md:128` ("both 2m, never below ~90 s") and the reasoning ADR 0004 #41
   applied to `set_payments`; the endpoint README (:230) documents the 500 ms as current behaviour, so the docs
   disagree with each other.
4. **J7 (03-7 = 04-5 = 04 row 88)** — `settled_by_query` (`gateway.rs:1105`) folds `QueryError::Api` and
   `Unavailable` into `Unconfirmed::Transport`. An answered non-7, non-credential code on the create step's *leading*
   query — data everywhere else in the crate — is retried under the issue policy (5 executions, 2 m → 10 m, ~39 min
   holding the order key) and ends as `outcome_unknown` although nothing was sent; the same answer one step earlier
   (the lookup) is an immediate `unavailable`. Undocumented in design §5 step 4 and CONTEXT *Unconfirmed*.
5. **J23 + J38 (04-4, 06 F-3, 06 F-4)** — `correct_invoice`, `create_final`, `set_payments` are invoked by no test;
   `delete_proforma` only with a malformed body; `storno_invoice`'s four verify `Break` arms are untested. Concretely,
   deleting `check_pins(...)?` from `correct()` (`create.rs:320`) or from `verify_for_storno` (`storno.rs:138`), or the
   `carries_order` guard at `storno.rs:132`, fails no test — the glossary's ownership claims for these handlers are
   pinned by nothing.

Also notable: **J8 (03-8)** — `rust_decimal` `Mul`/`Add` panic on overflow (verified in
`rust_decimal-1.43.0/src/arithmetic_impls.rs:232`), and the SDK runs the handler future inside hyper's connection
task with no `catch_unwind`, so one adversarial `unit_price × quantity` drops the *whole HTTP/2 connection* between
Restate and the worker — every in-flight invocation multiplexed on it is retried. Corrected: the panic happens after the
prologue (`namespace`/`account` are journaled), not before.

**Most consequential refutation:** **J17 (03-17)** — the claimed foreign-document false positive (`create_prepayment`
naming the order's own live `VS` as `conflict{foreign}`) cannot occur under verified behaviour: after an `ES` storno the
newest document under the order is the `SS` (inherits `rendelesszam`, `szamlazz-hu-behaviour.md:30,62`), which
`is_foreign` ignores, and a `VS` cannot be issued after the `ES` is reversed (`prepayment_for_final` →
`prepayment_reversed`). The underlying gap in `exclusive_with` is real but has a different consequence (additional
finding A1). Also materially recharacterised: **J1 (03-1)** — the HS external-id premise is unverifiable from the repo,
and the reviewer's "no server-side refusal" overstates: `HS`-vs-`HS` under the toggle is explicitly *untested*
(`szamlazz-hu-behaviour.md:24,149`), so the risk is two-sided (either the extid query is the only guard, or a legitimate
second corrective is refused with 152). **J12 (03-12)** — `get` failing on an `Api` answer is exactly what design
§6:354-355 specifies; the "must not fail on an answer" comment (`storno.rs:222-223`) is about collisions.

## (b) Verdict table

Severity/confidence as "sev/conf". Reviewer columns show the originating report's rating.

| ID | Source | Title | Reviewer sev/conf | VERDICT | Judge sev/conf | Justification |
|---|---|---|---|---|---|---|
| J1 | 03 #1 | Correctives rely solely on HS external-id queryability, never verified | high/medium | UNVERIFIABLE | medium/low | `lookup_inner` skips the hint for `Corrective` (`gateway.rs:847`); `after_duplicate` → `Rejected` (`:1014-1017`); the behaviour doc verifies extid queryability for SZ/D/SS only and lists HS-vs-HS as untested (:24,149). Settled by a live probe (create HS with extid, query by extid; two HS under one order). Go-live checklist (:203-212) has no HS row. |
| J2 | 03 #2 | `issue.initial_delay ≥ 90 s` invariant not validated | medium/high | CONFIRMED-WITH-CORRECTION | medium/high | `config.rs:98-139` checks only `max_attempts≠0`, `initial≤max`, `factor≥1`; 60 s timeout hard-coded at `szamlazz-agent/src/client.rs:138`. Correction: the 1 s policy is test-internal Rust (`tests/service.rs:753-781`), not a copyable TOML; `fixtures/single.toml:6` says `2m`. |
| J3 | 03 #3 | `Szamlazz.Agent.storno` has no `initial_interval` | medium/high | CONFIRMED | medium/high | `handlers.rs:379-385` vs `set_payments` `:352-357` (`initial_interval = "2m"`) and every Order write handler (`:44-51`). Contradicts `szamlazz-hu-behaviour.md:128`; endpoint README:230 documents the 500 ms, so the two docs disagree. Consequence (two concurrent SS) unverified. |
| J4 | 03 #4 | `Szamlazz.Agent` unkeyed: no worker-side serialization of by-number writes | medium/medium | DOWNGRADED | low/high | `handlers.rs:264` `#[restate_sdk::service]`; fact true. Exposure needs two operators with different `Idempotency-Key`s racing within seconds on one unmanaged number; storno is server-idempotent sequentially; `additive` is documented at-least-once. Design limitation worth a sentence, not a defect. |
| J5 | 03 #5 (+04 #19b) | `OrderKey` admits single internal space, NBSP, `:`, non-NFC; a server normalisation strands the document behind a permanent collision | high/medium | CONFIRMED-WITH-CORRECTION | medium/medium | `identity.rs:51-57` rejects only *runs*; `carries_order` is exact after trim (`gateway.rs:383-385`); collision path `:1124-1126`. Consequence follows *if* the premise holds, which `szamlazz-hu-behaviour.md:169-170` lists as untested. Confirmable now: ADR 0002:131-132 and behaviour doc:27 say internal whitespace is *rejected* — the code does not. Pretix keys never carry whitespace. |
| J6 | 03 #6 | No timeout around credential fetch / resolver call | medium/high | DOWNGRADED | low/high | `prologue.rs:113-134` and `support.rs:407-416` have no deadline; true. Only the in-memory static resolver exists in-tree; a hang is latent for out-of-tree implementations and cheap to guard. |
| J7 | 03 #7 = 04 #5 = 04 row 88 | Failed/answered leading query costs an issue attempt and 2 m; `Api` retried as if unanswered | medium/high (03), low–medium/high (04) | CONFIRMED | medium/high | `gateway.rs:1105` maps every non-credential `QueryError` (incl. `Api`, `Unavailable`) to `Unconfirmed::Transport`; lookup maps `Api` to data (`:838-841`) → `unavailable` (`support.rs:325`). No gateway test for `Api` on the leading query (`tests/gateway.rs:965-1003` covers collision + 500 only). Safe, but breaks the crate's own outcome-as-data rule and misreports as "transport failure". |
| J8 | 03 #8 | Adversarial decimals panic before journaling; key held for the retry budget | medium/medium | CONFIRMED-WITH-CORRECTION | medium/medium-high | `item.rs:139-143` uses `*`/`+` on `Decimal`; `rust_decimal-1.43.0/src/arithmetic_impls.rs:232` panics "Multiplication overflowed". Correction 1: reached via `issue_kind` → `prepare` (`handlers.rs:64-65`), i.e. *after* the prologue journals `namespace`/`account`. Correction 2 (worse): the SDK polls the handler inside hyper's connection task (`restate-sdk-0.12.0/src/endpoint/mod.rs:285-311`, `http_server.rs:125`) with no `catch_unwind`, so the panic drops the HTTP/2 connection and every in-flight invocation on it is retried. |
| J9 | 03 #9 | `Szamlazz.Agent.storno` sends for D/SL/SS and relies on the echo | low/high | CONFIRMED | low/high | `agent.rs:285-296` has no `document_type` gate; `storno.rs:148-150` has one. Ends correctly (`NotStornoable` / 14), but a write is attempted. |
| J10 | 03 #10 | Read handlers keep the 1 m default inactivity timeout with 60 s reads | low/high | CONFIRMED | low/medium | `handlers.rs:248-256, 273-282, 291-300, 319-328` set no `inactivity_timeout`; `get` runs four sequential 60 s-bounded reads. Server suspension semantics not independently verified. |
| J11 | 03 #11 = 04 #14 = 04 row 6 | `:` allowed in `OrderKey`; correction id may equal a kind token → two orders compose one external id | low/high | CONFIRMED | low/high | `identity.rs:40-59` has no `:` rule; `for_kind`/`for_corrective` (`:157-166`) concatenate; validation (`rendelesszam`) prevents adoption, so it is a spurious `external_id_collision`, never a wrong document. |
| J12 | 03 #12 | `get` is four reads, not a snapshot; fails on `Api` "despite the comment" | low/high | DOWNGRADED | info/high | `status()` (`storno.rs:224-263`) propagates `Lookup::classify`'s `inconclusive_answer` — exactly design §6:354-355 ("another code on any query → unavailable"). The comment at `storno.rs:222-223` is about collisions. Non-snapshot is inherent in "live view". |
| J13 | 03 #13 = 04 #18 | `storno_number_of` swallows a cancellation (409) and answers `reversed` | low/high | CONFIRMED | low/high | `support.rs:656-667` returns `Ok(None)` for every `Fault` of `hint()`, incl. `read_exhausted`'s mapping of a 409. Nothing further is written. |
| J14 | 03 #14 | `credentials_rejected` message "this attempt issued nothing" can be false on the post-send re-query | low/high | CONFIRMED | low/high | `support.rs:127` text; `gateway.rs:1102-1104` reached from `settle_or` after a send (`:978`). Narrow window (key rotated between send and re-query); contract ("fault = outcome unknown") still holds. |
| J15 | 03 #15 | `Endpoint::parse` accepts userinfo; journaled with `Account` | low/high | CONFIRMED | low/high | `account.rs:180-190` checks scheme and host only; `InvalidEndpoint` docs (`:247-248`) even anticipate userinfo. Operator-induced only. |
| J16 | 03 #16 + 06 F-6 (step-name/envelope part) | Journal fixtures pin type layouts, not the run-name sequence or SDK envelope | low–info/high (03), medium/high (06) | CONFIRMED-WITH-CORRECTION | low/high | True: `tests/journal/**` pins payloads only; renaming/inserting a `ctx.run` is caught by nothing. Correction to 06's "guard is presently inert": the *generator* test asserts byte-equality of current output with committed fixtures, so any layout change fails CI today; `UPDATE_JOURNAL_FIXTURES=1` archives the old shape, which the compat test then replays. Zero archived shapes exist (`find … -name '*.[0-9].json'` → 0), so the compat test is a round-trip *today*, but the mechanism is not inert. |
| J17 | 03 #17 | Foreign false positive: order's own live `VS` reported as another channel's after an `ES` storno | low/high | REFUTED | — | `is_foreign` (`gateway.rs:1677-1682`) sees only the hint's newest document; after an `ES` storno that is the `SS` (inherits `rendelesszam`, `szamlazz-hu-behaviour.md:30,62`), not the `VS`. A `VS` newer than the `SS` is impossible through the worker (`create.rs:469-471` refuses a reversed prepayment) and a UI-issued one *would* be foreign. The reviewer's "allowed by the server as far as verified" is wrong: storno of `ES`/`VS` is listed as unverified (:159). See A1 for the real consequence of the table gap. |
| J18 | 03 #18 | Run retries do not consume the invocation budget (server-source verification) | info/high | UNVERIFIABLE | info/— | Rests on `restate-server v1.7.8` source not in the repo; consistent with the e2e assertions (`retry_count = 1`, run delays observed) and ADR 0004. Passed through as not independently verified. |
| J19 | 03 #19, #20 | Client failure funnelling; credentials never journaled/logged | info/high | CONFIRMED | info/high | Spot-checked: `classify_failure` (`gateway.rs:1633-1672`), `assert_not_impl_any!` (`account.rs:435-436`), redacted `Debug` path. |
| J20 | 04 #1 (row 53) | Docs claim `Szamlazz.Agent.storno` 422 and `set_payments` 404; code never produces either | medium/high | DOWNGRADED | low/high | `agent.rs:252-261` (7 → 404, `Api` → 503 `unavailable`), send rejection → 200 `rejected` (`support.rs:282-286`); `set_payments` has no query (`agent.rs:187-224`), rejection → 422. Doc-only; code behaviour is the sensible one and caller rule 2 ("any error = outcome unknown") dominates. Fix README lib:309-310, design §7:360-361, ADR 0006:354-356, CONTEXT *Outcome*. |
| J21 | 04 #2 (row 102) | Endpoint README counts four `Szamlazz.Agent` handlers; there are five | low/high | CONFIRMED | low/high | README endpoint:122 (`handlers=4`), :240 ("four on"); `handlers.rs:264-395` registers five. |
| J22 | 04 #3 (row 121) + 06 map row | Go-live checklist (8 probes) not automated | medium/high | CONFIRMED | medium/high | `szamlazz-agent/tests/live.rs` has three lifecycle tests; none of the eight checklist rows (`szamlazz-hu-behaviour.md:203-212`). Every "verified" fact rests on one test account, one day. Acknowledged (issue #15). |
| J23 | 04 #4 = 06 F-3 | `correct_invoice`, `create_final`, `set_payments` never invoked; `delete_proforma` only with a malformed body | medium/high (04), high/high (06) | CONFIRMED | medium/high | `grep` of `tests/service.rs`: 0/0/0/1 occurrences. Gateway pieces are tested (`tests/gateway.rs:700,724,2088,2200`); handler branches (`prepayment_missing/_reversed`, `base_reversed`, `not_managed` on the base, `proforma_paid`, `run_once` + `outcome_unknown`) are not. Removing `check_pins` at `create.rs:320` fails no test. |
| J24 | 04 #6 (rows 62/63) | Wire-contract violations surface as undocumented code `request` | low/high | CONFIRMED | low/high | `gateway.rs:1484-1492, 1506-1509` (`set_payments` → 422 `request`), `:1666-1669` (create → `rejected{request}` 200). Arguably `invalid_input` by CONTEXT's definition. |
| J25 | 04 #7 (row 95) | `Szamlazz.Agent.storno` short-circuits an already-reversed unmanaged document without the storno number | low/high | CONFIRMED | low/high | `agent.rs:285-287` returns `reversed` with `storno_number: None` before the lookup step. Design §4:168 describes no early return. |
| J26 | 04 #8 (row 124) | Stale ADR "Consequences" (`max_attempts(1)` on lookup/storno; `has_corrective`) | low/high | CONFIRMED | low/high | ADR 0001:55-57 and ADR 0006:297-298 read as current but predate #30/#37; `support.rs:434-450` uses `max_attempts(1)` only for `namespace`, `delete-proforma-*`, `set-payments-*`. |
| J27 | 04 #9 (row 19) | "issue and resolve policies" omits the read policy in four places | low/high | CONFIRMED | low/high | `lib.rs:16`, `service.rs:13-14`, design §2:36, ADR 0001:23; `config.rs:62-74` holds `read`. |
| J28 | 04 #10 (row 59) | Design §7 lists "invalid VO key" as post-prologue; code refuses it before | low/high | CONFIRMED | low/high | `handlers.rs:63-64`: `order_key(ctx.key())?` (full `OrderKey::parse`) precedes `self.prologue`. Design §7:405-408 stale. |
| J29 | 04 #11 (row 125) | Two doc comments name `set_payments` as the only pin exemption | low/high | CONFIRMED | low/high | `contract.rs:310`, `service.rs:94-97`; `query_taxpayer` is the second (`agent.rs:156-178`). |
| J30 | 04 #12 (row 107) | 10 s → 1 m back-off on the three read handlers only partially documented | low/high | CONFIRMED | low/high | `handlers.rs:273-282, 291-300, 319-328` set `initial_interval = "10s", factor 2.0, max_interval = "1m"`; design §4:164-166 says only "`max_attempts = 3`, kill"; README endpoint:230 omits `query_taxpayer`. |
| J31 | 04 #13 (row 126) | Library README gateway fn lists omit `query_taxpayer` | low/high | CONFIRMED | low/high | README lib:216-219 vs `gateway.rs:1229`. |
| J32 | 04 #15 (row 25 note) | `(endpoint, agent_key)` uniqueness compares URL text | low/high | CONFIRMED-WITH-CORRECTION | low/high | `static_resolver.rs:396-402` uses `endpoint.to_string()`. Correction: `supplier_id` is *required and unique* in the multi shape (`:376-390`), so fan-in through `http://x/` vs `http://x` also needs a fabricated second supplier id — the gap is in the secondary guard only. |
| J33 | 04 #16 (row 89) | `Unconfirmed::Open` prints `szlahu_down` for "create succeeded without a number" | low/high | CONFIRMED | low/high | `gateway.rs:274` `unwrap_or("szlahu_down")`; `:935-940` constructs `Open{code: None}` for the no-number case. |
| J34 | 04 #17 | Nine durable-step names not in the design | info/high | CONFIRMED | info/high | `exclusivity-*`, `prepayment-for-final`, `proforma-link`, `proforma-for-delete`, `verify-base-*`, `verify-storno-*`, `hint-storno-*`, `delete-proforma-*`, `set-payments-*` all present in `service/*.rs`; none in design §5/§6. |
| J35 | 04 #19 | Glossary/doc imprecisions (7 items) | info/high | CONFIRMED | info/high | Spot-checked 3/7: `gone` terminal at once (`prologue.rs:131`) vs CONTEXT "both … after a short retry"; internal-whitespace wording (see J5); `terminal_code_tokens` omits `UnknownAccount` (not re-checked). Rest passed through. |
| J36 | 06 F-1 | Handler layer verified only by a docker-gated ignored e2e that CI never runs; docker-absent path passes silently | high/high | CONFIRMED | high/high | Dagger `rust` module @ `ce82b85` `test` defaults: `allFeatures=false`, `allTargets=false`, `locked=false`; `dagger.toml` sets only `settings.version`; no docker in the container; `tests/service.rs:1460-1463` `return`s on missing docker. Design §11:603-604 states handler decisions are tested "end to end or as pure functions" — `issue`, `verify_for_storno`, `delete`, `status`, `correct` are not extracted. |
| J37 | 06 F-2 | No test of two concurrent invocations on one VO key | high/high | DOWNGRADED | low/high | Absence confirmed (`tokio::join!` at `:3331, :3999, :4103` are cross-scope or call+mutation). But the property is Restate's per-key lock, statically pinned by the discovery test (`handler.ty == None` for every non-`get` handler, `tests.rs:142-143`), and the "second caller's lookup sees the first's document" path is already covered sequentially (`issued_then_already_issued`). Nice-to-have regression guard. |
| J38 | 06 F-4 | `storno_invoice` verify decisions (`not_managed`, `account_mismatch`, already-reversed, `not_stornoable`) untested at any level | medium/high | CONFIRMED | medium/high | `storno.rs:132-150` four `Break` arms; `tests/service.rs` drives only the `Continue` arm (:1705, :3566); `tests/gateway.rs` tests `Gateway::storno`, not `verify_for_storno`. Mutation 6/7 survive. |
| J39 | 06 F-5 | `szamlazz-adatkapcsolat` archiver tests never run in CI | medium/high | CONFIRMED | medium/high | `tests/archive.rs:3` `#![cfg(feature = "opendal")]`; no workspace member enables `opendal` (grep); CI runs plain `cargo test`. Outside the worker domain but verified. |
| J40 | 06 F-7 | Upstream fixture corpus (60 files) referenced by zero tests | medium/high | CONFIRMED | low/medium | `grep -rn upstream --include='*.rs' crates` → 0; symlinks exist, `exclude = ["tests/upstream"]` in both Cargo.tomls. Agent-crate concern; the worker journals `InvoiceDocument`, so parser fidelity matters indirectly. Not deep-verified beyond the grep. |
| J41 | 06 F-8 | e2e journal scan never sees `set_payments`/`Transport(String)` variants | low/medium | CONFIRMED | low/high | Follows from J23; `SetPaymentsOutcome::Transport(String)` / `DeleteOutcome::Transport(String)` (`gateway.rs:671, 721`) carry client error text. |
| J42 | 06 F-9 | `check_account`'s `scope: null`-under-scoped-call canary never provoked | low/high | CONFIRMED | low/high | Harness asserts the three flags on (`service.rs:983-989`); no server without v7 is started. |
| J43 | 06 F-10 | `watch` 40 × 100 ms window and fixed host ports are a flakiness risk | low/medium | CONFIRMED | low/medium | `tests/service.rs:1276-1316`; `INGRESS_PORT = 18080`, `ADMIN_PORT = 19070` (`:82-83`). Passed 2/2 per the reviewer; structural. |
| J44 | 06 F-11, F-12 | `RunRetryPolicy` pinned via `Debug` strings; single 36-scenario test fn | info/high | CONFIRMED | info/high | `config.rs:875-935`; `tests/service.rs:1457-1505`. |

## (c) Detailed notes

**J1 (03 #1) — UNVERIFIABLE, recharacterised.** The code facts are exact: `correct()` (`create.rs:275-344`) verifies
the base, then runs the same `issue()` with `our_numbers: Vec::new()` and `reissue: false`; `lookup_inner` skips the
order-number hint for `IssuedKind::Corrective` (`gateway.rs:847`); `after_duplicate` turns a 71/152 into `Rejected`
without naming (`:1014-1017`). So for correctives the create step's leading external-id query is indeed the only
worker-side guard. Whether an `HS` created with `szamlaKulsoAzon` is returned by `xmlszamlaxml` for that id is not
recorded anywhere in `szamlazz-hu-behaviour.md` (B7-query-corrective at :62 says only that an HS was queried, not by
what). The repo cannot settle it. Two corrections to the reviewer's framing: (1) "no server-side refusal (correctives
are exempt from 152)" overstates — the verified exemption (C1-6, B7) is *cross-kind*; the doc explicitly says
"`HS`-vs-`HS` not tested" (:24) and lists it under "Still unverified" (:149-151). Under the per-kind rule that governs
every other kind, `HS`-vs-`HS` with different content might well be 152 — which would guard the reviewer's scenario
for non-identical content but would *also* refuse a legitimate second corrective (new `correction_id`, different
amounts) as `rejected{152}`, contradicting the design's "a new `correction_id` issues a new corrective by contract".
(2) The alternative recommendation (take the hint on correctives) would false-positive on any order with two legitimate
correctives. The sound recommendation is the probe: create an `HS` with an external id, query by it at +0/+2/+60 s,
then create a second `HS` with different content under the same order — and add both to the go-live checklist. This
is a verification gap, not a code bug; severity medium because the path is rare and the fix is a probe.

**J2 (03 #2) — CONFIRMED-WITH-CORRECTION.** `WorkerConfig::validate` (`config.rs:98-139`) enforces nothing about the
absolute size of `issue.initial_delay`; the 60 s timeout is `Duration::from_mins(1)` at
`szamlazz-agent/src/client.rs:138` and is not exported. ADR 0002:58-62, ADR 0004:23-24, design §9:475 and
`szamlazz-hu-behaviour.md:128` all state the ≥ 90 s rule; only prose enforces it. Correction: the reviewer's "the e2e
suite ships a configuration with `initial_delay = "1s"`" refers to a `WorkerConfig` built from inline JSON inside
`tests/service.rs:753-781` — not a TOML an operator could copy. `fixtures/single.toml:6`, `fixtures/multi.toml`, both
READMEs and design §9 all show `2m` with the explanatory comment. The residual risk is an operator "tuning" the value
down. A floor in `validate()` (with a test-only override) is cheap; the e2e builds `WorkerConfig` directly and would be
unaffected if the floor lives in the loader path. Medium/high stands.

**J3 (03 #3) — CONFIRMED.** Verified at `handlers.rs:379-385` versus `:352-357` and `:44-51`. ADR 0004 #41 added
`initial_interval = "2m"` to `set_payments` precisely because "a retry that fires while the first send is still in
flight could append twice", and gave `storno` only the `4m/3m` timeouts. The behaviour doc's design consequence at
:128 states the rule generically for "the handlers' `initial_interval` (a crash)". The endpoint README at :230 lists
"the server's ~500 ms default on `get` and `Szamlazz.Agent.storno`" as current behaviour — so the omission is known
but the docs contradict one another and no rationale for exempting storno is given. Storno's sequential idempotency
(B4) does not cover two `xmlszamlast` in flight at once. Medium/high on the discrepancy; the duplicate-`SS` consequence
is unverified (behaviour doc has no concurrent-storno probe).

**J4 (03 #4) — DOWNGRADED to low.** `Szamlazz.Agent` is a plain service (`handlers.rs:264`); two invocations with
different `Idempotency-Key`s for the same number do race verify → lookup → send. But the affected surface is
deliberately the unmanaged-document facade; the race needs two independent callers within the ~2 s the leading
query and send take; for `set_payments` replace-mode it is last-writer-wins (benign) and additive mode is documented
at-least-once. A sentence in design §4 stating the limitation is proportionate; a `Szamlazz.Document` Virtual Object is
over-engineering for the exposure.

**J5 (03 #5) — CONFIRMED-WITH-CORRECTION, downgraded to medium.** Code facts confirmed: `OrderKey::parse`
(`identity.rs:40-59`) rejects empty, > 64 bytes, control chars and whitespace *runs* — a single internal space, a
single U+00A0, `:` and any non-NFC sequence pass; `carries_order` compares `rendelesszam.trim() == key` exactly
(`gateway.rs:383-385`); mismatch → `Seen::Collision` (`:1124-1126`) → `conflict{external_id_collision}` on every
create, `not_managed` on `storno_invoice` (`storno.rs:132`), `managed_by_order` from `Szamlazz.Agent.storno`
(`agent.rs:273-284`) — the document would indeed be unmanageable through the worker. The premise (szamlazz.hu
normalising internal whitespace/NBSP/NFC in `rendelesszam`) is listed as untested (`szamlazz-hu-behaviour.md:169-170`,
"Low: rejected rather than guessed") — and that line, like ADR 0002:131-132, claims internal whitespace *is*
rejected, which the code contradicts. That doc/code mismatch is the confirmable defect. Severity medium rather than
high: the consequence is a stuck order with an existing document (recoverable by `Szamlazz.Agent.query` + manual
storno, or by a code fix that normalises), not a duplicate; the intended caller's keys (`{event-slug}-{order-code}`)
never contain whitespace; the premise is speculative. Recommendation stands: either tighten `OrderKey` to the verified
alphabet or add the case to the go-live checklist.

**J6 (03 #6) — DOWNGRADED to low.** `fetch_credentials` (`prologue.rs:113-134`) awaits `accounts.fetch` with no
deadline; the `account` step awaits `accounts.resolve` inside `ctx.run` (`support.rs:407-416`) likewise. True, and
the described consequence (a hang defeats the resolve policy's `max_duration` and routes into inactivity/abort
timeouts and the kill-on-five path) follows. But the only implementations in the repository are in-memory
(`StaticResolver`), so the hazard exists only for out-of-tree resolvers, whose authors can also wrap their own I/O in a
timeout. A `tokio::time::timeout` at the trait boundary is a reasonable hardening, not a defect.

**J8 (03 #8) — CONFIRMED-WITH-CORRECTION.** `LineItem::calculated_for_currency` (`szamlazz-agent/src/item.rs:139-143`)
computes `unit_price * quantity`, `net * rate / 100`, `net + vat` with operator arithmetic; `rust_decimal 1.43.0`
(`Cargo.lock`) panics on overflow in `Mul`/`Add` (`arithmetic_impls.rs:161, 232`). `Decimal` deserialises 28–29-digit
mantissas, so `unit_price: "79228162514264337593543950335", quantity: "10"` panics. Correction (a): the reviewer says
"step 0, before the prologue"; in fact `handlers.rs:62-65` runs `into_request` → `order_key` → `prologue` →
`issue_kind`, and `prepare`/`validate_document` is the first line of `issue_kind` (`create.rs:219`), so `namespace` and
`account` are journaled before the panic; the operational consequence (retry under the handler's invocation policy with
the key held, 5 attempts, kill) is unchanged. Correction (b), which makes it worse: `restate-sdk 0.12.0` runs the
handler future inside the response body that hyper polls on the connection task (`endpoint/mod.rs:285-311`) and the
connection task is a bare `tokio::spawn` (`http_server.rs:125`) with no `catch_unwind`; the workspace does not set
`panic = "abort"`. The panic therefore tears down the HTTP/2 connection between Restate and the worker, and every
invocation multiplexed on it is retried after its own `initial_interval` (2 m for Order handlers) — a delay and an
attempt burnt for unrelated orders, repeated at each of the poisoned invocation's five attempts. Still no document
risk (the cut create steps are query-first on re-execution). Checked arithmetic surfacing `InputError` →
`invalid_input` is the right fix; a `catch_unwind` at the handler boundary is a defence in depth the SDK does not offer.

**J12 (03 #12) — DOWNGRADED to info.** `status()` (`storno.rs:224-263`) calls `shared::lookup` per kind, whose
`Lookup::classify` (`support.rs:315-338`) returns `Err(Fault::inconclusive_answer)` on `QueryOutcome::Api`, propagated
by `?` → 503. Design §6:354-355 says exactly that: "another code on any query → `TerminalError{unavailable}`". The
"a read must not fail on an answer" clause in the code comment (`storno.rs:222-223`) and in design §6:352-353 is
scoped to a *collision* leaving the slot absent. So this is conformant, documented behaviour, not a contradiction.
That `get` is four independent reads rather than a snapshot is implied by "live view" and by `get` being shared; a
one-line note would not hurt.

**J16 (03 #16 + 06 F-6) — CONFIRMED-WITH-CORRECTION.** Both reviewers are right that `tests/journal/**` pins type
layouts and that neither the ordered run-name sequence per handler nor the SDK's `Json<T>` envelope is pinned;
inserting or renaming a `ctx.run` breaks replay of in-flight invocations and no test would notice. Reviewer 06's
"the guard is presently inert" is too strong: `every_variant_of_every_journaled_type_is_pinned` compares the JSON the
*current* code writes with the committed fixture byte for byte, so a rename/retype fails CI immediately; the developer
then runs `UPDATE_JOURNAL_FIXTURES=1`, which archives the old shape, and the compatibility test replays the archive.
The dependency on a reviewer not hand-editing the fixture is real but is the same dependency every golden-file test
has. Zero archived shapes exist today (`*.<n>.json` → 0). Pinning the run-name list per handler (03's recommendation)
is the cheap, high-value addition.

**J17 (03 #17) — REFUTED.** The code facts are right: `exclusive_with(Prepayment)` checks only `…:invoice`
(`create.rs:194-204`) and `is_foreign` excludes only `our_numbers` and the document seen under our id
(`gateway.rs:1677-1682`). The scenario is not: "an order with `ES` reversed and `VS` live … the lookup hint returns the
live `VS`" requires the `VS` to be the *newest* document carrying the order number. The hint returns the most recently
issued document of any kind (`szamlazz-hu-behaviour.md:30`), and a storno issues an `SS` that inherits
`rendelesszam` (:62) — so after the `ES` storno the newest document is the `SS`, which `is_invoice_family` rejects, and
the lookup proceeds to `Reversed{storno_number}`/`Absent`. For the `VS` to be newer than the `SS`, the `VS` would have to
be created *after* the `ES` reversal, which `prepayment_for_final` refuses (`create.rs:469-471`, `prepayment_reversed`);
a UI-created `VS` in that position would genuinely be another channel's. The reviewer's "allowed by the server as far
as verified" is also wrong — storno of an `ES`/`VS` is under "Still unverified" (:159). The gap in the exclusivity table
is real but its consequence is the *opposite* one (a missed refusal, not a spurious one) — see A1.

**J20 (04 #1) — DOWNGRADED to low.** All four code facts verified (`agent.rs:252-261`, `support.rs:282-286`,
`agent.rs:187-224, 212-216`); the sentence "`Szamlazz.Agent.query`, `set_payments` and `storno` also answer a by-number
miss as 404 … and pass a szamlazz.hu error through as 422" is indeed in README lib:309-310, design §7:360-361, ADR
0006:354-356 and CONTEXT *Outcome*, and is wrong for two of the three handlers. It is documentation only; the code's
choices (a storno send rejection is a domain `rejected`, `set_payments` cannot 404 without a query) are the ones §6
step 4 and §4 describe elsewhere, and caller-contract rule 2 already tells callers to treat any non-200 as "outcome
unknown". Fix the four documents; consider an e2e assertion for `Szamlazz.Agent.storno` 7 → 404.

**J32 (04 #15) — CONFIRMED-WITH-CORRECTION.** `static_resolver.rs:396-402` keys the credential-pair check on
`endpoint.to_string()`, so `http://x/` and `http://x` are distinct. But the same loop requires a `supplier_id` on every
account and refuses duplicates (`:376-390`); the supplier id is the account's only server-side identity, so evading the
pair check also requires lying about the supplier id. The gap is in the secondary guard; low.

**J37 (06 F-2) — DOWNGRADED to low.** No scenario races two calls on one `(scope, key)` — confirmed. What the test
would prove is Restate's per-key lock, a server property, plus that the handlers are exclusive — which the discovery
test already pins statically (`tests.rs:142-143`, `handler.ty == None`), so an accidental `#[shared]` fails CI. The
protocol-level content of the proposed scenario (second caller's lookup finds the first's document) is exercised
sequentially by `issued_then_already_issued` and `storno_then_stale_create_then_reissue`. A useful regression guard
against a future refactor to a plain service, but not a hole in the safety argument.

**J18 (03 #18) — UNVERIFIABLE.** The claim that `next_retry_interval_override.or_else(|| retry_iter.next())` in
`restate-server`'s invoker leaves the invocation budget untouched when the SDK supplies `next_retry_delay` is
plausible and consistent with ADR 0004's arithmetic and the e2e observations (`retry_count = 1` while the run
retries), but the server source is not in the repository and I did not fetch it. Passed through.

## (d) Additional findings (judge)

### A1. `exclusive_with(Invoice)` omits the final invoice: a plain `SZ` can be issued beside a live `VS` once the `ES` is reversed
- **Severity:** low–medium · **Confidence:** medium (the code path is certain; whether the state is reachable is
  unverified).
- **Location:** `crates/restate-szamlazz/src/service/create.rs:194-204` (`exclusive_with`), `:416-443`
  (`exclusivity`), `gateway.rs:1677-1682` (`is_foreign`).
- **Evidence:** `create_invoice` refuses only on a *live* `…:prepayment`. State: `ES` live → `VS` issued
  (`prepayment_for_final` passes) → `ES` reversed (by `storno_invoice`, whose type gate at `storno.rs:148` admits `ES`,
  or in the UI). Now `create_invoice`: exclusivity finds the `ES` reversed → proceeds; `lookup-invoice` → 7; the hint's
  newest document is the `SS` of the `ES` → not invoice-family → `Absent`; the create step sends an `SZ`. szamlazz.hu's
  toggle is per kind (`szamlazz-hu-behaviour.md:23`), so nothing server-side refuses: the order ends with a live `VS`
  *and* a live `SZ` — double billing. Reachability depends on szamlazz.hu allowing the storno of an `ES` that has a
  `VS`, which the behaviour doc lists as unverified (:159; a 221-like refusal is plausible).
- **Recommendation:** Add `(DocumentKind::Final, ConflictReason::PrepaidChain)` to `exclusive_with(Invoice)` (and to
  `Proforma` as `OrderInvoiced`) — symmetric with the existing table, one extra read, and it also puts the `VS` into
  `our_numbers`. Probe "storno of a settled `ES`" on the go-live list.

### A2. The composed external id is unbounded (up to 157 bytes) while only 110 characters were verified accepted and queryable
- **Severity:** low–medium · **Confidence:** medium (premise unverified: szamlazz.hu's `szamlaKulsoAzon` length
  limit and whether it truncates or rejects).
- **Location:** `identity.rs:31` (`OrderKey::MAX_LEN = 64`), `contract.rs:53` (`CorrectionId::MAX_LEN = 64`),
  `identity.rs:157-180` (no length check on the composed id), `config.rs` (namespace ≤ 16).
- **Evidence:** `{ns}:{order}:corrective:{id}` = 16 + 1 + 64 + 12 + 64 = 157 bytes; `{ns}:{order}:storno:{number}` and
  `{ns}:by-number:{number}:storno` add an invoice number of unbounded length. `szamlazz-hu-behaviour.md:51` verified a
  110-character id. If szamlazz.hu silently truncates a longer id on create, the create step's leading query by the
  *full* id answers 7 on every execution, and a lost reply re-sends — for correctives with no toggle guard verified.
  If it rejects, the create is `rejected` with an unfamiliar code; safe but confusing. Realistic Pretix-shaped ids
  (~85 bytes with a UUID `correction_id`) stay under 110.
- **Recommendation:** Either bound `OrderKey`+`CorrectionId` so the longest composed id ≤ 110, or add a
  `ExternalId` length assertion at construction with a documented limit, and probe the actual limit (create with a
  120-, 160-char id; query back) on the go-live list.

### A3. A failed post-send re-query hides the original open cause in `last_failure`
- **Severity:** info · **Confidence:** high.
- **Location:** `gateway.rs:973-982` (`settle_or`), `:1385-1394` (`storno_settle_or`).
- **Evidence:** `self.settled_by_query(request).await?` propagates the *re-query's* `Unconfirmed::Transport(...)`
  and drops the `unconfirmed` argument (e.g. `Open{code: Some("56")}` or `Open{code: None}` for a no-number success).
  The journaled/`last_failure` text then says "transport failure: …" about the query, hiding that a send happened
  with an open code. Purely observability; the retry path is identical.
- **Recommendation:** Compose the message ("open code 56 after send; re-query failed: …") or prefer the send's cause.

## (e) Reviewer quality ratings

**Report 03 (restate-resilience).** 17 substantive findings verified: 12 confirmed (5 with corrections), 4 downgraded,
1 refuted — factual accuracy on code claims ≈ 100 %, accuracy on drawn consequences ≈ 75 %. The interleaving analysis is
deep and mostly right (the create-step send condition, the 2 m gap's role, the crash-vs-lost-reply split), the
positive verifications against SDK/server source are valuable, and the recommendations are concrete and ADR-consistent.
It over-rates severities where the premise is unverified (#1, #5), mis-states one scenario (#17 ignores that the
`SS` is the newest document), and misses two consequences of its own findings (#8's blast radius, #1's two-sided
HS-vs-HS toggle question).

**Report 04 (spec-coverage).** Every one of the 14 DEVIATES/UNDOCUMENTED/CONTRADICTORY rows and all 9 CONFORMS rows I
spot-checked were correct at the cited lines; all 19 findings are factually right (one downgraded in severity, one
corrected for a mitigating guard). Exceptionally thorough and precisely cited for a documentation audit; its depth on
resilience is intentionally limited, and it occasionally rates doc drift (finding #1) higher than its practical effect.
Highly actionable — each finding names the exact files to fix.

**Report 06 (test-coverage).** 12 findings: 10 confirmed (1 with a correction), 1 downgraded, none refuted — including
the CI analysis, which I re-verified against the dagger module source. The coverage map and reasoned mutation table are
the most useful artefacts of the three reports and pin exactly which guards are untested. It overstates F-6 ("inert")
and F-2 (a Restate property already pinned statically), and its proposed e2e scenarios are specific enough to
implement directly.
