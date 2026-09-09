# Architecture, naming, conventions, docs and tracker review, 2026-09-09

A five-reviewer review of the whole workspace at `8355ff5` (the day after #172 / ADR 0009 merged), asked four questions: are there seams not worth their value; is any naming off from the domain glossary; where does the code deviate from Rust conventions and built-in traits; is anything in the documentation or the tracker out of sync with the code.

## How it was produced

1. **Five reviewers**, each with a brief, read the code, CONTEXT.md, the ADRs and the two earlier reviews, and wrote a report (`raw/`): the worker (01); the Számla Agent crate and the CLI (02); the two receivers and the e2e harness crate (03); documentation drift, every concrete claim checked against the code (04); the issue tracker, every open issue checked against the code and the merged PRs (05).
2. **The lead** spot-checked the load-bearing claims (the `Static*` mirrors' rationale, `gross_total`'s callers, `Gateway: Clone`'s uses, the `Document` `#[non_exhaustive]` / README compile failure, the two `Totals` trees, the design doc's counts, the `support.rs` inventory, the harness forwarders), synthesised `REVIEW.md`, applied the documentation fixes, edited the stale issue bodies and published the tickets.

Clippy (pedantic, `unwrap_used`) and rustdoc clean on every crate; doctests green on every crate.

## What to read

| File | Trust | Use it for |
|---|---|---|
| [`REVIEW.md`](REVIEW.md) | lead-verified | The verdict, the nine candidates, the naming and convention tables, what was fixed in the docs and the tracker, the ticket map. **Start here.** |
| [`raw/01-restate-szamlazz.md`](raw/01-restate-szamlazz.md) | reviewer | The worker: seams, naming, conventions, what was checked and found fine. |
| [`raw/02-agent-cli.md`](raw/02-agent-cli.md) | reviewer | The Számla Agent crate and the CLI. |
| [`raw/03-receivers-harness.md`](raw/03-receivers-harness.md) | reviewer | adatkapcsolat, ipn, restate-e2e-harness. |
| [`raw/04-docs-drift.md`](raw/04-docs-drift.md) | reviewer | Every stale, ambiguous and fine claim per document; all STALE items are fixed in the tree. |
| [`raw/05-tracker.md`](raw/05-tracker.md) | reviewer | The per-issue verdict table as of before the edits. |

## Where the work is tracked

Tracking issue: [#173](https://github.com/sagikazarmark/szamlazz-rs/issues/173). Sixteen tickets, #174–#189; two of them (#176, #184) carry a decision. Nineteen existing issues had their bodies edited and every dead *Blocked by* edge removed.
