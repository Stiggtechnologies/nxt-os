# Parallel preservation imports

Old systems stay the source of truth. NXT stores a read-only copy and can compare that copy with totals the operator types in from the old system. Nothing in this API writes back to QuickBooks, Caseware, WebTax, DocuSign, or DT Max. Filing stays off. `decommission_allowed` is always false. Orville is the only person who can allow a source to be turned off, and this software cannot record that decision.

A scheduler may `POST` the parallel report again with the same JSON. Identical legacy totals return the stored report. A different legacy total stores another report. No production schedule is installed here.

## Shared batch rules

Each lane uses the DT Max checkpoint pattern:

- Open a batch. A vendor `source_kind` returns `blocked_pending_sample`. `synthetic` must be true. `filing_requested` must be false. `tax_cases_imported` must be 0.
- Stream rows in chunks of at most 1,000. `seq` is the checkpoint. A gap does not advance it. Replaying the same chunk bytes does not double-count.
- A bad row is an exception. The rest of the chunk is kept.
- Commit checks that submitted rows equal `expected_records` and that the accepted cent total equals `expected_accepted_amount_cents`.
- Recovery rereads every chunk.
- `write_back` is false. `live_cutover_allowed` is false.

The parallel body is `{ "source_records", "source_amount_cents", "as_of" }`. Those numbers are the legacy system's counts and balances. NXT does not fetch them from the old system.

Exit criteria on every report:

| Id | When it can be true | Decommissioning |
| --- | --- | --- |
| `vendor_sample_pinned` | A real export is pinned and a parser is reviewed | Still unmet |
| `counts_match` | This report's legacy count equals the NXT accepted count | One match is not enough |
| `balances_match` | This report's legacy cents equal the NXT accepted cents | One match is not enough |
| `exceptions_reviewed` | A person has reviewed the exception list | Still unmet |
| `no_write_back` | The API has no legacy write path | True, and not sufficient |
| `filing_off` | Filing stays disabled | True, and not sufficient |
| `orville_decision` | Orville records that the old system may be turned off | Always unmet here |

## QuickBooks documents

Format `nxt-quickbooks-preservation-batch/v1`, `source_kind` `declared_documents`. Any other kind, including a report ZIP or an API pull, is blocked. The journal and trial-balance path on main is unchanged and is not this contract.

| record_type | Amount | Content hash |
| --- | --- | --- |
| `invoice`, `bill`, `payment`, `payroll_item` | `amount_state: cents` and a canonical cent string | Not accepted |
| `attachment` | `amount_state: absent` | Required SHA-256 |

No invoice, bill, payment, payroll, or attachment export is pinned. Employee contact workbooks are not payroll. A journal memo is not an attachment.

6,000 accepted rows (5,996 invoices, one bill, one payment, one payroll item, one attachment) plus 3 exceptions reconciled in **350 ms**. Accepted cents: 599,900. In-memory, one process, not production.

## Caseware engagement files

Format `nxt-caseware-preservation-batch/v1`, `source_kind` `declared_files`. Record type `engagement_file` only. Amount must be absent. Content hash is required. Caseware packages, and native `nxt-engagement-tb/v1`, are not this file. No Caseware export is pinned.

6,000 accepted file rows plus 3 exceptions reconciled in **350 ms**. Accepted cents: 0.

## WebTax

Format `nxt-webtax-preservation-batch/v1`, `source_kind` `declared_archive`. Record type `archive_member`. Amount absent. Content hash required. No WebTax database, backup, or restore file is pinned. Rows are not decoded and are not a T2 return.

6,000 accepted archive rows plus 3 exceptions reconciled in **352 ms**. Accepted cents: 0.

## DocuSign archive

Format `nxt-docusign-preservation-batch/v1`, `source_kind` `declared_archive`. Record types `signed_document` and `audit_trail` are both required. Amount absent. Content hash required. Connect payloads and envelope APIs are refused. This lane does not send or sign.

6,000 accepted rows (signed documents and audit-trail records) plus 3 exceptions reconciled in **358 ms**. Accepted cents: 0.

## Firm

A legacy source exists: the QuickBooks firm screens for projects, workflows, and closes. No export file was captured, so `source_kind` other than `declared_firm` is blocked.

Format `nxt-firm-preservation-batch/v1`. Record types `project`, `workflow`, and `close` are all required. Amounts are absent. The copy does not create Firm projects.

6,000 accepted rows plus 3 exceptions reconciled in **295 ms**. Accepted cents: 0.

## Referrals

No legacy export was found. There is no referrals importer.
