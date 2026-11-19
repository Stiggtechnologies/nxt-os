# QuickBooks Online report coverage

NXT Ledger produces seven native reports. QuickBooks Online exposes **84 report
identifiers** plus **15 payroll report routes** in its Standard reports list. This note
records the real catalogue and scores it honestly, so a migrating bookkeeper can see what
carries over before opening a single client.

Evidence: [the report catalogue](evidence/qbo-standard-reports-2026-11-18.json), observed
read-only on 2026-11-18 from `qbo.intuit.com/app/standardreports` (REA evidence
`ev_77b9930a45b975e8495ad8b3605ea7b86f5adf1218cc65c425eca9d1beb6b832`). Identifiers only —
no company name, period, amount or row is retained.

## What maps, and how closely

| QuickBooks report | NXT native report | Limit |
| --- | --- | --- |
| `PANDL` | Profit and loss | one period layout; no comparative, quarterly, by-customer, by-tag or %-of-income variant |
| `BAL_SHEET` | Balance sheet | one layout with retained-earnings roll-forward; no comparative, summary or detail variant |
| `TRIAL_BAL` | Trial balance | no adjusted-trial-balance variant |
| `AR_AGING` | AR aging | summary only, no detail variant |
| `AP_AGING` | AP aging | summary only, no detail variant |
| `GEN_LEDGER` | General ledger | one account at a time; `TX_DET_BY_ACCT` is not reproduced |
| — | GST/HST working paper | NXT-only; CRA lines 101/105/108/109 |

Six of 84 identifiers have a close native equivalent, and each one is narrower than the
source. Everything else — every payroll report, the inventory reports, the customer and
vendor contact/phone lists, `T4A`, `T5018`, cash-flow detail, unbilled charges and time,
custom and memorised reports — has **no native equivalent**.

## Why the payroll gap is not really a report gap

Fifteen payroll report routes exist because QuickBooks computes payroll. NXT does not: the
company book's employee list is a parallel list, and payroll calculation, remittance and
T4s are explicitly disabled. A payroll report without a payroll engine would be a
fabrication, so these stay absent until the engine exists — a server-side decision, not a
UI one.

## Where this is visible in the product

The same table renders inside NXT under **Migrations → Source parity**, read from
`public/apps/migrations/qbo-report-coverage.json`, so a preparer can see which reports
carry over without opening a document.

## Reading this correctly

- A native report is **original NXT output** computed from this book. It is not a
  QuickBooks report and proves no equivalence.
- The catalogue is one company's Standard reports list on one day. QBO varies it by
  country, plan and enabled features — this is an observation, not the complete vendor
  inventory.
- No report was run, exported or compared value-for-value. Paired source-versus-native
  figures remain the open gate in
  [the replication audit](REPLICATION_COMPARISON_AUDIT.md).
