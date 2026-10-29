# NXT Accounting automated client journey

Module: `src/client_journey/` (config `journey.json`, state machine + ledger in `mod.rs`). Tests: `tests/client_journey_flow.rs`.

## Source pattern: AIMOS "CVI 20/40" — not found
A search of `Stiggtechnologies/AIMOS` (main and all remote branches; terms `CVI`, `20/40`, `20-step`, `40-step`, `guided`) found no CVI 20/40 process. Bot memory had no record of it either. **The step structure below is an assumption**: 20 primary guided steps, and each one has an execute sub-step and a verify sub-step (40 in total). Update the `cvi_step` column once the real CVI definition turns up.

## Gating
- Steps run in order. A step can start only after every earlier step is `completed` or `skipped`.
- Statuses: `pending → in_progress → completed` (auto) or `in_progress → awaiting_approval → completed` (human, approver must be `Actor::Human`). `failed` can retry. A step marked skippable can go `pending → skipped`.
- Only legally required steps are human gates: engagement letter signature, filing authorization (T183), and filing submission.
- Every transition appends a `LedgerEntry` to the client's ledger (seq, step, from, to, actor, timestamp, note).
- `ESignProvider`, `FilingProvider` and `PaymentProvider` are interfaces with `StubHooks` only. Nothing sends, charges, or files.
- `data_migration` runs through `scripts/autonomous-client-path.py`, the migration orchestrator, which is owned by a separate workstream.

## Mapping
| # | CVI step | Key | Finance/accounting step | Gate | Skippable | Executor |
|---|---|---|---|---|---|---|
| 1 | Awareness / entry (assumed) | `website_visit` | Website visit and intent capture | auto | no | web analytics + intake widget |
| 2 | Capture (assumed) | `lead_capture` | Lead capture and contact record | auto | no | firm CRM intake |
| 3 | Discover (assumed) | `needs_discovery` | Needs discovery questionnaire | auto | no | guided intake questionnaire |
| 4 | Define (assumed) | `service_scoping` | Service definition and scoping | auto | no | scope rules engine |
| 5 | Offer (assumed) | `proposal_pricing` | Proposal and fixed-fee pricing | auto | no | pricing rules |
| 6 | Commit (assumed) | `engagement_letter` | Engagement letter signature | **HUMAN**: Engagement letter must be signed by the client and accepted by the responsible professional (CPA Canada / CPA Alberta practice standards). | no | ESignProvider (stub) |
| 7 | Onboard (assumed) | `client_onboarding` | Onboarding and client identity verification | auto | no | identity verification service |
| 8 | Collect (assumed) | `data_collection` | Data and document collection | auto | no | secure document request |
| 9 | Transfer (assumed) | `data_migration` | Data migration from prior system | auto | yes | scripts/autonomous-client-path.py (migration orchestrator) |
| 10 | Execute (assumed) | `bookkeeping` | Bookkeeping and adjustments | auto | yes | company_books / ledger |
| 11 | Verify (assumed) | `reconciliation` | Bank and subledger reconciliation | auto | no | ledger reconciliation |
| 12 | Quality check (assumed) | `automated_review` | Automated review and exception checks | auto | no | review rules |
| 13 | Prepare (assumed) | `deliverable_preparation` | Financial statements / return preparation | auto | no | engagement + tax modules |
| 14 | Confirm (assumed) | `client_review` | Client review of deliverables | auto | no | client portal |
| 15 | Authorize (assumed) | `filing_authorization` | Filing authorization (e.g. CRA T183 / T183CORP) | **HUMAN**: CRA requires the client's signed authorization before electronic filing. | yes | ESignProvider (stub) |
| 16 | Submit (assumed) | `filing_submission` | Return filing | **HUMAN**: Filing is submitted under the preparer's EFILE credentials; a human preparer must submit. | yes | FilingProvider (stub) |
| 17 | Deliver (assumed) | `delivery` | Deliverable package to client | auto | no | client portal |
| 18 | Bill (assumed) | `invoicing` | Invoice issued | auto | no | firm billing |
| 19 | Collect payment (assumed) | `payment` | Payment received | auto | no | PaymentProvider (stub) |
| 20 | Feedback / close (assumed) | `client_feedback` | Client feedback and close-out | auto | no | feedback survey |
