# Agent contract follow-up — implementation and verification

Implemented against starting commit `d85cdf29fab3dccc4a0d3eff209fda9303a263ce`, following the
[whole-crate review](2026-09-11-agent-api-28dcec1.md) and approved recommendations.

## Delivered

- Tri-state `InvoiceHeader::paid`, preserving omission by default. Missing/null
  JSON omits the field; explicit false now sends false. The Agent README documents
  migration from old false-as-omission values. The worker's existing boolean input
  retains its prior emission behavior.
- Optional reported invoice numbers on credit balances and fetched PDFs, without
  substituting request targets. Storno distinguishes `Numbered(CreatedInvoice)`
  from `Unnumbered(InvoiceAcknowledgement)`; both retain their metadata/artifact.
- Optional taxpayer validity, with present malformed/blank boolean still refused.
  Successful results preserve optional header, software and result diagnostics,
  including ordered notifications. Error results retain the existing code/message
  projection; additional error metadata is not exposed.
- Worker projection gates preserve definite taxpayer validity and journal privacy.
  CLI output distinguishes absent facts and an unconfirmed storno acknowledgement.
- Corrected code-56 scope, receipt provenance and mainstream-operation scope;
  documented breaking changes. Extended the Hungarian vendor-question draft.
  **Draft not sent; no vendor answer received.**
- XML ordering and customer-URL decoding retain their existing policies pending
  clarification. No new vendor execution evidence is claimed.

## Independent review

### Standards

No documented-standard violations. Two maintainability observations:

1. The worker's new string projection error was replaced with typed
   `MissingTaxpayerValidity`, implementing `std::error::Error` with fixed safe text.
2. PDF projection has two explicit metadata-copy branches for numbered versus
   unnumbered envelopes. Retained as a local, bounded duplication; no additional
   general-purpose response abstraction was introduced.

### Spec

One P2 found and fixed: unmanaged storno could have re-entered the issue retry
policy after an unnumbered acknowledgement. It now performs one read-only
reconciliation; an inconclusive result completes the run as journaled
`StornoOutcome::Unnumbered` data and raises `outcome_unknown` outside the retry
loop. An actual run-closure test checks completion, replayed fault, exactly one
mutation and exclusion of acknowledgement metadata/PDF. Independent re-review
confirmed the policy-driven resend gap closed, including failed reconciliation.

Crash-before-journaling re-execution remains the existing unmanaged-write
limitation. This change does not extend Order's durable send-permission mechanism
to unmanaged storno.

## Verification

- `cargo test --workspace --all-features --locked --offline`: **773 passed**,
  zero failed, including doctests. External live/Restate scenarios stayed ignored.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`: passed.
- `cargo fmt --all --check`, `git diff --check`: passed.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p szamlazz-agent -p restate-szamlazz --all-features --no-deps --locked --offline`: passed.
- Required `scripts/check-agent-schemas.py`, with installed libxml2 on PATH:
  430 generated requests, 860 validations, 790 valid and 70 exact expected
  vendor-source conflicts; seven negative controls passed; complete declared-path
  coverage for each retained schema.
- `python3 scripts/test-agent-schema-runner.py`: eight passed.
- `python3 scripts/test-order-migration.py`: five passed.
- `cargo hack` was unavailable. Explicit locked/offline `cargo check` covered
  Agent without features, worker without features, worker with `schemars`, and
  worker with `test-util`; all-feature checks above cover their remaining
  combinations and the CLI.

No authenticated requests, PDF rendering or fresh browser execution performed.
Existing review reports were preserved.
