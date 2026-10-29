# Source import coverage

Old systems stay the source of truth. NXT holds read-only copies. `decommission_allowed` stays false until Orville decides. See [PARALLEL_IMPORT_CONTRACTS.md](PARALLEL_IMPORT_CONTRACTS.md) and [DTMAX_EXPORT_CONTRACT.md](DTMAX_EXPORT_CONTRACT.md).

| Module | Source | Importer status | Blockers |
| --- | --- | --- | --- |
| NXT Ledger | QuickBooks trial balance and journal | Observed report profile on main | Not a full backup. Not the document importer. |
| NXT Ledger | QuickBooks invoices, bills, payments, payroll, attachments | Bulk declared copy. Vendor files blocked | No real export is pinned. 6,000 rows in 350 ms. No write-back. |
| Firm Operations | QuickBooks firm UI | Bulk declared copy. Vendor files blocked | Screens were seen. No export file. 6,000 rows in 295 ms. |
| NXT Engagements | Caseware | Bulk declared file copy. Vendor files blocked | Caseware schema is not verified. 6,000 rows in 350 ms. |
| Prepare signatures | DocuSign signed document and audit trail | Bulk declared archive. Vendor files blocked | No Connect payload or envelope export. Does not send or sign. 6,000 rows in 358 ms. |
| NXT Tax | DT Max | Bulk declared accounts | Vendor backup, database, keyword file, and SDK XML are blocked. 6,000 rows in 311 ms on the re-run. |
| NXT Tax | DT Max original client files | Immutable hash-deduped attachments, client-linked, unmatched queue | Synthetic only. See [DTMAX_SOURCE_FILES.md](DTMAX_SOURCE_FILES.md). |
| NXT Tax | WebTaxT2 | Bulk declared archive members | No WebTax export is pinned. Not decoded. Not filed. 6,000 rows in 352 ms. |
| NXT Referrals | None found | No importer | No cloned export in this repository. |

`GET /api/source-imports/coverage` returns the lane list. Parallel reports are `POST /api/source-imports/{lane}/batches/{batch}/parallel`.
