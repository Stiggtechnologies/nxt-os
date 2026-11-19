# Accounting production acceptance blockers

Live writes remain disabled. The continuity cases use a checked-in synthetic fixture with named test identities, two versioned releases, a sandbox journal adapter, queue and recovery targets, and a fault injected before submission. The 21-case report separates local evidence from production acceptance. The firm has not supplied an approved fixture set or authorized a deployment for production acceptance.

## Six continuity cases

| Case | Sandbox exercise | Production evidence still required |
| --- | --- | --- |
| LW-02C | Persist a time-limited emergency grant, reject use at expiry, allow pause only, and verify that dispatch admission stays closed after a pause. | Approved emergency role policy and named MFA identities, configured expiry, independently supervised review, and proof that real authorization and signature gates remain enforced. |
| LW-03A | Reject unpinned connector, model, and manifest identifiers. Exercise the permitted claim against the existing sandbox operation store. | Signed version inventory and deployed adapter and model routing controls, incompatible-version tests, and alert delivery. |
| LW-03C | Roll back to an earlier approved fixture manifest while preserving a revoked identity and a completed journal receipt. | Approved rollback releases, production-like deployment rollback and exact manifests, current policy reconciliation, and authoritative destination receipts. |
| LW-07A | Reconcile 24 sandbox journals over four concurrent workers and check virtual queue and deadline targets. | Firm-approved peak volumes, actual queue and reconciliation latency targets, representative infrastructure, and alerting to named operators. |
| LW-07B | Restore a pending attempt from an in-memory snapshot. Replay a revocation and hold, read the separately stored provider receipt, and reconcile once. | Approved recovery time and data-loss targets, durable backup and restore, revocation and hold replay from authoritative sources, and independent read-back through an approved provider. |
| LW-07C | Inject provider unavailability before submission. Route six sandbox journals to two assigned fallback identities, check the virtual deadline, and reject duplicate automated claims when the provider returns. | A provider outage exercise on the approved sandbox, actual deadline and staffing assumptions, verified manual mandates, recovery reconciliation, and evidence of operator throughput. |

The fixture uses a local operation state machine and isolated destination. The release and emergency checks run inside the sandbox harness. The load and fallback schedules use fixture time. These exercises do not establish deployed throughput, external exactly-once behavior, or production operational controls.

## Remaining production prerequisites by gate

| Gate | What the firm must provide and verify |
| --- | --- |
| LW-01 Accuracy | Independently reviewed close and jurisdiction reference cases, verified source counts and control totals, approved tolerances, and service-owner sign-off. |
| LW-02 Authority | A deployed authority matrix, individual MFA and connector identities, independently approved admin changes, revocation at dispatch, and tested emergency controls. |
| LW-03 Release management | An approved versioned release manifest, pinned connector and model routing, change and migration tests on active workflows, and a rollback drill with current policy enforcement. |
| LW-04 Execution | Provider-specific sandbox access, exclusive manual and automated claims, definitive read-back, correction authority, and notification and ledger reconciliation evidence. |
| LW-05 Archive | Firm-approved archive inventory, permission and purpose approvals, regional processing configuration, access-feed timing targets, and verified revocation, legal hold, disposition, and backup restore. |
| LW-06 Firm controls | The signed service and jurisdiction matrix with credentials, connector methods, rule versions, limits, reviewers, and independently reviewed test evidence for each applicable control. |
| LW-07 Continuity | Traffic and deadline targets, durable recovery infrastructure, assigned operational owners, and independently supervised peak-load, restore, and deadline-day fallback exercises. |

Each launch profile needs authorization from the designated firm owners against the exact release and control digests. The accounting extension has no production dispatch adapter or activation path.

Use the [executable acceptance plan](ACCOUNTING_EXECUTABLE_ACCEPTANCE.md) to run the suite and identify required case inputs, owner approvals, and independent review evidence per launch profile.
