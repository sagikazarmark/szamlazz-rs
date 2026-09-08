# Comprehensive review, 2026-09-06

A multi-agent review of the whole workspace at v0.3.0 (`0e4238c`), covering API and
developer experience, szamlazz.hu and Hungarian invoicing correctness, resilience
against the Számla Agent's idempotency-hostile design, architecture and complexity,
test coverage of the critical paths, spec conformance, security and operations.

## How it was produced

1. **Eight reviewers**, one per domain, each read the code and docs and wrote a report
   with a severity and a confidence per finding (`raw/`).
2. **Three judges** independently re-read every cited line for the critical / high /
   medium findings, refuted or downgraded the wrong ones, merged duplicates and
   assigned their own severity and confidence (`judges/`). They also added findings the
   reviewers missed (the judges' own IDs: `J-01`, `A1`–`A3`, `C-A`, `C-B`).
3. **A meta-judge** synthesised the three verdict tables into the final report
   (`FINAL-REVIEW.md`), re-reading the code for every top-10 entry.

Baseline at review time: `cargo test --workspace --all-targets --all-features --locked`
green, 434 passed, 0 failed; the docker-gated e2e and the three live tests ignored.

## What to read

| File | Trust | Use it for |
|---|---|---|
| [`FINAL-REVIEW.md`](FINAL-REVIEW.md) | verified | The verdict: executive summary, top 10, cross-cutting themes, every remaining verified finding grouped by crate (§5), the live probes the repo cannot settle (§6), the roadmap (§7). **Start here.** |
| [`judges/judge-A-protocol.md`](judges/judge-A-protocol.md) | verified | Per-finding verdicts for `szamlazz-agent`, `szamlazz-ipn`, `szamlazz-adatkapcsolat` (IDs `A-nn`, `B-nn`, `J-01`). |
| [`judges/judge-B-worker.md`](judges/judge-B-worker.md) | verified | Per-finding verdicts for `restate-szamlazz` resilience, spec conformance and test coverage (IDs `J1`–`J44`, `A1`–`A3`). |
| [`judges/judge-C-arch-sec-ux.md`](judges/judge-C-arch-sec-ux.md) | verified | Per-finding verdicts for architecture, security/operations and developer experience (IDs `J-05-nn`, `J-07-nn`, `J-08-nn`, `C-A`, `C-B`). |
| [`raw/`](raw/) | **unverified** | The eight reviewer reports as written. Superseded by the judges' verdicts, some findings in them were refuted or downgraded (see §4 of the final review). Retained for provenance and for the detail the judges' one-line justifications compress. |

## Finding IDs

Every finding has one stable ID, assigned by the judge that verified it; the final
report and the tracker issues use those IDs. A reviewer's original numbering
(`03 #17`) appears only in the judges' tables as the source column.

## Where the work is tracked

Tracking issue: [#59](https://github.com/sagikazarmark/szamlazz-rs/issues/59), the full ticket
list with blocking edges and a suggested order. `restate-szamlazz` and `szamlazz-agent` were
broken into PR-sized tickets (#60–#73); `szamlazz-adatkapcsolat`, `szamlazz-ipn`, `szamlazz-cli`
and the endpoint/CI/container got one umbrella issue each (#74–#77). Findings already covered by
an open issue were recorded as comments on that issue (#13, #14, #15, #17, #44, #45, #46) rather
than duplicated.
