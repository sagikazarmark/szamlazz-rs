# Raw reviewer reports, read the judges' verdicts first

These are the eight domain reports as the reviewers wrote them, **before** verification.
The judges in [`../judges/`](../judges/) re-read the cited code for every critical, high
and medium finding; some of what is here was refuted or downgraded, and a few
consequences were corrected (the final review's §4 lists them). Treat a claim here as
a lead, not a fact, unless the matching judge row says CONFIRMED.

| Report | Domain | Judged in |
|---|---|---|
| `01-agent-protocol.md` | Számla Agent crate protocol correctness | judge-A |
| `02-ipn-adatkapcsolat.md` | IPN and Adatkapcsolat receivers | judge-A |
| `03-restate-resilience.md` | Worker exactly-once and failure modes | judge-B |
| `04-spec-coverage.md` | Spec vs implementation checklist (128 claims) | judge-B |
| `05-architecture.md` | Architecture, complexity, API ergonomics | judge-C |
| `06-test-coverage.md` | Test coverage map, mutation spot-checks, CI | judge-B |
| `07-security-ops.md` | Credentials, isolation, receivers, container, supply chain | judge-C |
| `08-ux-api-docs.md` | Developer experience for three personas | judge-C |

Kept for provenance and for the detail the judges' one-line justifications compress
(the coverage map in 06 and the checklist table in 04 in particular).
