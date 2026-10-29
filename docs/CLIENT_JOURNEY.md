# NXT Accounting guided client walk

Module: `src/client_journey/` (`journey.json` steps, `ladder.json` value ladder, state machine + ledger + progress in `mod.rs`, ladder logic in `ladder.rs`). Tests: `tests/client_journey_flow.rs`.

## Principle

Guided client walk: the client is walked one step at a time. Each step has one clear action, a visible outcome or value delivered, a gate that must pass before the next step, and shows progress so the client always knows where they are and what is next.

The step count is whatever the accounting client's life needs (currently 24 steps across 9 phases). There is no fixed 20/40 split.

## Gating and progress

- Steps run in order. A step can start only after every earlier step is `completed` or `skipped`. The `gate_check` column states what must be true before the next step.
- Statuses: `pending → in_progress → completed` (auto) or `in_progress → awaiting_approval → completed` (human; approver must be `Actor::Human`). `failed` can retry. Skippable steps can go `pending → skipped` (for example payroll for a client with no employees).
- Human-only gates: engagement letter, filing authorization (T183 / T183CORP), and e-file.
- `ClientJourney::progress()` returns done/total/percent, the current phase, the current step's action, outcome and gate, and the next step, so the client always sees where they are and what is next.
- Every step transition appends a `LedgerEntry` (seq, step, from, to, actor, timestamp, note). Ledgers are append-only.
- `ESignProvider`, `FilingProvider`, `PaymentProvider` and `OfferSender` are interfaces with `StubHooks` only. Nothing sends, charges, signs, or files.
- `data_migration` hands off to `scripts/autonomous-client-path.py`, the migration orchestrator, which is owned by a separate workstream.

## Walk mapping

| # | Phase | Key | Step | One clear action | Outcome / value delivered | Gate before next step | Gate type | Skippable | Executor | Offer point |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | first_contact | `enquiry_received` | Enquiry received | Client submits the enquiry form or calls the firm | Acknowledgement sent with a booked discovery call and a link to their walk progress | Contact record exists and discovery call is booked | auto | no | firm CRM intake |  |
| 2 | first_contact | `discovery_call` | Discovery call | Client describes their business, entity type, current software and pain points | Written summary of needs and the free books health check explained | Needs summary saved and client agrees to the health check | auto | no | discovery questionnaire + call notes |  |
| 3 | diagnosis | `books_access` | Grant read-only books access | Client grants read-only access or uploads exports (QBO, bank statements, prior returns) | Firm confirms what it received and what is missing | Required exports or read-only access confirmed | auto | no | secure document request / source_import |  |
| 4 | diagnosis | `books_health_check` | Free books health check | Firm runs the diagnostic on the client's books | Scored health check report: reconciliation status, months behind, exceptions, filing exposure | Report generated and reviewed by a staff accountant | auto | no | company_books + ledger diagnostics | next rung above `health_check` |
| 5 | diagnosis | `health_check_review` | Health check walkthrough | Client reviews the findings with the firm | Client understands their books' condition and the recommended next rung | Client chooses a path (or declines); next-rung offer approved by a human before it is sent | auto | no | client portal + value ladder |  |
| 6 | proposal | `proposal` | Proposal | Client receives a scoped proposal for the chosen rung(s) | Fixed scope, deliverables and timeline in writing (pricing TBD by Orville) | Client accepts the proposal | auto | no | scope rules + value ladder |  |
| 7 | engagement | `engagement_letter` | Engagement letter | Client signs the engagement letter; responsible professional accepts it | Signed engagement on file; client is now a client of the firm | Signed by client AND accepted by a human professional | **HUMAN**: Engagement letter must be signed by the client and accepted by the responsible professional (CPA Canada / CPA Alberta practice standards). | no | ESignProvider (stub) |  |
| 8 | onboarding | `client_onboarding` | Onboarding and identity verification | Client completes identity verification and onboarding profile | Verified client profile and portal access | Identity verification passed | auto | no | identity verification service |  |
| 9 | onboarding | `document_collection` | Document collection | Client uploads outstanding documents from the checklist | Checklist shows every item received or waived | All required documents received | auto | no | secure document request |  |
| 10 | onboarding | `data_migration` | Data migration | Firm migrates prior-system data (QuickBooks, Caseware, DT Max, WebTax) | Client's history is in NXT OS with a migration report | Migration report passes reconciliation checks | auto | yes | scripts/autonomous-client-path.py (migration orchestrator) |  |
| 11 | onboarding | `catch_up_cleanup` | Catch-up / cleanup | Firm brings books current and fixes historical errors | Clean, reconciled books up to the current period | All catch-up months reconciled with no open exceptions | auto | yes | company_books / ledger |  |
| 12 | monthly_close | `monthly_close` | Monthly close | Client sends the month's documents; firm closes the month | Reconciled bank and subledgers for the month | Month reconciled; exceptions resolved or carried forward with a note | auto | yes | ledger reconciliation |  |
| 13 | monthly_close | `monthly_report` | Monthly report | Client receives the month-end package | P&L, balance sheet and exceptions summary in the portal | Report delivered; next-rung signals evaluated and any offer approved by a human before it is sent | auto | yes | company_books reports + value ladder | next rung above `monthly_bookkeeping` |
| 14 | monthly_close | `payroll_setup` | Payroll | Client confirms employees and pay schedule | Payroll running with remittances tracked | First pay run reconciled | auto | yes | payroll module |  |
| 15 | year_end | `year_end_statements` | Year-end statements | Firm prepares the year-end financial statements | Draft year-end financial statements | Statements prepared and reviewed | auto | yes | engagement module |  |
| 16 | year_end | `year_end_review` | Client review of statements | Client reviews and approves the year-end statements | Final year-end statements | Client approval recorded | auto | yes | client portal |  |
| 17 | tax | `tax_preparation` | Tax return preparation | Firm prepares the T2 / T1 return | Draft return with a summary of tax owing or refund | Return prepared and reviewed | auto | yes | tax module |  |
| 18 | tax | `filing_authorization` | Filing authorization (T183 / T183CORP) | Client signs the CRA filing authorization | Signed authorization on file | Signed T183 / T183CORP received and verified by a human | **HUMAN**: CRA requires the client's signed authorization before electronic filing. | yes | ESignProvider (stub) |  |
| 19 | tax | `filing_submission` | E-file | Preparer submits the return | Return filed, with the CRA confirmation number saved | Filing confirmation recorded by the preparer | **HUMAN**: Filing is submitted under the preparer's EFILE credentials; a human preparer must submit. | yes | FilingProvider (stub) |  |
| 20 | tax | `invoicing` | Invoice | Client receives the invoice for the period | Clear invoice tied to the delivered work | Invoice issued | auto | no | firm billing |  |
| 21 | tax | `payment` | Payment | Client pays the invoice | Receipt issued | Payment received | auto | no | PaymentProvider (stub) |  |
| 22 | advisory | `annual_review` | Annual review | Client meets the firm to review the year | Year in review, KPIs and recommendations; next-rung offer if signals fire | Review held; any offer approved by a human before it is sent | auto | no | value ladder | next rung above `year_end_tax` |
| 23 | advisory | `cfo_advisory` | CFO / advisory | Client agrees forecasting and KPI targets | Forecast and KPI dashboard | Dashboard live and first advisory session held | auto | yes | advisory (planned) | next rung above `cfo_advisory` |
| 24 | advisory | `strategic_readiness` | Strategic readiness | Client sets an exit, acquisition or financing goal | Readiness plan and data room checklist | Plan agreed | auto | yes | advisory (planned) |  |

## Value ladder

Pricing on every rung is a placeholder: **pricing TBD by Orville**. Triggers are the signals that recommend moving a client UP TO that rung (any one firing is enough). `Ladder::recommend_next(current, signals)` returns the lowest rung above the client's current rung whose triggers fire, with the reasons.

| Rung | Tier | Entry criteria | Deliverables | Pricing model (placeholder) | Triggers that recommend this rung |
|---|---|---|---|---|---|
| 1 | Free books health check (`health_check`) | Any prospect or client with books to assess | Scored books health report; Months-behind and reconciliation status; Filing exposure summary | Free diagnostic (pricing TBD by Orville) | entry rung (offered to every prospect) |
| 2 | Catch-up / cleanup (`catch_up_cleanup`) | Books are behind or have unreconciled history | Books brought current; Historical reconciliation; Opening balances confirmed | Fixed fee per month behind (pricing TBD by Orville) | `months_behind >= 1`; `reconciliation_exceptions >= 10` |
| 3 | Monthly bookkeeping (`monthly_bookkeeping`) | Books current, or cleanup complete | Monthly close; Bank and subledger reconciliation; Monthly report | Monthly subscription tiered by transaction volume (pricing TBD by Orville) | `monthly_transactions >= 50` |
| 4 | Payroll (`payroll`) | On monthly bookkeeping and has or is adding employees | Pay runs; Source deduction remittances; T4 / ROE support | Per employee per pay run (pricing TBD by Orville) | `employees >= 1` |
| 5 | Year-end statements + T2/T1 tax (`year_end_tax`) | Fiscal year-end reached with closed months | Year-end financial statements; T2 or T1 return; Filing (human-gated) | Annual fixed fee by entity complexity (pricing TBD by Orville) | `months_to_year_end <= 3` |
| 6 | CFO / advisory (`cfo_advisory`) | On monthly bookkeeping with a completed year-end | Rolling forecast; KPI dashboard; Quarterly advisory session | Monthly retainer (pricing TBD by Orville) | `revenue_growth_pct >= 20`; `annual_revenue >= 1e+06` |
| 7 | Strategic (exit / acquisition readiness) (`strategic`) | On CFO/advisory with a stated transaction goal | Quality-of-earnings style cleanup; Data room checklist; Valuation readiness plan | Project fee (pricing TBD by Orville) | `exit_horizon_years <= 3` |

## Offers (human approval required)

- At an offer point (`books_health_check`, `monthly_report`, `annual_review`, `cfo_advisory`), `draft_offer` evaluates the client's signals and drafts the next-rung offer. Drafting only works once the step has been reached.
- `approve_offer` / `decline_offer` require `Actor::Human`. `send_offer` only works on an approved offer and calls the stub `OfferSender`.
- Every offer event (drafted, approved, declined, sent) is appended to the client's `offers` ledger.

## Open items

- Pricing for every rung: TBD by Orville.
- Trigger thresholds are starting examples and need tuning against real client data.
- Payroll, CFO/advisory and strategic executors are planned modules; the steps exist but the work is manual today.
- Signals are passed in by the caller; wiring them from company_books / ledger metrics is not done yet.
