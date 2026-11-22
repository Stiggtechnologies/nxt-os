# REA capability map and gap closure — 2026-11-05

Method: REA (docs/REA_GAP_ANALYSIS_2026-10-29.md). Each row was checked against code/routes, not nav labels. Status: present / partial / missing / behaves differently.

## Counts

 | App | Present | Partial | Missing | Behaves differently |
|---|---|---|---|---|
| QuickBooks | 13 | 7 | 5 | 1 |
| Firm | 5 | 4 | 1 | 0 |
| Caseware | 5 | 3 | 3 | 0 |
| DocuSign | 6 | 3 | 2 | 1 |
| DT Max | 5 | 4 | 2 | 0 |
| WebTax | 1 | 4 | 0 | 1 |

## QuickBooks

| Module | Capability | Status | Evidence |
|---|---|---|---|
| Chart of accounts | Account list, types, sub-accounts | present | src/ledger/routes.rs /api/ledger/accounts |
| Sales | Invoices + receive payment | present | /api/ledger/invoices, /payments |
| Sales | Estimates | partial | company_books estimates; not a ledger DocKind (ledger DocKind = Invoice|Bill) |
| Sales | Credit memos / sales receipts / refunds | missing | No DocKind in src/ledger |
| Sales | Customer statements | missing | No statement route/UI |
| Expenses | Bills, bill pay, vendors | present | /api/ledger/bills, contacts |
| Expenses | Purchase orders | partial | company_books purchase-orders; no PO→bill conversion in ledger |
| Banking | Bank feed import, categorize, exclude | present | /api/ledger/feed/* |
| Banking | Bank rules management | behaves differently | BankRule only created implicitly on categorize; no list/edit/delete route |
| Banking | Reconcile | present | /api/ledger/reconcile |
| Banking | Transfers, tax payments | present | /api/ledger/transfers, /tax-payments |
| Transactions | Journal entries + void | present | /api/ledger/entries, void |
| Transactions | Recurring / memorized transactions | missing | Nav label only: public/apps/ledger/qb-navigation.json |
| Reports | P&L, balance sheet, TB, GL, AR/AP aging, sales tax | present | run_report in src/ledger/routes.rs |
| Reports | Statement of cash flows | present | NEW PR #161 cash-flow |
| Reports | P&L by month / prior-year comparison | present | NEW PR #161 |
| Reports | Customer / vendor balance summary | present | NEW PR #161 |
| Lists | Classes, departments/locations, items | partial | company_books classes/departments/items; not tagged on ledger lines or reports |
| Payroll | Payroll runs / exports | partial | company_books payroll + import_parsers payroll; no remittance/T4 run |
| Time | Time tracking | partial | company_books time; no billable-time→invoice |
| Budgets | Budgets & budget vs actual | partial | company_books budgets; no BvA report in ledger |
| Close | Closing date / period lock | present | company_books period-lock |
| Audit | Audit log | present | /api/ledger/audit |
| Multicurrency | Foreign currency + revaluation | missing | No currency field in ledger |
| Accountant | Write-off invoices, reclassify transactions | missing | No route |
| Desktop data | IIF / QBD report import | partial | scripts/read-iif.py, qbd-report-profiles.py (read-only) |

## Firm

| Module | Capability | Status | Evidence |
|---|---|---|---|
| Clients | Client/contact records | present | src/firm contacts |
| Workflow | Projects, templates, workflows | present | src/firm projects/templates/workflows |
| Workflow | Monthly closes, approvals, playbooks | present | src/firm closes/approvals/playbooks |
| Staff | Staff accounts + MFA | present | src/firm staff MFA |
| Time & billing | Time entry, WIP, invoicing clients | missing | No firm-level time/WIP/billing module |
| Documents | Client portal / document requests | partial | engagement attachments; client contact features intentionally disabled |
| Reporting | Exports CSV/XLSX | present | src/firm exports |
| Integrations | Provider connections | partial | src/firm integrations scaffold; creds external |
| Excel | Spreadsheet sync / add-in | partial | spreadsheet-sync read-only |
| Scheduling | Due-date calendar / deadlines | partial | close/project due dates; no CRA deadline calendar |

## Caseware

| Module | Capability | Status | Evidence |
|---|---|---|---|
| Engagement | Engagement setup, rollforward, lockdown | present | src/engagement rollforward, lockdown |
| Mapping | Account mapping / GIFI / leadsheets | present | engagement account-anchors, gifi; leadsheets in statements.rs |
| Adjustments | AJE worksheet (adjusting/reclass/proposed) | partial | Uncorrected misstatements in statements.rs; no typed AJE worksheet with posting |
| Documents | Document manager, attachments, sign-offs, tickmarks | present | engagement attachments/signoffs/tickmarks |
| Financial statements | FS generation + PDF | present | engagement statements/pdf, print |
| Templates | Engagement templates (CPA Canada style) | partial | engagement templates; no licensed CaseView content |
| Analytics | Materiality calculation | missing | No materiality route |
| Tax | T2 Schedule 1 / GIFI export to tax | present | engagement t2-schedule1, gifi |
| Collaboration | SmartSync multi-user sync | missing | External/licensed |
| Reporting | iXBRL | missing | Not implemented |
| Import | Caseware file import | partial | scripts/read-caseware-engagement.py |

## DocuSign

| Module | Capability | Status | Evidence |
|---|---|---|---|
| Envelopes | Send, sequential routing, decline, void | present | src/signatures/esign.rs |
| Auth | Email OTP signer authentication | present | esign.rs OTP |
| Signing | Consent, adopt signature | present | esign.rs consent/adopt |
| Bulk | Bulk send / approve | present | esign.rs bulk-send |
| Reminders | Reminders & expiry | present | esign.rs sweep |
| Evidence | Certificate of completion | present | esign.rs certificate PDF |
| Delivery | Actual email delivery | behaves differently | Outbox only (client contact disabled by policy) |
| Templates | Reusable envelope templates | partial | Firm templates; no tab-position template library |
| Fields | Drag-drop tab placement | partial | Fixed fields |
| PowerForms | Self-service PowerForms | missing | Not implemented |
| API | DocuSign API/Connect import | partial | import_parsers docusign; OAuth external |
| Notary | Remote notary / ID verification | missing | External |

## DT Max

| Module | Capability | Status | Evidence |
|---|---|---|---|
| T1 | T1 return prep + schedules | present | src/tax T1 schedules |
| T2/T5013 | T2 schedules, T5013 CCA | partial | src/tax T2/T5013 CCA; schedule coverage partial |
| Slips | Slip entry | present | src/tax slips |
| Family | Family linking + optimization | present | src/tax families/optimize |
| Carryforward | Rollforward / prior-year lines | present | src/tax rollforward, prior-lines |
| Review | Parallel compare / diagnostics | partial | src/tax parallel-compare |
| Authorization | T183 | present | src/tax t183 |
| Filing | EFILE / NETFILE | missing | Filing disabled by design; CRA certification external |
| AFR | Auto-fill my return | missing | CRA RepCentre access external |
| Import | DT Max data import | partial | scripts/read-dtmax-export.py; office export external |
| Billing | Client billing (Stripe) | partial | Stripe test mode only |

## WebTax

| Module | Capability | Status | Evidence |
|---|---|---|---|
| Corp info | Identification / corp info | present | T2 intake + docs/WEBTAX_RECONSTRUCTION.md |
| GIFI | S100/S101/S125/S141 | partial | engagement gifi; S141 partial |
| Provinces | Provincial schedules | partial | Partial provincial coverage |
| Calc | Calculation + results | partial | src/tax T2 calc |
| Output | Output / print | partial | import_parsers webtax_cor; print partial |
| Years | 2004–2027 tax years | behaves differently | Current-year rules only |

## Ranked backlog

| # | App | Item | Status | PR |
|---|---|---|---|---|
| 1 | QuickBooks | Cash flow, P&L by month/comparison, customer/vendor balances | in progress | https://github.com/Stiggtechnologies/nxt-os/pull/161 |
| 2 | QuickBooks | Bank rules management routes + UI | not started |  |
| 3 | QuickBooks | Recurring/memorized transaction templates | not started |  |
| 4 | QuickBooks | Credit memo / sales receipt DocKinds | not started |  |
| 5 | Caseware | Typed AJE worksheet (AJE/RJE/PAJE) with TB impact | not started |  |
| 6 | Caseware | Materiality calculator | not started |  |
| 7 | QuickBooks | Customer statements (internal PDF, no send) | not started |  |
| 8 | QuickBooks | Class/department tagging on ledger lines + reports | not started |  |
| 9 | Firm | Time/WIP/billing module | not started |  |
| 10 | QuickBooks | Budget vs actual report | not started |  |
| 11 | DocuSign | Envelope template library with tab positions | not started |  |
| 12 | WebTax | Multi-year rule sets | not started |  |
| 13 | QuickBooks | Multicurrency | not started |  |

## External dependencies (cannot close in code)

- CRA EFILE/NETFILE certification and AFR (RepCentre)
- DT Max office data export files
- DocuSign API account (OAuth, Connect, PowerForms, Notary)
- Caseware licensing (CaseView content, SmartSync, iXBRL)
- QuickBooks Online OAuth connection + QBD IIF/company samples
- Payroll provider exports
- Firm provider credentials / Excel add-in
- WebTax credential
- TaxCycle sample + golden files
