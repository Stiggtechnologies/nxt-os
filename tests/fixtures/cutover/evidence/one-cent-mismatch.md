# Offline cutover rehearsal

Controls: **BLOCKED**. Live cutover allowed: **false**.

Cohort: `synthetic-company-001`. As of: 2025-12-31. Synthetic data.

Environment binding: `synthetic-legacy-readonly` → `synthetic-nxt-rehearsal`.

No production writes or network requests were performed. External signed cutover authorization is not evaluated by this tool. Passing controls never authorizes a live cutover.

| Entity mapping | Source count | Target count | Mapped IDs | Matching records |
| --- | ---: | ---: | ---: | ---: |
| accounts → accounts | 2 | 2 | 2 | 2 |
| contacts → contacts | 1 | 1 | 1 | 1 |
| journal_entries → journal_entries | 1 | 1 | 1 | 1 |
| journal_lines → journal_lines | 2 | 2 | 2 | 1 |

## Accounting controls

Source debits/credits: 10000 / 10000 CAD cents.

Target debits/credits: 10001 / 10000 CAD cents.

Per-entity monetary sums and per-account controls are retained in the JSON report.

## Blockers (6)

- **field_mismatch** at `journal_lines.l-1.debit_cents`: Target field differs from the explicitly transformed source
- **money_control_mismatch** at `journal_lines.debit_cents`: Transformed source and target monetary control totals differ
- **unbalanced_or_empty_journal** at `target.journal_entries.je_1`: Each journal entry must have posted lines with equal, nonzero debit and credit totals
- **unbalanced_gl** at `target`: GL aggregate debit and credit controls do not balance
- **gl_control_total_mismatch** at `ledger`: Source and target GL debit/credit controls differ
- **account_gl_control_mismatch** at `ledger.account.a-cash`: Mapped account debit/credit controls differ

## Provenance

- source: SHA-256 `076e30784927d9b1db5a0737db278d1edca0fc77f546d2f200d0d65234cdd386`; 905 bytes; raw-file-bytes
- target: SHA-256 `f8131ae008cb017a9a3dc1f0a0a8a6303409d04f0c1e9a316a524fc109a5abed`; 795 bytes; raw-file-bytes
- mapping: SHA-256 `e70b3d5f0b1476d74cc4ee5de5c3c42ce52749d6f1210b3404200fa45fc74442`; 2747 bytes; raw-file-bytes
- Report SHA-256: `898029a18bf3ae380bf560bc6373930ae8ba22f33f577b6999f94228eed139a8` (canonical report before this digest field).

## Evidence limits

- Supplied control manifests are not native QuickBooks, WebTax or DT Max exports/imports.
- Input hashes prove the bytes compared, not export completeness, source authenticity or tenant authorization.
- Only explicit one-to-one ID/scalar mappings and supplied accounting controls are compared; attachments, source-system semantics and operational readiness require separate evidence.
- Live approval verification, data loading, dual-write/freeze, rollback and production promotion are outside this offline tool.
