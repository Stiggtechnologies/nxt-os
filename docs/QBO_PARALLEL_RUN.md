# QuickBooks Online parallel run

QuickBooks Online (QBO) stays the **book of record** for every imported QBO company until Orville approves cutover. NXT OS runs alongside it.

## Which books are in the parallel run

Every active (not archived) QBO company book whose committed head has **no review hold** (`head.review` is empty). Books with a `needs_review` hold stay out until their held items are fixed in the software. The live API already marks every book with `controls.parallel_test = true`, `controls.source_authoritative = true` and `controls.live_cutover_allowed = false`; the reconciliation fails a book if any of those change.

The per-company list is written by the reconciliation run (`parallel-run-list-<stamp>.csv`). It contains client names, so it is kept on the operator machine and never committed to this public repository.

## Weekly reconciliation (read-only)

```
NXT_COMPANY_BOOKS_KEY=... python3 scripts/qbo-parallel-reconcile.py \
  --qbo-dir <folder with QBO "Trial Balance" .xlsx exports> \
  [--archive-dir <folder with QBO export .zip archives>] \
  --out <reports folder> [--scope clean|all] [--workers 4]
```

The script only issues `GET` requests (`/api/company-books`, `/api/company-books/{id}/trial-balance`) and reads local export files. It never writes to NXT OS.

For each book it:

1. Finds the QBO trial-balance export whose SHA-256 equals the book's recorded `source.trial_balance_sha256` (a loose `.xlsx`, or the trial-balance member of an export `.zip`).
2. Parses that export independently with `inspect-qbo-export.py` (falling back to the lenient `qbo-needs-review.py` reader when the strict reader rejects a cell format) and compares every report row with the NXT OS live balance for that account.
3. Checks QBO's TB total against the sum of NXT OS live balances, that derived-zero accounts are still zero, that no NXT-native entries were posted (QBO is the book of record), the server's own `combined_balances_match_source` flag, and the parallel-run controls.
4. When a newer same-company `..._Trial Balance_Dec31_<year>.xlsx` export is present it reports whether its total differs from the loaded one (`drift_since_load`), which means QBO activity has happened since load.

Outputs: `recon-<stamp>.md` (summary), `.csv`, `.json`, and `parallel-run-list-<stamp>.csv`. Times are America/Edmonton (MT).

A `MATCH` proves NXT OS still equals the QBO export it was loaded from. To prove NXT OS equals QBO **today**, download fresh QBO trial-balance exports into `--qbo-dir` first; any drift means QBO has moved on and the book needs a reload before cutover.
