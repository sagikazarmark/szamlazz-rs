# Spec-conformance audit, `restate-szamlazz` / `restate-szamlazz-endpoint`

Repository: `/home/laborant/szamlazz-rs2` at `0e4238c` (merge of #56).
Specs read in full: `CONTEXT.md`, `docs/design/restate-szamlazz.md`, ADRs 0001–0006, `docs/szamlazz-hu-behaviour.md`,
`crates/restate-szamlazz/README.md`, `crates/restate-szamlazz-endpoint/README.md`, root `README.md`.
Implementation read: `crates/restate-szamlazz/src/**`, `crates/restate-szamlazz-endpoint/src/**`, the unit tests in both crates,
`tests/gateway.rs`, `tests/service.rs` (e2e), `tests/journal/**`, `tests/check_config.rs`, `tests/stop.rs`.
No files in the repository were modified; no `cargo` command was run.

## (a) Summary

| Classification | Count |
|---|---|
| CONFORMS | 113 |
| DEVIATES | 6 |
| UNIMPLEMENTED | 1 |
| UNDOCUMENTED | 7 |
| CONTRADICTORY | 1 |
| **Total claims checked** | **128** |

Headline: the implementation tracks the design closely; every fault code, HTTP status, external-id format, config default,
validation rule, prologue step, ownership pin and journaled type I could derive from the specs is in the code and, with a
handful of exceptions, pinned by a test. The deviations are almost all on the *documentation* side: the user-facing
contract claims a 422 pass-through on `Szamlazz.Agent.storno` and a 404 on `set_payments` that the code never produces;
the endpoint README still counts four `Szamlazz.Agent` handlers; several docs still say "issue and resolve policies" from
before the read policy existed. The one genuinely unimplemented item is the acknowledged go-live checklist automation
(issue #15). The undocumented behaviours are small but caller-visible (status codes for wire-contract violations, the
create step's handling of an `Api` answer on its leading query, the Agent storno's early return on an already-reversed
document).

Test-coverage gaps worth naming (not spec deviations, but "claim with no test"): `correct_invoice`, `create_final`,
`delete_proforma` (beyond the malformed-body case) and the `set_payments` *handler* have no end-to-end scenario; their
branch logic is covered only through gateway tests and pure-function unit tests.

## (b) Checklist

Legend, Status: C = CONFORMS, D = DEVIATES, U = UNIMPLEMENTED, N = UNDOCUMENTED, X = CONTRADICTORY.
"Test" column: the test that pins the behaviour, or "-" when none was found.

### Identity: order key, external ids, sentinel

| # | Claim | Source | Status | Evidence (file:line) | Test | Note |
|---|---|---|---|---|---|---|
| 1 | Order key = order number trimmed, case preserved; 1–64 bytes, no control chars, no whitespace runs → `invalid_input` | design §3:46-48; ADR 0002:27-28 | C | `identity.rs:40-59`; `support.rs:203-211` | `identity.rs:237-274`, `tests.rs:788-830` | |
| 2 | An untrimmed key is refused as `invalid_input` **before the prologue**, naming the rule | CONTEXT *Order*; design §3:48-49; README lib:190-193 | C | `support.rs:204-207`; `handlers.rs:62-64` (body → key → prologue) | `tests.rs:788-830`; e2e `service.rs:2451` | |
| 3 | The `OrderKey` *type* stays lenient (trims) for `FromStr`/`TryFrom`/serde | design §3:52-54; README lib:191 | C | `identity.rs:40-41,68-114` | `identity.rs:277-282`, `tests.rs:800-804` | |
| 4 | External ids: `{ns}:{order}:{kind}`, `…:corrective:{id}`, `…:storno:{number}`, `{ns}:by-number:{number}:storno` | CONTEXT *External id*; design §3:60-63; ADR 0005:49-55 | C | `identity.rs:157-180` | `identity.rs:285-326` | |
| 5 | Sentinel `{namespace}:check-account` is two segments; every issued id has ≥3 | CONTEXT *External id*, *check_account*; design §4:164 | C | `identity.rs:188-190` | `identity.rs:332-348` | |
| 6 | An order key may contain `:`; nothing forbids it, so two different orders can compose the same external id (`ns:A:corrective:invoice` from order `A:corrective`/kind invoice and from order `A`/corrective `invoice`) | (none) | N | `identity.rs:40-59` accepts `:`; `identity.rs:157-166` | - | Validation (`rendelesszam`) prevents adoption, but such orders would meet spurious `conflict{external_id_collision}`. Undocumented edge; see finding 14. |
| 7 | Buyer name normalised once (trim + NFC) and serialised identically | design §3:81-82; ADR 0005:25-27 | C | `identity.rs:224-226`; `build.rs:152` | `identity.rs:351-360`; `build.rs:281` | |
| 8 | `CorrectionId` matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`; embedded in the corrective external id | ADR 0005:129; README lib:188-189 | C | `contract.rs:47-78,150-155`; `create.rs:289` | `contract.rs:356-401,460-464` | |
| 9 | Order number query hint is exact/untrimmed server-side, so `carries_order` compares the trimmed `rendelesszam` | behaviour doc:25-27 | C | `gateway.rs:383-385` | `gateway.rs:1759-1767` | |

### Configuration (`WorkerConfig`, policies, namespace)

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 10 | Namespace: 1–16 bytes of `[a-z0-9-]`, `:` excluded | CONTEXT *Namespace*; design §9:471 | C | `config.rs:227,235-249` | `config.rs:840-870` | |
| 11 | Issue policy defaults `5`, `2m → 10m`, factor 2, `1h` | design §9:473-478; README lib:198-200 | C | `config.rs:511-521` | `config.rs:811-835,893-911` | |
| 12 | Read policy defaults `3`, `5s → 30s`, factor 2, `2m` | CONTEXT *Read policy*; design §9:480-485 | C | `config.rs:576-586` | `config.rs:917-972` | |
| 13 | Resolve policy defaults `1s → 10s`, `1m`, **no attempt cap** | CONTEXT *Resolve policy*; design §9:487-491 | C | `config.rs:632-653` | `config.rs:875-887` | |
| 14 | All three built on `RunRetryPolicy::new()` field for field (not `default()`) | ADR 0004:74-78; ADR 0006:294-300 | C | `config.rs:530-536,594-600,648-653` | `config.rs:893-935` | |
| 15 | `validate()`: `max_attempts ≥ 1` (issue, read), `initial_delay ≤ max_delay` and `factor ≥ 1` (all three) | design §9:506-508 | C | `config.rs:98-139` | `config.rs:977-1091` | |
| 16 | Durations: `"90s"`, `"2m"`, `"1h"` or bare non-negative integer seconds | design §9:505; README endpoint:98 | C | `config.rs:664-683,720-767` | `config.rs:1094-1186` | |
| 17 | `WorkerConfig` / `StaticConfig` are `Deserialize`-only | design §9:463; README lib:44 | C | `config.rs:61`; `static_resolver.rs:71` | - | `IssueConfig`/`ReadConfig`/`ResolveConfig`/`Defaults`/`SellerConfig` derive `Serialize` too (used by the loader's drift test); the two top-level types do not. |
| 18 | `Secret` `Debug` is redacted; accepts an integer (all-digit keys) | README lib:206; static_resolver docs | C | `config.rs:331-366` | `config.rs:1189-1199` | |
| 19 | `config` module / services hold "namespace, issue and resolve policies" | `lib.rs:16`; `service.rs:13-14`; design §2:36; ADR 0001:23 | D | `config.rs:62-74` holds `read` too | - | Stale wording from before #37; four places omit the read policy. Finding 9. |

### Account model, resolver, credential store

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 20 | `Account` = id, mode (default live), supplier_id?, endpoint, defaults, seller, credential_ref; never the key; additive-only | CONTEXT *Account*; ADR 0006:72-80 | C | `account.rs:60-85` (`#[serde(default)]` on every optional) | `account.rs:500-542`; `journal/resolution/account.json` | |
| 20a | The `Account` is journaled once per invocation; the UI shows it without the key | design §4:131-133 | C | `support.rs:405-417`; e2e asserts one `account` run and no key | e2e `service.rs:1561-1569, 4148` | |
| 21 | Static resolver: `credential_ref = id` | CONTEXT *Credential ref*; design §9:495 | C | `static_resolver.rs:303` | `static_resolver.rs:541-545` | |
| 22 | Single shape: `resolve(None)` = account, any scope unknown | CONTEXT *Account resolver*; ADR 0006:344-345 | C | `static_resolver.rs:439-443` | `static_resolver.rs:577-588` | |
| 23 | Multi shape: unscoped → `Unscoped`; scope → account or unknown; no default account | design §9:526-528; ADR 0006:306-308 | C | `static_resolver.rs:444-450` | `static_resolver.rs:670-714` | |
| 24 | Both shapes present → load error; neither → error | design §9:527; README endpoint:150 | C | `static_resolver.rs:419-426`; loader `schema.rs:157-168` | `static_resolver.rs:717-736`; `config.rs:661` | |
| 25 | Multi shape load-time checks: supplier id required; unique supplier ids; unique `(endpoint, agent_key)`; unique ids | CONTEXT *Account resolver*; design §9:529-532; ADR 0006:127-129 | C | `static_resolver.rs:363-408` | `static_resolver.rs:739-832` | The `(endpoint, key)` pair compares the endpoint *text*; `http://x/` vs `http://x` evade it (finding 15). |
| 26 | Scope keys `[a-z0-9_]`, 1–36 bytes | CONTEXT *Scope*; design §9:532-534 | C | `static_resolver.rs:67,245-259` | `static_resolver.rs:837-877` | |
| 27 | Per-account: non-blank id and key, http(s) endpoint with host | design §9:509 | C | `static_resolver.rs:282-302`; `account.rs:180-190` | `static_resolver.rs:601-631`; `account.rs:545-576` | |
| 28 | Traits object-safe, boxed futures, `Debug` supertrait; `Accounts` bundles `Arc<dyn …>` | CONTEXT *Account resolver*; ADR 0006:318-319 | C | `account.rs:265,291-297,312-318,382-414` | `account.rs:631-663` | |
| 29 | `Unavailable` display never echoes the source's message | CONTEXT *Account resolver*, *Credential store*; ADR 0006:81-83 | C | `account.rs:339-340,362-363`; `prologue.rs:52-54` | `account.rs:669-691`; `prologue.rs:230-240` | |
| 30 | `Credentials`/`AgentKey` implement neither `Serialize` nor `Deserialize` (compile-time guard) | ADR 0006:89-92; README lib:99-100 | C | `account.rs:435-436` (`assert_not_impl_any!`) | same | |
| 31 | `mode` defaults to `live` and is always validated | ADR 0006:258-268 | C | `config.rs:199-205`; `gateway.rs:387-393` | `static_resolver.rs:562-574`; `gateway.rs:2365` | |

### Contract: request / response shapes

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 32 | Every request type and nested object is `deny_unknown_fields` (15 types listed) | design §7:384-388; README lib:150-153 | C | `request.rs:26,49,89,103,125,136,163,222,244`; `document.rs:26,84,108,132,222,286` | `request.rs:408-473`; `contract.rs:507-590` (schema `additionalProperties:false`) | |
| 33 | Response types stay open | design §7:392; README endpoint:238 | C | no `deny_unknown_fields` in `response.rs` | `contract.rs:592-608` | |
| 34 | `CreateRequest { document, options{reissue, proforma: auto\|none\|{number}} }` | design §4:92-95; README lib:146-149 | C | `request.rs:27-84` | `request.rs:289-327` | |
| 35 | `CorrectRequest { invoice_number, correction_id, document }` | design §4:96 | C | `request.rs:90-98` | `request.rs:330-344` | |
| 36 | `StornoRequest { invoice_number, comment? }`; `DeleteProformaRequest { force }`; `QueryRequest { selector }` with 3 selectors; `SetPaymentsRequest { invoice_number, entries, additive }` | design §4:97-98,165,167 | C | `request.rs:104-158,223-255` | `request.rs:346-400` | |
| 37 | `QueryTaxpayerRequest.tax_number` accepts exactly the 8-digit stem or `NNNNNNNN-N-NN`; anything else `invalid_input` naming input + forms, **before the prologue** | CONTEXT *Taxpayer lookup*; design §4:166 | C | `request.rs:188-217`; `agent.rs:36-40`; `handlers.rs:335-337` | `request.rs:539-583`; `agent.rs:395-433`; e2e `service.rs:3876` | |
| 38 | `CreateResponse` fields exactly as §7 | design §7:368-374 | C | `response.rs:104-147` | `response.rs:945-998` | |
| 39 | `Outcome` and `ConflictReason` token sets | design §7:371-373; README lib:157-162 | C | `response.rs:24-87` | `response.rs:1001-1025` | |
| 40 | `StornoResponse { outcome ∈ reversed\|rejected\|conflict\|managed_by_order, conflict_reason?, invoice_number, storno_number?, order_key?, code?, message? }` | design §7:375-376 | C | `response.rs:267-307` | `response.rs:1028-1049` | |
| 41 | `DeleteProformaResponse { deleted, reason? }`; `absent` reason for nothing-to-delete | design §7:377; §6:341 | C | `response.rs:364-400` | `response.rs:1053-1057` | |
| 42 | `OrderStatus{proforma?,invoice?,prepayment?,final?}`, `DocumentStatus{number,state,gross,net,payments,referenced_proforma?,e_invoice?}`, `DocumentState` flattened as `state: live\|reversed{storno_number?}\|consumed{by}` | design §6:347-349; README lib:184-187 | C | `response.rs:608-715` (`rename="final"`, `flatten`, `tag="state"`) | `response.rs` + `contract.rs:496-498` | |
| 43 | `CheckAccountResponse { scope, account{id,mode,supplier_id}, namespace, credentials: {state: ok}\|{state: rejected, code, message} }` | CONTEXT *check_account*; design §4:164 | C | `response.rs:727-807` | e2e `service.rs:2941-2949` | |
| 44 | `QueryTaxpayerResponse { valid, name?, tax_number?, vat_code?, addresses[] }` is a crate-owned additive-only projection, never the agent's `TaxpayerInfo` | CONTEXT *Taxpayer lookup*; ADR 0005:183-185 | C | `response.rs:825-853`; `gateway.rs:474-476` | `journal/taxpayer-outcome/*.json` | |
| 45 | `TerminalCode` has exactly six codes | design §7:379-380; ADR 0006:352-354 | C | `contract.rs:295-328` | `contract.rs:437-456` (`UnknownAccount` not in the token loop, info) | |
| 46 | `QueryResponse` is a projection of `InvoiceDocument` with `outstanding = gross − Σ payments`, `supplier_id`, `test` | design §4:165 | C | `response.rs:479-593,918-920` | `response.rs:1066-1094` | |

### Fault codes, statuses, bodies

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 47 | Statuses: `outcome_unknown` 500, `unavailable` 503, `account_mismatch` 409, `invalid_input` 400, `credentials_rejected` 503, `unknown_account` 400 | design §7:379-380; README lib:318-325 | C | `support.rs:146-157` | `tests.rs:425-466` | |
| 48 | Fault body `{code, message, order?, kind?, external_id?}`; `external_id` is the only namespace marker; no account named | README lib:307; design §7:363-365 | C | `support.rs:50-60,160-166` | `tests.rs:425-466`; `account_mismatch_never_leaks…` | |
| 49 | Malformed body → `invalid_input` with `malformed request body: …` + serde's message; decoded by the handler via `Body<T>` whose SDK `Deserialize` never fails; discovery metadata = `Json<T>`'s | CONTEXT *Malformed body*; design §7:393-400 | C | `body.rs:29-88`; `handlers.rs:62,90,…` | `tests.rs:305-358,365-422`; e2e `service.rs:2369` | |
| 50 | `Szamlazz.Agent.query`: 7 → 404 `not_found`; other code → 422 pass-through with szamlazz.hu's code | CONTEXT *Outcome*; design §4:165 | C | `agent.rs:131-143`; `support.rs:176-179` | e2e `service.rs:3812-3874` (404) | 422 path not e2e-tested. |
| 51 | `Szamlazz.Agent.query_taxpayer`: any `funcCode ≠ OK` → 422 pass-through, never 404; `valid:false` is 200 | CONTEXT *Taxpayer lookup*; design §4:166 | C | `agent.rs:167-177`; `gateway.rs:1236-1249` | `gateway.rs:2472,2523`; e2e `service.rs:3876` | |
| 52 | `Szamlazz.Agent.set_payments`: rejection → 422; lost reply → `outcome_unknown` with additive-conditional message | design §4:167; ADR 0004:156-162 | C | `agent.rs:67-76,205-223` | `agent.rs:341-374` | Handler not e2e-tested. |
| 53 | "`Szamlazz.Agent.query`, `set_payments` and `storno` also answer a by-number miss as 404 `not_found` and pass a szamlazz.hu error through as 422" | README lib:309-310; README endpoint (implicit); design §7:360-361; ADR 0006:354-356; CONTEXT *Outcome* | X | `set_payments` sends without a query → can never 404 (`agent.rs:187-224`); `storno`: verify `Api` → **503 `unavailable`** (`agent.rs:259-261`), storno-send rejection → **200 `outcome: rejected`** (`support.rs:282-286`), never 422. Design §4 storno row (:168) lists neither 404 nor 422, yet code *does* 404 (`agent.rs:252-258`). | e2e asserts storno 404? no; query 404 yes | Finding 1. |
| 54 | `credentials_rejected` on 3/135/136/164 on any step; logged `warn` with namespace and code, never the key | design §7:420-427; README lib:294-298 | C | `gateway.rs:1548-1556`; `support.rs:112-130` | `tests.rs:504-606`; `gateway.rs:1321-1437,1541,1928,2152,2172,2499` | |
| 55 | `account_mismatch` message names document, observed and expected pins; never the key | design §7:434; CONTEXT *Account* | C | `support.rs:232-245` | `tests.rs:614-659,839-893` | |
| 56 | `unavailable` on read-policy exhaustion names the step and the last failure; `about` the document when known | CONTEXT *Outcome*; design §7:413-416 | C | `support.rs:185-191,495-510,555-558` | `tests.rs:740-778`; `agent.rs:439-458`; e2e `service.rs:2686` | |
| 57 | `unavailable` also covers resolve exhaustion and credential-store gone/unavailable; `gone` terminal at once | design §4:134-141; CONTEXT *Outcome* | C | `prologue.rs:86-92,113-149` | `prologue.rs:243-279`; e2e `service.rs:3113` | CONTEXT *Credential store* says "both end … after a short in-process retry": `gone` has no retry (info, finding 19). |
| 58 | `unknown_account` (400) for unscoped-on-multi / scoped-on-single, raised by the `account` step before anything is issued | CONTEXT *Outcome*; design §7:410-413 | C | `prologue.rs:69-82` | `prologue.rs:193-227`; e2e `service.rs:2978-2989, 3202` | |
| 59 | "an invalid Virtual Object key (§3)" is a **third-source** `invalid_input` "raised after the prologue" | design §7:405-408 | D | `handlers.rs:63-64`: `order_key(ctx.key())?` (full `OrderKey::parse` validation) runs **before** `self.prologue` | `tests.rs:819-829` | Doc stale; code is stricter/better. Finding 10. |
| 60 | `options.proforma` on a non-invoice kind → `invalid_input` (after the prologue) | design §5:277-278; §7:406 | C | `create.rs:355-359` (in `prepare`, called after prologue at `handlers.rs:64-65`) | `create.rs:793-824`; e2e `service.rs:2248` | |
| 61 | Empty `buyer.name` → `invalid_input` | design §7:407 | C | `create.rs:377-379` | - | No direct test found for the empty-name path. |
| 62 | A sixth credit entry is refused "by the crate before sending" | behaviour doc:110; design §4:167 (`entries[≤5]`) | N | `gateway.rs:1484-1492` → `Rejected{code:"request"}` → `agent.rs:212-216` → **422 with code `request`** | `gateway.rs:2259` | Status/code undocumented; arguably `invalid_input` 400 by CONTEXT's definition. Finding 6. |
| 63 | A wire-contract failure on the create (`ClientError::Request`) | (none) | N | `gateway.rs:1666-1669` → `Failure::Rejected{code:"request"}` → `outcome: rejected{code:"request"}` (200) | - | Undocumented outcome code. Finding 6. |

### Prologue

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 64 | Order: decode body → parse key → pin `namespace` → resolve `account` (resolve policy) → fetch outside journal → open gateway | CONTEXT *Prologue*; design §4:117-143 | C | `handlers.rs:62-64`; `support.rs:388-426` | e2e: every journal opens `namespace`,`account` (`service.rs:1561,2953,3675`) | |
| 65 | Resolution journaled as data (`Account`/`Unscoped`/`Unknown`); only resolver unavailability is retryable | ADR 0006:72-83 | C | `prologue.rs:36-67`; `support.rs:407-417` | `prologue.rs:181-240`; e2e `service.rs:3044`; `journal/resolution/*` | |
| 66 | Fetch: 3 attempts, 200 ms apart, terminal `unavailable` after; on every execution incl. replays | design §4:134-136; ADR 0006:84-87 | C | `prologue.rs:97-134` | e2e `service.rs:3113, 4058` | |
| 67 | Fresh Számla Agent client per execution (session boundary) | ADR 0006:310-313; CONTEXT *Credential store* | C | `gateway.rs:766-772`; `prologue.rs:152-160` | `gateway.rs:2314` | |
| 68 | Pinned namespace used for the rest of the handler (redeploy cannot move a running invocation) | design §4:124-125 | C | `support.rs:394-401` | - | Not directly tested (would need a namespace change mid-invocation). |

### Ownership validation

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 69 | Under our external id: ours ⇔ `rendelesszam == order ∧ tipus of kind ∧ teszt == mode ∧ (supplier unset ∨ equal)`; else `conflict{external_id_collision}` | design §3:68-70; ADR 0005:62-64 | C | `gateway.rs:353-406,1124-1126`; `support.rs:315-338` | `gateway.rs:1748-1785`; `tests.rs:662-733`; `gateway.rs:437` | |
| 70 | Found by number: `Szamlazz.Order` verifies (storno, corrective base, `options.proforma:{number}`) check order (`not_managed`) then pins (`account_mismatch`) | CONTEXT *Order*; design §3:72-78; §5:196-200; §6:299-300 | C | `storno.rs:132-138`; `create.rs:316-320,563-569` | e2e `service.rs:2057` (proforma); storno/correct verifies not e2e-tested | |
| 71 | `Szamlazz.Agent.query` and `storno` check pins on every found document; storno checks **before** `managed_by_order` | design §4:165,168; ADR 0006:357-361 | C | `agent.rs:127-128,272-284` | e2e `service.rs:3652-3805, 3812` | |
| 72 | `set_payments` and `query_taxpayer` are the two exemptions | CONTEXT *Account*; design §4:166-167 | C | `agent.rs:187-224` (no query), `156-178` (no document) |, (negative) | Doc comments at `contract.rs:310` and `service.rs:97` still name only `set_payments` (finding 11). |
| 73 | Order number is not a pin of the account check | design §11:649-650 | C | `support.rs:232-234` (`account_matches` only) | `tests.rs:851-852` | |

### Create protocol

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 74 | Step 1 exclusivity: invoice↔prepayment `prepaid_chain`; proforma vs invoice **and** prepayment `order_invoiced`; final none | design §5:184-188; CONTEXT *Foreign document* | C | `create.rs:194-204,416-443` | `create.rs:770-787`; e2e `service.rs:2303` | |
| 75 | Collision under a secondary id → `conflict{external_id_collision}` (never "absent") | design §5:190-191 | C | `create.rs:435-437,466-468,515-520` | e2e `service.rs:2206` | |
| 76 | Step 2 proforma link: `auto` passes live D; `none` + live D → `proforma_live`; `{number}` verify → 7 `proforma_missing`, not this order → `not_managed`, pins → `account_mismatch`, `tipus≠D` → `invalid_input`; prepayment skips step 2 | design §5:192-202, 277-281 | C | `create.rs:246-252,493-584` | e2e `service.rs:1969,2057,2248` | |
| 77 | `create_final`: prepayment must be live ES (7 → `prepayment_missing`, reversed → `prepayment_reversed`, invalid → collision); passes `elolegSzamlaszam` | design §5:281-284 | C | `create.rs:233-239,446-478`; `build.rs:110-117` | `build.rs:403-423` | Handler branches not e2e-tested. |
| 78 | Step 3 lookup: one read step `lookup-{kind}` under the read policy; hint on every kind but correctives; `Live/Reversed{storno?}/Collision/Foreign/Absent/CredentialsRejected/Api` | CONTEXT *Lookup step*; design §5:204-225 | C | `create.rs:651-673`; `gateway.rs:812-883` | `gateway.rs:389-724`; e2e `service.rs:2603,2686` | |
| 79 | Foreign = live `SZ\|ES\|VS` under the order ∉ `our_numbers` ∧ ≠ the document under our id; also when ours is reversed | CONTEXT *Foreign document*; design §5:212-214 | C | `gateway.rs:1677-1682,847-858` | `gateway.rs:542,585` | |
| 80 | Lookup handler mapping: Live → `already_issued` / `conflict{live}` with reissue; Reversed → `reversed{storno?}` / proceed; Collision; Foreign; `Api` → `unavailable`; CredentialsRejected → fault | design §5:220-223; ADR 0003 | C | `create.rs:602-636` | e2e `service.rs:1509,1686,1757,1786` | |
| 81 | Step 4 create: one run `create-{kind}` under the issue policy, query-first inside the closure; sends only when the id holds nothing or exactly the lookup's reversed document; `Found`/`Reversed`/`LiveAgain`/`Collision` settle without sending | CONTEXT *Create step*; design §5:226-241; ADR 0003 #36 | C | `create.rs:681-719`; `gateway.rs:1068-1107` | `gateway.rs:773-925`; e2e `service.rs:1850,1914` | |
| 82 | Lost reply / open code (1, 55, 56 w/o number, `szlahu_down`) → re-query once → `Unconfirmed` if nothing | CONTEXT *Unconfirmed*; design §5:245-248 | C | `gateway.rs:933-961,973-982,1650-1665` | `gateway.rs:1037,1083` | |
| 82a | 56 **with** a number is a success carrying `notification_delivery_failed = true` (→ `issued` + warning); 56 **without** a number is the open code | design §5:242-245,261; behaviour doc:129 | C | `szamlazz-agent/src/ops/invoice.rs:1051-1088`; `create.rs:116-118` | `response.rs:945-961` (warning token) | |
| 83 | 71/152 → re-query: ours live → `Reconciled`; invalid → `Collision`; reversed-not-lookup's → `Reversed`; else name via order query → `DuplicateOrderNumber{existing_number?}`; contradiction (7) settled + `warn`; correctives → `Rejected` without order query | design §5:249-256; ADR 0006:241-256 | C | `gateway.rs:999-1051` | `gateway.rs:1131-1264`; e2e `service.rs:1620` | |
| 84 | Step 5 mapping: `Issued` (+`notification_delivery_failed`), `Found`→`issued`, `Reversed`→`reversed` (no storno no.), `LiveAgain`→`conflict{live}`, `Reconciled`, `Collision`, `DuplicateOrderNumber` (+code,message), `Rejected`, `CredentialsRejected`→fault | design §5:261-269 | C | `create.rs:95-156` | `create.rs:833-903` | |
| 85 | Create-step `Err` (exhaustion/cancel) → `outcome_unknown{order,kind,external_id}` | design §5:257-260 | C | `create.rs:710-718` | e2e `service.rs:2513` | |
| 86 | `correct_invoice`: verify base (7 → `invalid_input`, not this order → `not_managed`, pins → `account_mismatch`, reversed → `base_reversed`); ext id `…:corrective:{id}`; no hint; `our_numbers` empty | design §5:285-289 | C | `create.rs:275-344` | `gateway.rs:700,724,1264` (gateway only) | Handler not e2e-tested. |
| 87 | `build_create`: defaults + overrides, line totals, `download_pdf=false`, `external_id`, `order_number`, seller block; `issue_date` passed through only when supplied | design §5:180-183; behaviour doc:39 | C | `build.rs:72-169` | `build.rs:230-471` | |
| 88 | On the create step's **leading query**, a szamlazz.hu `Api` answer (neither 7 nor a credential code) is … | design §5:233-239 says "transport → `Err(Transport)`", silent on `Api` | N | `gateway.rs:1105`: `Api`, `Unavailable`, `Transport` all → `Unconfirmed::Transport(error.to_string())` → issue-policy re-executions → `outcome_unknown` on exhaustion; the lookup step maps the same answer to immediate `unavailable` (`support.rs:325`) | - | Inconsistent handling of one answer + misleading message ("transport failure: szamlazz.hu error 57 …"). Finding 5. |
| 89 | `Unconfirmed::Open` display | (none) | N | `gateway.rs:274` prints `szlahu_down` whenever `code` is `None`, including the "create succeeded without a document number" case at `:936-939` | - | Misleading text in `last_failure`/`outcome_unknown`. Finding 16. |

### Storno, delete, get

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 90 | `storno_invoice` step 1 verify order: 7 → `invalid_input{not_found}`; ≠ order → `not_managed`; pins → `account_mismatch`; `sztornozott` → `reversed{storno?}` via best-effort hint; `tipus ∉ {SZ,ES,VS,HS}` → `rejected{not_stornoable}` | design §6:299-304 | C | `storno.rs:104-152`; `support.rs:650-683` |, (gateway-level only) | Handler not e2e-tested in phase 1; `purged_order_is_stornoed_and_reissued` covers the happy path. |
| 91 | Step 2 `lookup-storno-{number}` under read policy; matching SS → `reversed`; stray holder → proceed; `Api` → `unavailable` | design §6:305-311 | C | `support.rs:589-604`; `gateway.rs:1266-1296,1430-1453`; `storno.rs:62-77` | `gateway.rs:1697-1754` | |
| 92 | Step 3 `storno-{number}` under the issue policy, query-first; no `keltDatum`; `reverses` (≠ number ∧ gross ≤ 0); echo → `NotStornoable`; codes → `Rejected`; open → re-query → `Unconfirmed` | design §6:312-329; behaviour doc:63,71,74 | C | `support.rs:617-643`; `gateway.rs:1332-1419`; `szamlazz-agent/src/ops/invoice.rs:721-723` | `gateway.rs:1800-2051` | |
| 93 | `e_invoice` for the storno = verified `eszamla` else account default | design §6:334 | C | `storno.rs:49-51`; `agent.rs:288-290` | - | |
| 94 | `Szamlazz.Agent.storno`: verify `verify-{number}` → pins → `managed_by_order{key}` → lookup/storno steps under `{ns}:by-number:{number}:storno` | design §4:168; §6:336-339 | C | `agent.rs:231-328` | e2e `service.rs:3652-3805` | |
| 95 | `Szamlazz.Agent.storno` on an already-reversed *unmanaged* document | (none) | N | `agent.rs:285-287`: returns `reversed` **without** `storno_number` and without the lookup step (which might have named the SS) | - | Undocumented short-circuit; the design's flow would have yielded the storno number via the lookup or the server's echo. Finding 7. |
| 96 | `delete_proforma`: lookup under read policy; 7 → `{deleted:true, reason:absent}`; collision → `{deleted:false, external_id_collision}`; payments ∧ !force → `proforma_paid`; delete `max_attempts(1)`; 335 → deleted; other → `{deleted:false, reason:code}`; transport → `outcome_unknown` | design §6:341-345 | C | `storno.rs:157-214`; `support.rs:434-450`; `gateway.rs:677-692` | `gateway.rs:2088-2170` (gateway only) | Handler not e2e-tested. |
| 97 | `get`: four reads `get-{kind}` under the read policy; collision leaves slot absent; consumed proforma derived from `hivdijbekszam`; no storno number; `Api` → `unavailable`; credential codes → fault | design §6:347-355; CONTEXT *Consumed proforma* | C | `storno.rs:224-287`; `support.rs:315-338` | e2e `service.rs:1969,2174,2766,2206` | |

### `Szamlazz.Agent` reads and probe

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 98 | `check_account`: prologue, one step `probe` under the read policy querying the sentinel; `Accepted` on 7 / a document / any non-credential code; `Rejected{code,message}` on 3/135/136/164 as data; `scope` = `ctx.scope()`; echoes configured account and pinned namespace | CONTEXT *check_account*; design §4:164 | C | `agent.rs:87-105`; `gateway.rs:1189-1215` | `gateway.rs:1576-1663`; e2e `service.rs:2932-2993, 3451` | |
| 99 | `query_taxpayer`: step `taxpayer-{prefix}` (prefix, not the number as sent) | CONTEXT *Taxpayer lookup*; design §4:166 | C | `agent.rs:45-47,162` | `agent.rs:381-387`; e2e `service.rs:3913,3930` | |
| 100 | Not cached by the worker | CONTEXT *Taxpayer lookup*; ADR 0005 | C | no state anywhere (`rg ctx.set` empty) | - | |

### Handler registration and attributes

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 101 | Services registered as `Szamlazz.Order` (VO, 8 handlers) and `Szamlazz.Agent` (service, **5** handlers) | ADR 0001:79; design §4 | C | `handlers.rs:41,264` | `tests.rs:90-113,175-195` | |
| 102 | "Eight handlers on `Szamlazz.Order`, **four** on `Szamlazz.Agent`"; `--check-config` sample shows `handlers=4` | README endpoint:122,240 | D | `handlers.rs:264-395` registers five (`check_account, query, query_taxpayer, set_payments, storno`) | `tests.rs:175-195` asserts five | Stale since #49. Finding 2. |
| 103 | `Szamlazz.Order` issuing/storno/delete handlers: `2m`, ×2, `10m`, 5, kill; `inactivity 4m`, `abort 3m`, `journal 3d`, `idempotency 30d` | design §4:101-106; ADR 0004:20-24 | C | `handlers.rs:44-56,72-84,…,218-230` | `tests.rs:145-170` | |
| 104 | `get`: default back-off, `max_attempts 3`, kill, `journal 1d`, shared, no input | design §4:108 | C | `handlers.rs:248-256` | `tests.rs:124-139` | |
| 105 | `Szamlazz.Agent.storno`: 2 attempts, kill, `4m/3m`, no explicit `initial_interval` | design §4:168; ADR 0004 #41; README endpoint:230 | C | `handlers.rs:379-385` | `tests.rs:250-255` | |
| 106 | `set_payments`: `initial_interval 2m`, 2 attempts, kill, `2m/2m` | design §4:167; ADR 0004:156-162 | C | `handlers.rs:352-362` | `tests.rs:257-268` | |
| 107 | `query`, `query_taxpayer`, `check_account`: "`max_attempts = 3, kill`; `journal_retention = 1d`" | design §4:164-166; README lib:340-341 | N | `handlers.rs:273-282,291-300,319-328` also set `initial_interval = 10s, factor 2.0, max_interval = 1m` | `tests.rs:205-235` | Documented only in the endpoint README's Stopping section (:230), which omits `query_taxpayer`. Finding 12. |
| 108 | No Restate service calls another; no `Order` handler calls its own key; no VO state; no `ctx.sleep` | ADR 0001; ADR 0005; design §2:37-38 | C | `rg service_client\|object_client\|ctx.set\|ctx.sleep` → none in `src/` | - | |

### Journal compatibility

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 109 | Journaled types: `Namespace`, `Resolution`, `QueryOutcome`, `LookupOutcome`, `CreateOutcome`, `StornoLookupOutcome`, `StornoOutcome`, `DeleteOutcome`, `SetPaymentsOutcome`, `ProbeOutcome`, `TaxpayerOutcome`; `Journaled` bounds the run helpers | CONTEXT *Journaled type*; ADR 0005:135-192 | C | `support.rs:32-44,442,472,504` | `journal.rs:130-145,701,743` | |
| 110 | One fixture per variant under `tests/journal/<type>/`; generator never writes unless `UPDATE_JOURNAL_FIXTURES=1`; archives differing shapes; compatibility replays every fixture | ADR 0005:155-167; README lib:394-402 | C | `journal.rs:700-1049`; directory listing (11 dirs, every variant present) | `journal.rs:904-1049` | |
| 111 | `Transport` variants of the read outcomes are gone; `Api{code,message}` is data | CONTEXT *Unanswered*; ADR 0004 #37 | C | `gateway.rs:119-162,412-434,447-459,562-590` (no `Transport`); `DeleteOutcome`/`SetPaymentsOutcome` keep `Transport` (writes) | `journal/*` | |

### Endpoint binary

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 112 | `--config`/`CONFIG_FILE`, `--bind`/`BIND_ADDR` (default `0.0.0.0`), `--port`/`PORT` (default 9080), `--check-config`; `RUST_LOG` default `info` | design §10:568-572; README endpoint:213-216 | C | `main.rs:33-53,65-66,71-74` | `check_config.rs`, `stop.rs:80` | |
| 113 | Env overrides `RESTATE_SZAMLAZZ_*` with `__` nesting, read as strings (`extract_lossy`) | design §9:521-523; README endpoint:98 | C | `config.rs:25-27,55,146-148`; `sources.rs` | `config.rs:407-465`; `main.rs:339-381` | |
| 114 | Strict loader: every unknown key at every level named with path, source and expected keys; pre-release layout refused with moved-key hints; both shapes named with sources; tree matched against library `Serialize` output | design §9:511-520; README endpoint:102-113 | C | `schema.rs:48-170,229-253` | `config.rs:661-870`; `schema.rs:354-361`; `check_config.rs:90-117` | |
| 115 | `--check-config` loads, validates, builds the endpoint, logs the summary, exits 0 without listening; non-zero with the error otherwise | design §9:524; §10:571-572 | C | `main.rs:68-74` | `check_config.rs:33-148` | |
| 116 | Start-up log: namespace, scoped flag, per account scope/id/mode/endpoint/supplier, never the key; bound address; `stop_on` | design §10:573-575 | C | `main.rs:88-92,151-166` | `check_config.rs:38-62`; `stop.rs:64-75` | |
| 117 | `SIGTERM`/`SIGINT` stop cleanly via `serve_with_cancel`, exit 0 | design §10:575-578; README endpoint:228 | C | `main.rs:79,94-96,110-123` | `stop.rs:37-60` | |
| 118 | Image: non-root uid 65532, `STOPSIGNAL SIGTERM` | design §10:573; README endpoint:26 | C | `Dockerfile:37-53` | - | |
| 119 | `compose.yaml` sets the three experimental flags; e2e asserts them on `/version` | design §11:668-669; ADR 0006:136-137 | C | `compose.yaml:15-17`; `tests/service.rs:408-410,975-989` | same | |
| 120 | Every TOML example in the READMEs and design §9 loads and builds | design §11:639-640 | C | `config.rs:274-313` (`every_documented_example_loads`) | same | The fixture's `[read]` comment omits `query_taxpayer` (info). |

### Go-live / verified-behaviour reliance

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 121 | Go-live checklist "to be automated as ignored tests (issue #15)" | design §11:745 | U | `szamlazz-agent/tests/live.rs:61-99` has three lifecycle tests, none of the eight probes (A1, A4, A4c, A5, B1, B4, B6, C4) | - | Finding 3. |
| 122 | Toggle "Rendelésszám ismétlődés tiltása" ON is a documented, undetectable precondition | ADR 0002:113-115; README endpoint:30 | C (documented reliance) | - |, | Correctly not enforced; correctly documented in both READMEs. |
| 123 | Credential codes assumed "before any write"; header form assumed; unverified live | behaviour doc:171-177 | C (documented reliance) | `gateway.rs:1542-1546` records the assumption | - | |

### Documentation consistency (claims about the code made by docs/comments)

| # | Claim | Source | Status | Evidence | Test | Note |
|---|---|---|---|---|---|---|
| 124 | "The lookup, storno and delete runs use `RunRetryPolicy::max_attempts(1)`"; "`max_attempts(1)` on every read and one-shot write" | ADR 0001:55-57 (Consequences); ADR 0006:297-298 | D | lookup under the read policy `support.rs:495-510`; storno under the issue policy `support.rs:617-643`; only `namespace`, `delete-proforma-*`, `set-payments-*` use `max_attempts(1)` (`support.rs:434-450`) | - | Stale after #30/#37. Finding 8. |
| 125 | `TerminalCode::AccountMismatch` rustdoc and `Agent` struct docs: "`set_payments` finds none and is exempt" (only) | `contract.rs:305-311`; `service.rs:94-97` | D | `agent.rs:156-178`; `query_taxpayer` is the second exemption; CONTEXT, design §4, READMEs name both | - | Finding 11. |
| 126 | Library README's gateway function lists (`lookup, create, …, probe`) and read-fn list | README lib:216-219 | D | `gateway.rs:1229` `query_taxpayer` missing from both lists; CONTEXT *Gateway* has it | - | Finding 13. |

## (c) Findings

### 1. `Szamlazz.Agent.storno` never answers 422 and `set_payments` never answers 404, the caller-facing docs say otherwise
- **Severity:** medium · **Confidence:** high, traced every return path of `storno_request` and `set_payments_request`.
- **Location:** `crates/restate-szamlazz/src/service/agent.rs:187-224, 231-328`; `support.rs:264-289`. Docs: `crates/restate-szamlazz/README.md:309-310`, `docs/design/restate-szamlazz.md:360-361`, `docs/adr/0006-…:354-356`, `CONTEXT.md` *Outcome* entry.
- **Evidence:** In `storno_request`, a szamlazz.hu `Api` answer on the verify is `Fault::inconclusive_answer` → **503 `unavailable`** (`agent.rs:259-261`); a rejection from the storno send is `StornoResponse{outcome: rejected, code, message}` → **200** (`support.rs:282-286`); a code-7 verify is 404 (`agent.rs:252-258`). No 422 exists on that handler. `set_payments_request` sends without a query, so an unknown invoice number is whatever code szamlazz.hu returns → **422** via `SetPaymentsOutcome::Rejected` (`agent.rs:212-216`); no 404 exists. Meanwhile design §4's storno row (:168) documents *neither* the 404 nor a 422, while the code does return 404.
- **Assessment:** stale/over-generalised docs (the code's behaviour is the more sensible one: a storno rejection is a domain outcome, as §6 step 4 says). The "query / set_payments / storno keep their 404 and 422" sentence was copied across four documents from an earlier shape.
- **Recommendation:** Fix docs. State per handler: `query` → 404 on 7, 422 on another code; `query_taxpayer` → 422 on any `funcCode ≠ OK`; `set_payments` → 422 on rejection (no 404); `storno` → 404 on 7 (verify), `outcome: rejected` (200) on a send rejection, `unavailable` on an inconclusive verify. Update README lib :309-310, README endpoint faults preamble, design §4 storno row and §7 :360-361, ADR 0006 :354-356 (mark as amended), CONTEXT *Outcome*. Optionally add an e2e assertion that `Szamlazz.Agent.storno` on a 7 is 404 and on a send rejection is 200 `rejected`.

### 2. Endpoint README counts four `Szamlazz.Agent` handlers; there are five
- **Severity:** low · **Confidence:** high.
- **Location:** `crates/restate-szamlazz-endpoint/README.md:122` (`handlers=4` in the `--check-config` sample) and `:240` ("four on `Szamlazz.Agent`"); the table immediately below lists five rows.
- **Evidence:** `handlers.rs:264-395` registers `check_account`, `query`, `query_taxpayer`, `set_payments`, `storno`; `tests.rs:175-195` asserts five.
- **Assessment:** stale doc (missed in #49).
- **Recommendation:** Fix doc: `handlers=5`, "five on `Szamlazz.Agent`". Consider having `check_config.rs` assert the `handlers=` count so the sample cannot drift again.

### 3. The go-live checklist is not automated
- **Severity:** medium · **Confidence:** high, `live.rs` inspected.
- **Location:** `docs/design/restate-szamlazz.md:745`; `docs/szamlazz-hu-behaviour.md:195-215`; `crates/szamlazz-agent/tests/live.rs`.
- **Evidence:** Design §11 says the checklist is "to be automated as ignored tests (issue #15)". The only live tests are the agent crate's `taxpayer_query`, `invoice_lifecycle`, `proforma_lifecycle`; none of the eight probes the worker's safety relies on (external-id read-your-writes lag, byte-identical replay, buyer-name fingerprint, replay-ends-at-storno, `sztornozott` on the original, repeat-storno echo, storno external id attaches to the SS, trim/case of the order number) is scripted.
- **Assessment:** acknowledged gap (open issue), but every ADR's "verified" fact rests on one test account on one day; a live account or a szamlazz.hu change could invalidate any of them unnoticed.
- **Recommendation:** Implement #15 as `#[ignore]` tests gated on `SZAMLAZZ_AGENT_KEY` (+ an explicit `SZAMLAZZ_GO_LIVE=1`), one per checklist row, asserting exactly the "Expect" column; print `szallito/id`, `teszt`, `eszamla` and the error-header presence per operation as the checklist asks the operator to record.

### 4. Four handlers have no end-to-end coverage
- **Severity:** medium · **Confidence:** high, `rg` over `tests/service.rs`.
- **Location:** `tests/service.rs` (only `delete_proforma` appears, and only in the malformed-body scenario at :2427).
- **Evidence:** `correct_invoice`, `create_final`, `delete_proforma` and `set_payments` are never invoked through Restate. Their gateway steps are tested (`gateway.rs:700,724,1264,2088,2200`) and `respond_to`/`prepare` are unit-tested, but the handler-level branches, `verify-base-*` → `not_managed`/`base_reversed`/`account_mismatch`; `prepayment-for-final` → `prepayment_missing`/`prepayment_reversed`; `proforma-for-delete` → `proforma_paid`/`absent`/collision; `set-payments-*` → 422 / `outcome_unknown` with the additive message, are asserted nowhere with a journal or a wire count. Design §11 does not claim them, so this is a coverage gap rather than a false claim.
- **Assessment:** test gap.
- **Recommendation:** Add one e2e scenario per handler (they fit the existing `holds`/`expect(n)` harness): corrective on a live base of this order → `issued` with `helyesbitettSzamlaszam` on the wire; corrective on another order's base → `conflict{not_managed}` with `create expect(0)`; final without prepayment → `conflict{prepayment_missing}`; delete of a paid proforma → `{deleted:false, proforma_paid}` with `delete expect(0)`, then `force:true` → deleted; `set_payments` with a 500 reply → `outcome_unknown` whose message says "query the invoice" when `additive`.

### 5. An `Api` answer on the create step's leading query is retried as if it were a transport failure
- **Severity:** low–medium · **Confidence:** high, single `match` arm.
- **Location:** `crates/restate-szamlazz/src/gateway.rs:1105` (`Err(error) => Err(Unconfirmed::Transport(error.to_string()))`).
- **Evidence:** `settled_by_query` folds `QueryError::Api`, `Unavailable` and `Transport` into `Unconfirmed::Transport`. Under the issue policy that is up to five executions over ~39 minutes holding the order key, then `outcome_unknown`. The lookup step (the same query a moment earlier) maps `Api` to an immediate `unavailable` (`support.rs:325`, design §5 step 1/3). The resulting `last_failure` text reads "transport failure: szamlazz.hu error 57: …", which is not a transport failure. Design §5 step 4 documents only "transport → `Err(Transport)`". No gateway test covers `Api` on the leading query (`gateway.rs:965-1003` covers a collision and a 500 only).
- **Assessment:** minor bug / undocumented. Not a safety issue (nothing is sent), but it spends the issue policy on an answer that will not change and misnames it.
- **Recommendation:** Either (a) settle it as data, add `CreateOutcome::Api{code,message}` (additive: new variant + fixture) mapped by the handler to `unavailable` like the lookup step, or (b) at minimum map `QueryError::Unavailable` to `Unconfirmed::Open{code:None}` and `QueryError::Api` to a distinct message, and document the choice in §5 step 4. Add a gateway test for the `Api`-on-leading-query case.

### 6. Wire-contract violations surface with an undocumented code `request` (422 on `set_payments`, `outcome: rejected` on a create)
- **Severity:** low · **Confidence:** high.
- **Location:** `gateway.rs:1484-1492, 1506-1509, 1666-1669`; `agent.rs:212-216`.
- **Evidence:** A sixth credit entry (`CreditEntries::try_from` → `TooMany`) or any `ClientError::Request` becomes `Rejected{code: "request", message}`; `set_payments` returns it as **422 `request`**, a create as `outcome: rejected{code: "request"}` (200). CONTEXT defines `invalid_input` as "the caller's request … the same request never succeeds"; design §4 writes `entries[≤5]` without saying how a violation is answered; no README mentions `request` as a code.
- **Assessment:** undocumented; arguably the wrong class (a caller error masquerading as a szamlazz.hu code).
- **Recommendation:** Prefer validating `entries.len() ≤ 5` in the handler before the prologue and answering `invalid_input` (400) like the other pre-prologue refusals; document any remaining `request`-coded rejection in the READMEs' fault/outcome tables. Add the sixth-entry case to the contract tests.

### 7. `Szamlazz.Agent.storno` short-circuits an already-reversed unmanaged document
- **Severity:** low · **Confidence:** high.
- **Location:** `crates/restate-szamlazz/src/service/agent.rs:285-287`.
- **Evidence:** After the pins pass and no order number is present, `found.info.reversed == Some(true)` returns `StornoResponse::new(Reversed, number)` with `storno_number: None`, skipping the lookup step (which would have found our SS under `{ns}:by-number:{number}:storno`) and the storno step (whose idempotent echo would have named the SS). Design §4 :168 describes: verify → pins → `managed_by_order` → lookup and storno steps; no early return. `Szamlazz.Order.storno_invoice` in the same situation at least attempts the hint (`storno.rs:139-147`).
- **Assessment:** justified simplification (one fewer call; the answer is already known) but undocumented and less informative than the documented flow.
- **Recommendation:** Either document it in design §4/§6 and the README (`reversed` without `storno_number` for an already-reversed unmanaged document) or run the lookup step first so the storno number is reported when we issued the storno.

### 8. ADR "consequences" that were never updated after later amendments
- **Severity:** low · **Confidence:** high.
- **Location:** `docs/adr/0001-…:55-57` ("The lookup, storno and delete runs use `RunRetryPolicy::max_attempts(1)`"); `docs/adr/0006-…:297-298` ("`max_attempts(1)` on every read and one-shot write"); `docs/adr/0004-…:25` ("`Szamlazz.Order.get` with `verify`"); `docs/adr/0003-…:128-129` ("the ledger refuses `storno_invoice` (`rejected{has_corrective}`)") not listed among the superseded items in the ADR's status line.
- **Evidence:** Reads run under the read policy (`support.rs:495-510`, #37); storno under the issue policy (`support.rs:617-643`, #30); only `delete-proforma-*`, `set-payments-*` and `namespace` use `max_attempts(1)` (`support.rs:434-450`). There is no `has_corrective` refusal: a corrected base's storno is the server's 221 → `rejected{221}` (`gateway.rs:1362-1365`, behaviour doc :73).
- **Assessment:** stale ADR text (ADRs are historical, but these are in "Consequences"/"still holds" sections a reader will take as current).
- **Recommendation:** Add one-line "*Amended (#37/#30)*" notes at ADR 0001:55-57 and ADR 0006:297; add `rejected{has_corrective}` to ADR 0003's superseded list; reword ADR 0004:25.

### 9. "issue and resolve policies": the read policy is missing from four descriptions
- **Severity:** low · **Confidence:** high.
- **Location:** `crates/restate-szamlazz/src/lib.rs:16`; `crates/restate-szamlazz/src/service.rs:13-14`; `docs/design/restate-szamlazz.md:36`; `docs/adr/0001-…:23`.
- **Evidence:** `WorkerConfig` has `issue`, `read`, `resolve` (`config.rs:62-74`); CONTEXT *Gateway* and the READMEs say "issue, read and resolve".
- **Assessment:** stale wording from before #37.
- **Recommendation:** Fix doc/comments ("the issue, read and resolve policies").

### 10. Design §7 places "an invalid Virtual Object key" after the prologue; the code refuses it before
- **Severity:** low · **Confidence:** high.
- **Location:** `docs/design/restate-szamlazz.md:405-408` vs `handlers.rs:62-64`, `support.rs:203-211`.
- **Evidence:** `order_key(ctx.key())?`, which runs the full `OrderKey::parse` validation (length, control characters, whitespace runs), not only the trim check, precedes `self.prologue(&ctx)` in every `Szamlazz.Order` handler. §7's second source correctly lists the *untrimmed* key as pre-prologue but its third source lists "an invalid Virtual Object key (§3)" among refusals "raised after the prologue".
- **Assessment:** stale doc; the code's ordering is the better one (nothing journaled for a key that can never be valid).
- **Recommendation:** Move "an invalid Virtual Object key" into §7's second source.

### 11. Two doc comments still name `set_payments` as the *only* pin-check exemption
- **Severity:** low · **Confidence:** high.
- **Location:** `crates/restate-szamlazz/src/contract.rs:310` (`TerminalCode::AccountMismatch` docs); `crates/restate-szamlazz/src/service.rs:94-97` (`Agent` struct docs).
- **Evidence:** `query_taxpayer` finds no document either (`agent.rs:156-178`); CONTEXT, design §4 and both READMEs name both exemptions.
- **Assessment:** stale rustdoc (missed in #49).
- **Recommendation:** Fix both comments.

### 12. The 10 s → 1 m back-off on `query`, `query_taxpayer`, `check_account` is documented only partially
- **Severity:** low · **Confidence:** high.
- **Location:** `handlers.rs:273-282, 291-300, 319-328`; design §4:164-166; README lib:340-341; README endpoint:230.
- **Evidence:** The three read handlers set `invocation_retry_policy(initial_interval = "10s", factor = 2.0, max_interval = "1m", max_attempts = 3, kill)`. Design §4 and the library README say only "`max_attempts = 3`, kill, `journal_retention = 1d`". The endpoint README's Stopping section mentions "10 s on `Szamlazz.Agent.query` and `check_account`" but omits `query_taxpayer`. The discovery test pins the values (`tests.rs:210-221`).
- **Assessment:** undocumented in the design; partially stale in the endpoint README.
- **Recommendation:** State the policy once in design §4 (a row note) and add `query_taxpayer` to the endpoint README's list.

### 13. Library README's gateway function lists omit `query_taxpayer`
- **Severity:** low · **Confidence:** high.
- **Location:** `crates/restate-szamlazz/README.md:216-219`.
- **Evidence:** Both the "one plain async fn per `ctx.run`" list and the read-fn list stop at `probe`; `gateway.rs:1229` has `query_taxpayer`, and CONTEXT *Gateway* lists it.
- **Assessment:** stale README.
- **Recommendation:** Add `query_taxpayer` to both lists.

### 14. Order keys may contain `:`, the external-id separator
- **Severity:** low · **Confidence:** medium, the collision is constructible but requires unusual order numbers.
- **Location:** `identity.rs:40-59` (no `:` rule), `identity.rs:157-166`.
- **Evidence:** The namespace excludes `:` "because it is the separator" (`config.rs:219-221`), but the order key does not. Order `A:corrective` / kind `invoice` and order `A` / corrective id `invoice` both produce `ns:A:corrective:invoice`; order `X:invoice` / kind `proforma` yields `ns:X:invoice:proforma`. Ownership validation (`rendelesszam`) keeps a stranger's document from being adopted, so this is not a safety hole, but two of this deployment's own orders would then report each other's document as `conflict{external_id_collision}` on every create.
- **Assessment:** undocumented edge case.
- **Recommendation:** Either state in design §3 / README that `:` in an order key is allowed and why it is harmless, or refuse `:` in `OrderKey` (a behaviour change; check the Pretix key shape first, `{event-slug}-{order-code}` never contains one).

### 15. The multi-shape `(endpoint, agent_key)` uniqueness check compares URL text
- **Severity:** low · **Confidence:** high.
- **Location:** `static_resolver.rs:396-402`; `account.rs:180-190` ("The text is kept as written").
- **Evidence:** `http://127.0.0.1:1/` and `http://127.0.0.1:1` are two keys of the map; the same agent key on both passes the check although it is one szamlazz.hu account under two scopes (fan-in, safety-contract rule 1). The test at `static_resolver.rs:798-812` only proves the *defaulted* production URL equals the *written* one.
- **Assessment:** minor gap in an enforced invariant.
- **Recommendation:** Normalise (`Uri` → scheme/host/port/path with a trailing-slash rule) before comparing, or document the limitation beside the rule in the endpoint README.

### 16. `Unconfirmed::Open` prints "szlahu_down" for a success without a document number
- **Severity:** low · **Confidence:** high.
- **Location:** `gateway.rs:274` (Display), `gateway.rs:935-940`.
- **Evidence:** `code.as_deref().unwrap_or("szlahu_down")` is right for `ClientError::ServiceUnavailable` but wrong for the "create succeeded without a document number" case, which also has `code: None`. The text ends up in `last_failure` and the `outcome_unknown` message.
- **Assessment:** cosmetic bug in an operator-facing message.
- **Recommendation:** Give the no-number case its own variant or a `Some("no_number")`-style code.

### 17. Undocumented durable-step names
- **Severity:** info · **Confidence:** high.
- **Location:** `create.rs:428,458,508,296,543`; `storno.rs:114,168,193`; `agent.rs:201`; `support.rs:656`.
- **Evidence:** The design names `namespace`, `account`, `lookup-{kind}`, `create-{kind}`, `lookup-storno-{n}`, `storno-{n}`, `verify-{n}`, `verify-proforma-{n}`, `get-{kind}`, `probe`, `query`, `taxpayer-{prefix}`. The code also journals `exclusivity-{kind}`, `prepayment-for-final`, `proforma-link`, `proforma-for-delete`, `verify-base-{n}`, `verify-storno-{n}`, `hint-storno-{n}`, `delete-proforma-{n}`, `set-payments-{n}`. These appear in the Restate UI and in `unavailable` messages ("names the step").
- **Assessment:** undocumented, harmless.
- **Recommendation:** Add a step-name table to the design (or the library README's "issuing is two durable steps" section).

### 18. `hint-storno-{number}` swallows every fault of the hint but rejected credentials
- **Severity:** info · **Confidence:** high.
- **Location:** `support.rs:656-667`.
- **Evidence:** Design §6 / ADR 0004 #37 say the storno-number hint is best effort when "the read policy could not get answered"; the code also swallows a **cancellation** (409) of the hint run and an `Api` answer, returning `reversed` without a number. Consistent with "best effort", but a cancelled invocation that reports `reversed` is a subtle case.
- **Assessment:** justified deviation, documented loosely.
- **Recommendation:** One clause in §6 step 1: "…any failure of the hint, a cancellation included".

### 19. Small glossary / doc imprecisions
- **Severity:** info · **Confidence:** high.
- CONTEXT *Credential store*: "both end as a terminal `unavailable` after a short in-process retry"; `gone` is terminal at once (`prologue.rs:131`; design §4:136).
- ADR 0002:131-132 and behaviour doc :27: "Internal whitespace … rejected"; a single internal space is accepted; only *runs* are rejected (`identity.rs:51-57`).
- `build.rs:2`: "(design §6 step 0)", step 0 is in §5.
- `fixtures/single.toml` `[read]` comment omits `query_taxpayer`; the README's identical block includes it.
- `contract.rs:437-456` `terminal_code_tokens` loops over five codes; `UnknownAccount` is not in the list.
- Design §9:497-498 says "every adopted document" (v1 vocabulary); CONTEXT says "found".
- Endpoint README `invalid_input` row (:285) omits the untrimmed-key and tax-number-format refusals the library README (:320) and CONTEXT list.
- **Recommendation:** Fix each in place.

## (d) What is done well

- **Fault and outcome tables are exact.** Every code, status and body field in §7 / the README tables exists in `support.rs` and `contract.rs` and is pinned by `tests.rs:425-466`; the two sentinel tests prove the agent key reaches neither the warning nor a fault body.
- **The prologue is implemented literally as specified** (body, key, pin, resolve, fetch, open) with the decisions extracted into pure functions (`prologue.rs`) and the durable behaviour asserted end to end (`namespace`/`account` in every journal, resolver retried under the resolve policy, store outage terminal, rotation picked up, journaled account byte-identical).
- **Ownership validation is uniform.** One `InvoiceDocumentExt` (`gateway.rs:353-406`), one `check_pins` (`support.rs:232-245`), one `Lookup::classify` (`support.rs:315-338`); every handler that finds a document routes through them, in the documented order (order number → pins → kind/state), and the e2e suite shows the storno/query/proforma-link cases with `expect(0)` on the write mocks.
- **The create and storno steps follow ADR 0003 #36 to the letter**: `settled_by_query` (`gateway.rs:1068-1107`) encodes "send only when the id holds nothing or exactly the lookup's reversed document" and the four settle-without-sending cases are each tested with a create mock `expect(0)`.
- **Journal compatibility is enforced, not promised**: every journaled type is behind the `Journaled` bound, every variant has a fixture, and the compatibility test replays archived shapes; the `Transport` read variants are indeed gone.
- **Configuration is strict and self-checking**: the key tree is compared against the library types' `Serialize` output, every documented TOML block is loaded in a test, and the pre-release layout is refused with a hint.
- **Terminology hygiene**: outside ADR status/historical lines and the loader's deliberate refusal message, none of the "avoid" terms (`slug`, `steps` as the module, `Contradiction`, `detect_foreign`, tenant, health check, cancellation/void for storno, "the API") appears in current docs or code.
- **The e2e suite is unusually faithful to the design's §11 list**, 36 scenarios on a real Restate 1.7.8 with the three flags asserted, journals read back by step name, wire counts per account key, and a positive control for the leak scan.

## Questions I could not resolve

1. Is the 10 s → 1 m invocation back-off on the three `Szamlazz.Agent` read handlers a deliberate choice (documented only in the endpoint README's Stopping section) or an accident of copy-paste? The design calls `get`'s policy "default" and says nothing for the others.
2. Is the `:` in order keys (finding 14) known and accepted, given the Pretix key shape never produces one?
3. The endpoint README's error example `invalid type: found string "three", expected u32 for key "RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS" in environment variables`; the unit test at `config.rs:449-458` asserts only that the message contains `"three"`, so I could not confirm figment renders the README's text verbatim without running the binary.
4. Whether "one execution of the create closure may take up to 180 s" still holds now that the leading query is preceded, in the same handler execution, by up to four read steps (exclusivity, prepayment-for-final, proforma-link, lookup): the design (§4:110-115) argues each read is its own execution because a failed read yields, but a *slow* (not failed) chain of reads plus the create closure runs in one execution against `inactivity_timeout = 4m`. I could not verify the SDK's inactivity semantics from the repository alone.

Resolved while writing: 56-with-a-number is parsed as a success with `notification_delivery_failed` (`invoice.rs:1051-1088`), matching the design; the e2e helper `assert_account_mismatch` (`service.rs:507-518`) does assert HTTP 409 for `account_mismatch`.
