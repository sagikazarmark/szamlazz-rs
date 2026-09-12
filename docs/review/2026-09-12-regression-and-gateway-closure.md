# Regression and Gateway architecture closure

Base: `e5fc6066bb0ce747e2f3ec83d2b13b6e9cd02e3b`.
Status: implemented and verified; included with this repair commit, not released.

The owner requested implementation of all recommendations from the independent
review of current code, the last 20 commits (`f83e5fd...e5fc606`), and the repository's
historical review evidence. That review followed the
[previous repair closure](2026-09-11-review-fixes-closure.md). Its temporary full
report did not survive the session interruption; the findings and dispositions
are retained here so closure does not depend on temporary files.

## Finding-to-fix index

| Finding | Resolution | Regression evidence |
|---|---|---|
| C1: public create called uncertain post-send observations settled | Immediate reconciliation uses shared intent-aware evidence; collision, credentials preventing verification, and old reissue targets retain `Unconfirmed` | Public Gateway create/recovery tests assert one send and correct outcomes, including wrong corrective bases |
| C2: failed diagnostic query erased a conclusive 71/152 refusal | Both creation paths retain the sole-send refusal as data; optional diagnostics cannot turn it into uncertainty | Duplicate tests cover external-id and order-hint failures, including credential diagnostics |
| C3: JSON-specific monetary/newtype adapters broke direct RON round trips | Raw-token decoding is confined to recognized concrete JSON deserializers; other formats use their ordinary scalar/enum representation | Named/unnamed RON, optional money, creation/storno enums, JSON member ordering and lookalike-object controls |
| C4: IPN rejected representable exponent spellings such as `1.00e-28` | Exponent adjustment normalizes redundant scale while independently checking the significand and preserving representable scale | Number/string, zero, scale, range and exactness tests |
| C5: downstream Decimal features changed serialization and lost digits | Monetary serialization is explicitly strings across Agent, worker and Adatkapcsolat; worker response/journal decoding is explicitly exact too | Eight dependency graphs, JSON text/bytes and actual Restate SDK codec replay |
| C6: Adatkapcsolat accepted multiple roots and ignored malformed unknown content | Full XML lexical/structural check before typed parsing, with identity/content policy preserved | Concatenated roots, exterior text, invalid characters/references, ignored attributes/elements, HTTP/archive regression |
| C7: Adatkapcsolat rejected XML-equivalent escaped namespaces | Normalize namespace attribute values and validate scopes/expanded attribute identities | All four roots, nested scopes, character-reference spellings and known/unknown-key HTTP paths |
| D1: active behavior reference prescribed superseded resend/settlement rules | Updated design consequences, kept dated observations and explicit uncertainty about vendor behavior | Documentation reviewed against current protocol, ADR amendments and implementation |
| D2: one-shot rustdoc promised no re-execution | Distinguishes recorded-result replay from interruption before journaling | Existing actual-Restate credit-entry interruption test remains the counterexample to the old promise |

## Architectural decision

[ADR 0014](../adr/0014-gateway-as-an-expert-orchestration-interface.md) records the
owner's September 12 approval: retain `Szamlazz.Order → Gateway ← Szamlazz.Agent`
and explicitly support expert custom orchestrators. Ordinary applications use Order.
The historical shared-module and embedder decisions are provenance, not a claim
that the newly approved consumer requirement existed previously.

`CreatePermission` remains a consumed caller assertion, not durable authorization.
`CreateStepRequest::operation()` extracts retainable intent, and public
`Gateway::reconcile(ReconciliationRequest)` shares evidence rules with protected
Order. Only matching issuance or verified reversal is positive evidence. Exact
candidates, expected old reissue targets and corrective bases are checked; reversal
requires the matching storno and a fresh matching reversed original. Deletion
cannot be established by document queries. Callers still own durable exclusion,
uncertainty retention and recording settlement.

This also addresses the review's duplication judgment through shared corrective
intent predicates and resolves its speculative-generality question through an
explicitly accepted public use case.

## Follow-up findings and integration repairs

Independent review and full-suite execution caught additional gaps before closure:

1. **Worker replay feature combination:** `serde-str + serde-float` (including
   `serde-bincode`'s alias) rejected serialized strings through the SDK. A `Value`
   round trip hid the failure. Explicit worker decoding and actual byte/SDK tests
   close it; the matrix now includes those combinations.
2. **DTD grammar:** the initial XML tokenizer accepted malformed declarations and
   rejected a legal quoted `>` in an attribute declaration. An explicit XML 1.0
   internal-subset grammar pass covers declarations, literals, references and PIs.
   A further recheck caught namespace-name restrictions; context-specific QName
   and NCName checks now cover declaration and reference positions. DTDs are not
   expanded, defaults are not applied and external resources are never fetched.
   The independent XML reviewer rechecked every reported case and signed off.
3. **CLI wrapper:** `serde_ignored` obscured the concrete JSON parser and caused
   fractional numeric input to be refused. The scalar decoder recognizes only
   that wrapper directly over the concrete parser, whose RawValue forwarding was
   source-checked. Buffered adapters remain conservative. Executable tests retain
   exactness, object refusal, unknown-field reporting, duplicates and trailing-input
   rejection. Whole-envelope decoding receives no wrapper bypass.
4. **Interrupted deletion fixture:** shared reconciliation correctly performs no
   query for deletion, but a historical test required one. The updated test requires
   exactly zero queries, verifies `reconcile-write`, and checks cancellation retains
   the exact unresolved deletion marker. Existing read/send guards remain asserted.

Concrete JSON dispatch uses `type_name`, because Serde exposes no raw-token format
capability query. Its textual representation is not a Rust stability guarantee.
This remains an acknowledged maintenance judgment, not a demonstrated current
failure: alias/re-export, direct/wrapped JSON and alternate-format probes passed,
and unrecognized adapters receive conservative scalar decoding rather than trusting
ambiguous maps. Dependency/compiler changes must retain the boundary checks.

## Verification

Successful final checks (separate executions, not one claimed combined run):

| Check | Result |
|---|---|
| `dagger --progress plain check --no-generate ci:test` | **823 ordinary tests passed**, 40 skipped; **25 doctests passed** |
| `dagger --progress plain check --no-generate ci:end-to-end` | **39 actual-Restate/mocked-vendor tests passed**, including the unfiltered main scenario suite and run-wide checks |
| `dagger --progress plain check --no-generate rust:check` | **37 feature-powerset checks passed** |
| `bash scripts/check-money-features.sh` | **65 tests × 8 graphs = 520 test executions passed**, including executable CLI and SDK journal coverage |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed after final test-only lint cleanup |
| `cargo fmt --all --check`; `git diff --check` | Passed |

Traces:

- [ordinary tests/doctests](https://dagger.cloud/sagikazarmark/traces/b21e21fe98b5cee93fcd4d6b420410c0)
- [end-to-end](https://dagger.cloud/sagikazarmark/traces/cd3693bfae3dad0d1955f351200394b7)
- [feature powerset](https://dagger.cloud/sagikazarmark/traces/e5cd2bf2951e6bf7809141f8833ed892)

The initial combined command
`dagger --progress plain check --no-generate ci:test ci:end-to-end ci:schemas`
first hit the outer 120-second execution deadline during cold compilation. Its
longer rerun completed schemas successfully but exposed the CLI and deletion-fixture
failures above; both affected checks were subsequently rerun successfully.
The schema check validated **430 generated requests against two retained sources**
(860 validations: 790 valid and 70 expected source conflicts), plus its controls.
Subsequent repairs changed response decoding, XML receiving and tests, not request
XML generation. Schema evidence remains offline and source-specific.

Independent final review outcomes:

- **Standards:** zero documented violations or blockers.
- **Spec/Gateway:** no actionable defect in the approved interface or protected
  protocol; conclusive refusals and uncertain sends remain distinct.
- **Money:** original regressions and SDK replay follow-up closed; final CLI
  wrapper exception separately reviewed and signed off.
- **XML:** multiple-root/namespace defects and all reported DTD follow-ups closed.

## Evidence boundary and migration

No vendor-live calls, credential reads or releases were performed. The owner
subsequently requested a commit containing these repairs and this closure record.
Genuine IPN captures, unanswered vendor questions and deployment seller/scope checks
remain separate from local correctness. Candidate-specific live acceptance remains
the documented release activity, not a result claimed by this closure.

Existing `create_once` signatures remain usable; expert callers can now retain
`operation()` with account/order/external-id identity and use public `reconcile`.
Monetary JSON output is consistently strings under downstream feature combinations;
applications relying on feature-induced float output must use the documented shape.
Malformed Adatkapcsolat XML that previously reached handlers is now refused, while
equivalent namespace spellings are accepted. The original raw XML remains intact.

The implementation stopping criterion is met within this reviewed scope: reported
correctness findings are closed, the public Gateway consumer decision is explicit,
and the corresponding checks pass. Historical reports retain their original scope
and approval statements; this record does not rewrite them or certify all possible
vendor behavior.
