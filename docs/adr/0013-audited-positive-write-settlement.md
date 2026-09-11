# Audited positive settlement of an unresolved write

Accepted 2026-09-10 following the release-readiness review. An authorized operator may attest that the exact
unresolved write completed and cannot execute later, with an audit reference and an operation-specific result:
issued document, reversal document, or deleted proforma. This complements verified document evidence and
non-execution attestation: a successful deletion with a lost answer cannot truthfully be called non-execution,
and its absence is not proof of completion.

The complete marker must match. A deletion names exactly its pinned target; a reversal names a different number
from its original; issuance excludes the old reissue target. The operator's independent evidence must establish
the pinned account, order, operation and intended effect, including corrective base and reissue identity, and
exclude delayed execution. The worker validates the shape and target constraints, not the truth of the audit
record. The recovery receipt retains this as `completed` operator evidence, never as vendor verification.

Empty queries, elapsed time, cancellation and kill are insufficient. No timeout, force-clear or generic forget
operation is introduced. Recovery sends nothing; a subsequent business operation still requires fresh checks.
This explicit trust boundary is preferable to indefinite blocking even after independently confirmed completion,
or forcing operators to bypass the contract through administrative state deletion.
