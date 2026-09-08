# Architecture-and-tests review, 2026-09-08

A four-reviewer review of the whole workspace two days after the 2026-09-06 one, asked four questions: is it well architected; is it well tested; is the suite stable, where is it fragile, can it be optimised and grouped; what could be tested in more isolation so that higher-level tests can shrink.

## How it was produced

1. **Four reviewers**, each with a brief, read the code and docs and wrote a report (`raw/`): architecture across all crates (01); the test pyramid of `restate-szamlazz` and the endpoint (02); the tests of the protocol crates, the CLI and the endpoint (03); stability and performance, with measurements: per-binary and per-test timings, a flakiness probe over four runs, a static fragility scan, the feature matrix, compile times (04).
2. **The lead** ran the un-ignored suite (579 green, 8.2 s) and the e2e (61 scenarios, 82.9 s), spot-checked the claims the tickets rest on, synthesised `REVIEW.md` and published the tickets.

Baseline: `cargo test --workspace --all-features --locked` 579 passed / 0 failed / 6 ignored; `-p restate-szamlazz --test e2e -- --ignored` green. Working tree at `4950e1f` with uncommitted edits being made while the review ran, so a `file:line` citation in `raw/` may be off by a few lines.

## What to read

| File | Trust | Use it for |
|---|---|---|
| [`REVIEW.md`](REVIEW.md) | lead-verified | The verdict, the baseline, the pyramid, the findings, what moves down the pyramid, grouping, compile cost, the ticket table. **Start here.** |
| [`raw/01-architecture.md`](raw/01-architecture.md) | reviewer | Module depth, coupling, type seams, the five seam-extraction proposals with signatures. |
| [`raw/02-worker-tests.md`](raw/02-worker-tests.md) | reviewer | Every `Szamlazz.Order` decision with its only test today; harness fragility; redundancy across layers; missing tests. |
| [`raw/03-protocol-tests.md`](raw/03-protocol-tests.md) | reviewer | Inventory and coverage matrix of the agent, ipn, adatkapcsolat, cli and endpoint crates. |
| [`raw/04-stability-performance.md`](raw/04-stability-performance.md) | reviewer, measured | Timings, the TLS root-store finding with its experiment, the fragility scan table, the feature matrix, the proposed aliases and nextest profiles, compile times. |

## Where the work is tracked

Tracking issue: [#135](https://github.com/sagikazarmark/szamlazz-rs/issues/135). Twenty-one tickets, #136–#156, with blocking edges; four of them split a row out of an existing umbrella (#68, #74, #75, #76) and say so.
