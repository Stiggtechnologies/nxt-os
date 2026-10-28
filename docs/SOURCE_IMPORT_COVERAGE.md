# Source import coverage

DT Max is the bulk lane. The other rows say what already exists and what is still blocked on a real sample. None of these set `live_cutover_allowed`. Filing stays off.

| Module | Source | Importer status | Blockers |
| --- | --- | --- | --- |
| NXT Ledger | QuickBooks trial balance and journal | Implemented for the observed All Dates report profile (`scripts/stage-qbo-journal.py`, company-books operator) | Not a full backup. Label-hash account ids are not QuickBooks ids. |
| NXT Ledger | QuickBooks invoices, bills, payments, attachments | No vendor sample | Export columns are not in the repo. Do not infer them from a journal memo. |
| NXT Ledger | QuickBooks or other payroll | Contact list only (`Employees.xlsx` preservation). No payroll importer | `docs/EMPLOYEE_PAYROLL_MIGRATION.md`. A contact row is not a paycheque. |
| Firm Operations | QuickBooks firm UI | No export file | Projects, workflows, and closes were seen as screens, not as a file contract. |
| NXT Engagements | Caseware | Native `nxt-engagement-tb/v1` only | Caseware schema was not verified. That native package is not a Caseware file. |
| Prepare signatures | DocuSign | No export | No envelope export or Connect payload is pinned. This module does not send or sign. |
| NXT Tax | DT Max | Bulk preservation of declared accounts. Vendor files refused | No account export is pinned. Orville's 5,800-account figure is not a counted file here. 6,000 synthetic rows reconciled in 300 ms. See `docs/DTMAX_EXPORT_CONTRACT.md`. |
| NXT Tax | WebTaxT2 | Opaque preservation only | No WebTax export or restore file is established. |
| NXT Referrals | None found | No importer | Referrals were built in this repository. No cloned export was found. |

`GET /api/source-imports/coverage` returns the same lane list. Only `dtmax_tax` has the checkpoint API.
