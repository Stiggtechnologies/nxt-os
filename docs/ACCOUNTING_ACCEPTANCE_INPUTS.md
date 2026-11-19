
# Firm inputs for accounting acceptance

Provide all inputs through approved firm systems. Keep production credentials, taxpayer identifiers, and unredacted workpapers out of the acceptance repo and manifests. Use a dedicated sandbox workspace with approved synthetic fixtures or authorized minimized records. Each test result should identify the release and control digests, exact profile, fixture, expected and actual outcome, reviewer, and receipt or trace. Professional and client decisions stay with the authorized people.

## Common inputs

- Name accountable firm operations, security and privacy, records, service-line, and professional reviewers. Provide individual test identities with firm-approved roles, MFA, segregated duties, and at least one separately authorized backup operator.
- Supply the service, jurisdiction, license, authority, and integration matrices for each profile to be enabled. Provide a sandbox endpoint, credentials scoped to that sandbox, provider documentation, supported read-back and idempotency behavior, and a manual fallback channel for each intended connector. Provision secrets through the approved secret manager.
- Approve the archive inventory and data flows by client, matter, purpose, processing region, vendor, sensitivity, retention class, legal hold, and OCR or embedding use. Identify the source permissions system, authoritative client mapping, archive exclusions, and independently measured coverage. Supply incident, backup, and restore contacts.
- Select or approve the embedding model, model version, source and query embedding jobs, quantization and dimension, evaluation corpus, and sensitive-data processing location. Hybrid retrieval accepts matched precomputed vectors. The app makes no embedding-provider call.
- Freeze the acceptance release manifest with workflow, rules, model, tools, indexes, and connector versions. Supply time limits, load targets, thresholds, tolerances, and an auditable reviewer sign-off for each required control.

## Case-by-case inputs

| Case | Firm must provide |
| --- | --- |
| LW-01A | A source extract with independently verified record counts and control totals, plus the omitted-batch variant and expected posting block. |
| LW-01B | An independently prepared close workpaper, known journals, duplicate and rejection outcomes, rounding rules, approved tolerances, and sign-off. |
| LW-01C | Approved jurisdiction boundary and historical-period fixtures with expected obligations, deadlines, applicable authority, and an omitted-obligation case. |
| LW-02A | Administrative authority matrix, a pair of distinct proposal and approval identities, and the actual admin entry points that must reject self-escalation. |
| LW-02B | Revocable connector and staff authority in sandbox, an instrumented queued or in-flight attempt, and the expected read-back and cutoff behavior. |
| LW-02C | Emergency role policy, expiry and independent review service targets, test identities, and a mandatory client or practitioner decision to exercise. |
| LW-03A | Approved connector schema and model version inventory, a deliberately incompatible substitute, and the expected deployment alert destination. |
| LW-03B | An active workflow with approved prior and replacement rule versions, expected migration or pinning behavior, and a reviewer for any new decision. |
| LW-03C | Two approved sandbox releases, rollback access and owner, and an independent record of revoked scopes and prior external receipts. |
| LW-04A | Approved provider sandbox, two eligible manual and automated actors, a shared test business key, and a destination count or receipt query. |
| LW-04B | A sandbox provider that can accept then withhold an acknowledgment, read-back credentials, and an independent operator authorized to resolve it. |
| LW-04C | A posted sandbox transaction or submission, the approved correction process, correction approver, notification fixture, and expected corrected destination state. |
| LW-05A | A reconciled archive inventory, authorized access to representative years and repositories, and fixtures for both a true no-match and an unavailable required collection. |
| LW-05B | Authoritative permission-feed integration, identities with distinct grants, permission freshness and revocation timing targets, and an environment for in-flight tests. |
| LW-05C | Approved dispositions, an active hold, and a stale backup with a documented restore procedure and verifiable expiry or deletion targets. |
| LW-06A | Signed profile-specific control matrix with authority, effective dates, limits, approvals, and an independently reviewed mapping to passing evidence. |
| LW-06B | Valid and expired credential fixtures, supported and unsupported channels, and an out-of-scope entity with expected denial paths. |
| LW-06C | Service-owner-approved old and new authority versions, impacted open and queued actions, and expected historical selection and invalidation. |
| LW-07A | Measured peak volumes, queue and reconciliation targets, test infrastructure, alert routing, and staffing assumptions. |
| LW-07B | Approved recovery time and data-loss objectives, restorable sandbox backups, receipts, active holds and revocations, and a scheduled recovery exercise. |
| LW-07C | Deadline-day simulation calendar, provider outage exercise, assigned fallback operators, manual throughput target, and destination reconciliation access. |

## Release conditions

The built-in journal connector writes only to a separate sandbox keyspace. It supports local evidence for the dispatch controls and supplies no evidence about the firm’s external provider. Keep production writes disabled until all 21 applicable cases have independent evidence, accountable reviewers have signed the exact production profile and release, and approved routing and recovery are verified.
