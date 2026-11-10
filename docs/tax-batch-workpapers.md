# Durable batch workpapers

Open Tax → Batch workpapers. Select 1–25 **saved NXT returns** with their exact revisions and choose Calculate, Capture workpaper JSON packages, or Capture printable workpaper HTML. Creating a job does not start it. Run/resume starts one item per server request; Run one step stops after one item. Refresh/reopen loads the durable job. Changed inputs/revisions, unsupported scope and storage limits produce explicit held outcomes.

Only existing calculation dispatchers are used. No imported SOURCE rows, formulas, review/approval, T183, filing, rollforward or automatic ownership mappings are added. Packages and print HTML reuse the individual export formatters and retain historical/current-scope qualifications. Print exports are internal workpapers, not certified returns or filed forms. Downloads are SHA-256/length checked by the browser.

Each job keeps its original selection manifest and hash, per-item outcomes, captured input/return hashes and the calculation snapshot/output hashes. Download the selection/outcomes manifest for reconciliation. Job metadata is revisioned; create/claim/reconcile/record/cancel/archive events append to the existing tax audit hash chain. This does not turn each mutable job revision into an independent hash-chain entry.

Cancel prevents new claims. A request already executing may commit and its outcome may arrive after cancellation. Resume reconciles committed keyed snapshots before starting work. A 30-second lease permits recovery after a terminated request; the same per-item calculation idempotency key and existing calculation CAS prevent duplicate snapshots. A cancelled expired claim without a committed keyed snapshot stays explicitly unconfirmed; inspect the return, and do not interpret cancellation as rollback. Such uncertain outcomes and exhausted retries cannot be archived automatically.

Completed/cancelled jobs with no uncertain in-flight effects can be explicitly archived. An immutable exact-byte copy is written first; the TaxState CAS registers it and removes the active entry. A crash between those steps is safely retried. No return/source data or captured history is deleted. Archived manifests/artifacts remain downloadable and cannot run. Idempotent creation using the original request key reopens the archived job instead of creating another.

Limits: 10 active jobs, 25 selected returns/job, 125 step keys/job, 5 claims/item, 1 MiB raw artifact, 2 MiB total active artifacts, 3 MiB artifact transport, 100 registered archived jobs/8 MiB archived raw data. Existing TaxState 8 MiB CAS and return/history limits still apply. The smaller remaining bound can hold a request; nothing is silently evicted. The browser executes at most 30 steps per Run click and requires explicit later resume if a lease makes no progress.

Routes (normal staff authentication/MFA/password policy applies):

- `GET/POST /api/tax/batches`: scoped active list / create.
- `GET /api/tax/batches/archives`: scoped archive index.
- `GET /api/tax/batches/{id}?company_ref=...`: sanitized detail (active or archived).
- `POST /api/tax/batches/{id}/step`: expected job revision, unique request key, exact company scope.
- `POST /api/tax/batches/{id}/cancel` and `/archive`: expected revision and exact company scope.
- `GET /api/tax/batches/{id}/manifest` and `/items/{index}/artifact`: exact same scope query.

A null company scope means the explicit multi-client selection; every selected return still needs current verified actor access. A non-null scope must exactly equal every selected current `client_ref`. All reads, steps and exports recheck **all items**, including archived bindings, so retargeting a return invalidates the old job view even for an admin. Staff preparers/reviewers/admins may mutate jobs; read-only staff can read only authorized jobs/artifacts. The archive blob is accessed only after its registered binding checks. New empty TaxState fields are omitted from serialization, preserving legacy empty-state encoding and return/snapshot hashes.

This is a bounded NXT batch workflow, not full DT Max batch parity or an atomic multi-return transaction. Earlier successful items remain committed if later items hold or the operator cancels. Preserve/review the manifest.
