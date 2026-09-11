# Live szamlazz.hu test strategy — 2026-09-11

## Recommendation

**Keep the five existing core scenarios. Strengthen their assertions and add one missing PDF read. Keep the seven probes as individually selected investigations, with receipt lifecycle promoted to optional acceptance.** The core is already appropriately small for rare execution. The strongest potential new worker scenario is a corrective lifecycle, conditional on corrections being a main production use case.

The important distinction is external dependency, not the name “e2e”: the broad worker e2e suite uses **real Restate and mocked szamlazz.hu** and belongs in regular CI. The five `live` scenarios use real szamlazz.hu. Reducing the latter should not remove deterministic concurrency, cancellation, recovery, or fault coverage from the former.

This is a source assessment, not a new vendor execution. No vendor requests or tests were run and no credentials were read. The working tree had ongoing changes; source references describe the files inspected, not a frozen release verdict. The detailed [agent assessment](2026-09-11-agent-live-test-assessment.md) supplies its full scenario and offline-coverage inventory.

## Current setup

| Layer | Actual dependencies | Role and current selection |
|---|---|---|
| Ordinary Rust tests, official examples, XSD check | Local fixtures / HTTP mocks | Broad parsing, serialization, request matrix, contract and business-decision coverage. Keep frequent. |
| `cargo e2e` | Actual Restate; mocked szamlazz.hu | Durable execution, locking, scopes, interruption, uncertainty, journals and handler paths. Automatic Dagger check. |
| `cargo live` | Actual szamlazz.hu; actual Restate for worker cases | Three agent scenarios and two worker journeys. Manual, ignored by default. |
| `cargo probes` | Actual szamlazz.hu | Seven investigative cases. Intended to be filtered to the question being investigated. |

Sources: [test guide:21–29,43–65](../testing.md#L21-L65), [nextest profiles](../../.config/nextest.toml#L5-L43), [Dagger e2e/live](../../.dagger/modules/ci/main.dang#L83-L158). The schema matrix covers all eleven Számla Agent wire operations and many option combinations; it is not proof of vendor business behavior ([guide:103–155](../testing.md#L103-L155)).

The execution policy is good:

- Credentials alone never enable live tests. Explicit selection requires credentials and, for worker journeys, a Restate source; missing prerequisites fail instead of passing by skipping.
- Nextest live/probes are serial, fail-fast and have no whole-test retries. The slow timer reports progress without forcibly terminating a possibly executing write. Production worker config is validated; the live harness does not use the shortened mock policies.
- UUID orders and external ids isolate successful runs. The shared helper records known documents, reverses/deletes in dependency order, and defers mutation cleanup when a send is unresolved. Successful worker tests call `Restate::finish()`.
- Manual Dagger execution injects a Secret and takes a fresh run label to invalidate its execution cache. It is not an automatic `@check`.

Sources: [worker startup:31–53](../../crates/restate-szamlazz/tests/live.rs#L31-L53), [shared helper:73–130,188–265](../../crates/szamlazz-agent/tests/live_support/mod.rs#L73-L265), [guide:189–196,276–318](../testing.md#L189-L318).

The dated acceptance record reports two complete Dagger core executions taking about **52–53 seconds**, with all five passing and cleanup complete. That is historical evidence, not a measurement of this working tree ([record:11–40](../testing-218-acceptance.md#L11-L40)). On expected successful paths, the current combined core allocates roughly **13 numbered documents including cleanup stornos**: agent 3, ordinary worker 5, EUR worker 5. Rare execution matters more than trimming a handful of read calls.

## Keep: the five core cases

| Scenario | Why its vendor evidence is worth retaining |
|---|---|
| Agent `invoice_lifecycle` | Paper HUF invoice; persisted rounded totals; create/query PDF; external-id identity; replacement versus additive credit entries; previous-month fulfillment; verified storno and repeat storno identity. These are several serious vendor semantics exercised on one original. |
| Agent `proforma_lifecycle` | Creation, query, deletion, then absence by both number and external id. This proves deletion, which is different from the worker journeys' consumption. |
| Agent `taxpayer_query` | Small independently selectable NAV smoke. Checks actual returned taxpayer information without pinning mutable company text. |
| Worker `ordinary_order_journey` | Proforma consumption, electronic invoice, same-key completion replay, fresh-invocation discovery, storno, refusal of implicit reissue, exact-number reissue, newest external-id holder, stale expected-number conflict. This validates the real service-to-vendor path. |
| Worker `prepayment_final_journey` | Explicit proforma-to-prepayment reference; EUR fractional-price rounding; final invoice's caller-supplied negative deduction; stored totals and references; consumption after conversion. The vendor's lack of automatic netting makes this especially valuable. |

Sources: [agent live:25–169](../../crates/szamlazz-agent/tests/live.rs#L25-L169), [ordinary worker:180–287](../../crates/restate-szamlazz/tests/live.rs#L180-L287), [EUR worker:289–405](../../crates/restate-szamlazz/tests/live.rs#L289-L405).

The two worker journeys are not redundant copies of agent tests. They cover request conversion, real ingress and persisted linked-document behavior. Conversely, the direct agent invoice covers credit-entry operations and paper appearance that the worker live journeys do not exercise.

## Add or strengthen, in priority order

### 1. Standalone invoice PDF retrieval — add to the existing agent journey

`QueryInvoicePdf` is a separate multipart operation. Current live tests request PDFs in create and XML-query responses, but never call standalone PDF retrieval. Add one retrieval by the existing invoice number and assert identity plus a `%PDF-` signature. Upgrade existing `is_some()` PDF assertions to a signature check too. No extra document is needed; one extra read covers a main user-facing operation.

Sources: [current PDF assertions](../../crates/szamlazz-agent/tests/live.rs#L47-L75), [separate operation](../../crates/szamlazz-agent/src/ops/query_pdf.rs#L13-L93), [agent assessment](2026-09-11-agent-live-test-assessment.md#p1--keep-the-core-at-three-tests-improve-its-existing-observations). Do not require byte-identical PDFs or a live selector/template matrix.

### 2. Make worker storno assertions distinguish derivation from defaults

Currently the account default is `e_invoice=true`, the ordinary invoice is electronic, and the storno must match it. A regression using the account default instead of deriving appearance from the verified original would still pass. Arrange **opposite account default and original appearance** within this same journey—for example a paper default and an explicit electronic original—and retain the matching storno assertion. This strengthens the serious edge case without creating another invoice.

Also assert the original's fulfillment date equals the deliberately requested previous-month date before comparing the storno with it. The current equality alone could pass if both dates were incorrectly today's date (or both absent).

Sources: [default:35–37](../../crates/restate-szamlazz/tests/live.rs#L35-L37), [input date:55–76](../../crates/restate-szamlazz/tests/live.rs#L55-L76), [assertions:211–238](../../crates/restate-szamlazz/tests/live.rs#L211-L238). These target observed vendor behavior: wrong storno date and appearance can be silently accepted ([behavior:111–117](../szamlazz-hu-behaviour.md#L111-L117)). Offline wire tests remain the direct check that the worker actually sends the derived fields.

### 3. Improve assertions using responses already fetched

- **Agent:** after storno, assert the original's queried credit entries are empty. The journey already reverses a credited invoice and already fetches the original. This checks the serious observed behavior that storno removes that history, with no extra request ([live:77–120](../../crates/szamlazz-agent/tests/live.rs#L77-L120), [behavior:99](../szamlazz-hu-behaviour.md#L99)).
- **Worker:** `consumed()` fetches the whole `get` response but checks only the proforma. Assert the live invoice/prepayment/final states and numbers relevant to that stage too. Add one `Szamlazz.Agent.query` of an existing invoice if full public-facade acceptance matters: current document verification bypasses that facade through the direct client. Neither needs another document ([helper:142–153](../../crates/restate-szamlazz/tests/live.rs#L142-L153), [read-back:211,328,379](../../crates/restate-szamlazz/tests/live.rs#L211)).
- **EUR final:** inspect the two stored line totals and the negative deduction, in addition to aggregate totals and item count. This better localizes a line-rounding error without another query ([live:350–391](../../crates/restate-szamlazz/tests/live.rs#L350-L391)).
- **Taxpayer:** assert the returned stem matches the requested stem, without pinning company name or address ([live:27–39](../../crates/szamlazz-agent/tests/live.rs#L27-L39)).

Do not turn these into complete field-equality checks. Some vendor fields are intentionally optional, mutable, or normalized; the stored buyer block has historically reflected partner master data ([behavior:130](../szamlazz-hu-behaviour.md#L130)).

### 4. Consolidate populated credit-entry clearing, if it is in the acceptance scope

The populated clearing probe has useful vendor-state evidence but duplicates an invoice create/register/storno lifecycle. Its check can follow the core invoice's replacement/additive sequence: clear, query, require empty entries and restored outstanding amount. Retire the separate populated probe from regular selection after consolidation.

There is a coverage tradeoff: that would make the eventual storno operate on an empty-credit invoice. If retaining credited-storno coverage (recommendation 3) matters, restore and verify one entry before reversal, or leave clearing as a targeted probe. Do not claim the combined test covers credited reversal after removing the credits.

Keep already-empty clearing as a targeted boundary investigation, not a second required original. [Agent assessment and source references](2026-09-11-agent-live-test-assessment.md#p2--consolidate-the-useful-optional-checks).

### 5. Corrective lifecycle — first candidate for a genuinely new worker journey

There is no current live corrective scenario in either crate. The worker's mocked e2e proves flags, base reference, correction external id and fresh-invocation discovery, but cannot establish vendor acceptance or queried relationships ([corrective e2e:18–125](../../crates/restate-szamlazz/tests/e2e/correct_invoice.rs#L18-L125)). Historical vendor observations establish one corrective under its base's order; the checked-in behavior note leaves **two correctives under the same order** unverified ([behavior:141–146,202–204](../szamlazz-hu-behaviour.md#L141-L204)).

If corrections are a principal production flow, add **one independently selectable worker journey**: create an ordinary base; issue a negative correction; query type/order/base reference/totals/external-id identity; repeat the completed logical correction with a fresh ingress key and require `already_issued`. Investigate a second distinct correction id against the original base before relying on that capability. Once established, those two corrections can form one acceptance journey if multiple corrections are a required use case.

Start as a targeted probe if usage is occasional. Do not insert a corrective into the current ordinary storno/reissue journey: a corrected base has historically refused storno with 221. The current shared cleanup supports ordinary/prepayment/final originals, **not corrective originals**, so a corrective probe needs an explicit, verified cleanup or retained-evidence plan before execution ([helper:214–255](../../crates/szamlazz-agent/tests/live_support/mod.rs#L214-L255), [behavior:108,145–146](../szamlazz-hu-behaviour.md#L108-L146)). This is a distinct business flow, not another parameter combination.

## Drop from routine selection; retain as investigations

These are already outside `cargo live`, so most reductions concern **how probes are selected**, not deleting valuable test source.

| Case | Disposition |
|---|---|
| Electronic original / paper storno and inverse | Keep targeted to appearance changes/vendor questions. Matching appearance plus an opposite-default worker fixture is stronger recurring acceptance of the intended behavior. |
| Already-empty clearing | Targeted boundary probe; populated clear is the main semantic check. |
| Populated clearing | Consolidate when included in acceptance; otherwise retain targeted. |
| Receipt automatic MNB rate | Targeted when automatic foreign-currency receipts or account setup change. Explicit EUR exchange-rate acceptance remains in worker core. |
| Receipt email resend | Manual capability check with inbox confirmation. Acknowledgements do not prove delivery or inherited content. The current delayed full scenario has not been recorded as rerun. |
| Receipt lifecycle | **Promote to optional acceptance**, if receipts are supported in practice. It already checks create/query/storno, tender totals, PDFs and duplicate call-id refusal. One distinct receipt journey is justified; all receipt probes are not. |

Full inventory and execution-record limitations: [agent assessment](2026-09-11-agent-live-test-assessment.md#actual-vendor-scenarios-3-core--7-probes), [receipt guide](../testing.md#L245-L274).

Possible small pruning: the EUR journey calls the full `repeated()` helper for both prepayment and final, in addition to the ordinary invoice. Keep each fresh-key `already_issued` assertion: it checks discovery after references have been consumed/settled. If simplifying, retain **same-key replay once** in the ordinary journey and omit it for the other two. The omitted calls normally replay retained completions rather than reach the vendor, so this is low priority and saves no documents ([helper:155–178](../../crates/restate-szamlazz/tests/live.rs#L155-L178), [EUR:319–327,370–378](../../crates/restate-szamlazz/tests/live.rs#L319-L378)).

## Deliberately keep out of the live core

- Concurrency, crashes, cancellation, delayed/lost answers, retention, unknown codes and uncertainty recovery. The controlled Restate/mocked-vendor suite can assert exact sends and inject these conditions. A live timeout cannot prove that the vendor did not execute a write. Keep this extensive coverage in CI ([main scenario list](../../crates/restate-szamlazz/tests/e2e/main.rs#L373-L438), [unresolved-write tests](../../crates/restate-szamlazz/tests/e2e/unresolved.rs)).
- The full input/schema/selector/error-code matrix. Its useful regression signals are local and deterministic.
- A separate live scenario for every worker facade handler. Direct proforma deletion and credit-entry semantics plus mocked worker handler tests are a reasonable division. Add a worker query read cheaply; prioritize a new corrective business flow over duplicating every direct mutation through Restate.
- Multi-account live provisioning as a requirement for library acceptance. The current live fixture is single-account/unscoped; scope isolation is exercised with real Restate and mocks. Deployed `check_account` and seller verification belong to deployment/rotation checks, with the actual deployed resolver/store ([guide:159–163](../testing.md#L159-L163)).
- Zero-total storno, delivery notes, simplified-item/preview combinations, special VAT categories and rendering permutations unless they are main uses or a specific incident calls for them. Their absence is a documented scope choice, not a claim that mock/XSD success proves vendor acceptance.

## Execution cadence and selection

Recommended policy:

1. **Every PR:** ordinary tests, schema validation and real-Restate/mocked-vendor e2e in CI.
2. **Manual release acceptance:** run the five core cases for substantial changes to client transport, wire formats, document conversion, arithmetic, or the worker write protocol. Do not require a new live run for documentation-only or unrelated changes.
3. **After a long inactive interval or a vendor change:** deliberately run core again as a drift check, with retained dated results. A quarterly manual check is a reasonable ceiling on staleness if no relevant release has already supplied evidence; it need not be a scheduled writer job.
4. **Capability-specific release:** additionally select receipt lifecycle or an established corrective journey when that capability is used/changed.
5. **Investigation:** select the exact probe answering the question. Keep dated observations separate from assertions about today's execution.

The most useful runner improvement is to expose **named scenario/experiment selection in manual Dagger**. Today `probes: true` runs all seven and requires receipt prefix/email, even when only clearing or appearance is under investigation. Local nextest filtering already solves this; the Dagger interface should offer similarly narrow selection instead of a growing all-probes switch ([Dagger:141–157](../../.dagger/modules/ci/main.dang#L141-L157), [guide:218–234,297–299](../testing.md#L218-L299)). Preserve the existing zero-retry policy and diagnostic handling of unresolved writes.

**Target size:** five core cases, optionally six with receipts; add a corrective journey only when it earns its place as a supported primary flow. The desired improvement is stronger evidence per document, rather than a larger live test count.
