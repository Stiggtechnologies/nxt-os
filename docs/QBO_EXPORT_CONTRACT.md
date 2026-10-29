# QuickBooks financial export assessment

`scripts/inspect-qbo-export.py` assesses an authorized local QuickBooks report ZIP without opening source software, running Excel, importing records or making network requests. It adds an observed source-format and exact-cent reconciliation control to migration preparation. It does not close migration, source-completeness, importer, identity or live-cutover gates.

Keep each company's acquisition and evidence in a separate private directory outside Git. Reports can contain account labels, balances, transaction date spans and report-row locators. Source ZIPs and resulting JSON must not be committed, uploaded to public CI or copied to a new destination without the applicable authorization. The operator must secure the parent directory's ACL before use; Windows outputs inherit that ACL.

## Run

```powershell
python scripts/inspect-qbo-export.py --source C:/private-pilot/acquisition/source-reports.zip --out C:/private-pilot/financial-assessment-001
```

The output directory must be new and its parent must exist. The CLI writes `assessment.json`, prints only status/counts, and never replaces inputs or existing evidence. Exit 0 means the precisely scoped financial controls below passed; exit 2 means blocked. **Both retain `live_cutover_allowed:false`.** A passing report is not permission to load data or change the system of record.

The tool opens only XLSX members whose simple report stems normalize to `Trial_balance`, `Journal`, `General_ledger`, `Balance_sheet` or `Profit_and_loss`. Trial balance and journal are required. Customer, employee and supplier list contents are not decompressed or inspected. The outer ZIP's bytes and directory metadata are checked for identity/safety; this does not authorize use of other source contents.

## Supported observed profile

Each selected XLSX has one visible worksheet, a company heading in A1, the expected report title in A2 and a nonempty period label in A3. Company-heading hashes must agree across selected reports. Trial balance and journal must both say `All Dates` for their cross-report reconciliation. Equal headings bind the supplied reports to each other; they do not authenticate a legal company or prove the reports came from QuickBooks.

| Report | Assessment |
| --- | --- |
| Trial balance | Row 5 has B `Debit`, C `Credit`. Subsequent rows carry an exact text account label in A and at least one amount in B/C. Duplicate account labels or both nonzero sides fail. A `TOTAL` row must contain both sides. Independent account sums must match the reported total, and debit/credit totals must balance exactly. Text-only footer rows may follow; financial rows after total fail. |
| Journal | Row 5 has B date, C transaction type, D number, E name, F memo/description, G account, H debit, I credit. A dated first line or header opens a group, account/amount continuation lines follow, then an unlabeled H/I subtotal closes it. Blank spacer rows can occur inside a group and do not close it. A header alone is not assigned a monetary value. Every group requires at least two valued account lines, exact subtotal agreement and equal debits/credits. An omitted zero subtotal side is accepted only when independent line sums and balance prove it is zero. A final `TOTAL` must match independent group sums. No name, memo or supplied transaction number is treated as a durable source ID. |
| General ledger, balance sheet, profit and loss | Raw XLSX hashes/sizes, sheet/cell/row/formula inventory, title, period and company-heading consistency only. Full balance-sheet structure, profit/loss arithmetic, GL opening/closing/period behavior and paired target outputs remain unassessed. |

Trial balance and journal are then compared by exact account label and net debit balance for all dates. A globally balanced misposting still blocks at account level. Zero-net journal labels omitted from the trial balance are counted explicitly; this is not a full chart-of-accounts inventory. No fuzzy label matching, inferred ID mapping or account merging is performed.

An account row with neither amount cell, or a group without an explicit subtotal before the next dated group/final total/end of sheet, blocks with `source_control_missing_amount_or_subtotal` and private row/group locators. It is never inferred to be zero, voided or non-posting from a memo or surrounding balance. The partial report retains structural inventory and lists account rows without amount cells and dated header-only rows, allowing an authorized reviewer to locate the source record. No journal or cross-report acceptance is emitted until that source-control issue is resolved.

The source currency is **not independently established** by this report profile. The report explicitly records CAD as an assumption and `source_currency_verified:false`; verify currency separately before mapping/import approval. An `All Dates` control match also does not establish a selected fiscal-year cutoff, source freeze or approved opening-versus-history strategy.

## Amount and formula rules

Amounts are interpreted with Python `Decimal` and converted to exact integer cents, serialized as strings. No binary floating point or rounding-to-fit is used. Fractional cents, errors, unsupported numeric tokens and amounts beyond the native safe monetary range block assessment.

The observed export can encode ordinary amounts as formulas such as `<f>100.01</f>` while the cached `<v>` remains zero. Reading cached values alone would lose balances. This tool compiles a small arithmetic grammar into an iterative instruction list:

- decimal literals, local cell references and same-column `SUM(B6:B20)` ranges;
- addition, subtraction, unary signs and parentheses;
- no multiplication/division, external references, arbitrary functions, macros, shared/array formulas or external execution.
- formulas are capped at 4,096 characters. The single exception is a QBO subtotal written as a pure same-column chain of backward references (`I10+I11+...`, no literals, functions, minus signs, other columns or references at/below the cell), which is compiled by the same restricted interpreter up to Excel's own 8,192-character limit and disclosed as a `long_reference_chain_formula` warning. Any other over-long formula still blocks with `formula_length_limit`.

Formula values are calculated only with this restricted interpreter. Trial-balance results retain the counts/locations of differing or missing numeric caches and the categories evaluated. A cache difference is disclosed; the supported formula expression supplies the value. GL/BS/P&L formulas receive syntax validation only and are not evaluated as accounting statements.

Dates support ISO text, ordinary Excel serial dates with the workbook's 1900/1904 epoch, and slash dates. Slash-date order is inferred only from unambiguous source dates. Entirely ambiguous slash dates block unless an operator supplies `--date-order dmy` or `--date-order mdy`; this choice is recorded as operator-supplied. Contradictory dates/order fail. Date-label conventions still require independent source review.

The current QBO Journal layout is accepted with or without its `Adj` column. Without it the adjusting flag is recorded as unknown (`null`), never inferred. A stored trial-balance amount written as a binary float's shortest representation (15-17 significant digits within 1e-4 cent of a whole cent, e.g. `2534.4399999999987`) is read as that cent with a located `stored_binary_float_noise` warning; a genuine fractional cent such as `1.005` still blocks. A stored control total in E notation (e.g. `3.657448561999983E7`) is compared to the exact line sum under the existing one-cent control tolerance and is never used as a posted amount.

## Safety and evidence bounds

The source ZIP is capped at 64 MiB. ZIPs have at most 512 entries, each expanded member at most 32 MiB, total declared expansion at most 128 MiB and expansion ratio at most 200. Duplicate/case-alias paths, traversal, linked members, encryption and unsupported compression fail. XLSX/XML is read as data only; nothing is extracted to executable paths. The XML profile is UTF-8 only and rejects DTD/entity declarations, duplicate cell value/formula children, mixed representations and error cells.

Each worksheet is capped at 250,000 rows and 2,500,000 cell slots (a ten-year All Dates journal of an operating firm exceeds 100,000 rows). Formulas are capped at 4,096 characters, 2,048 tokens and 256 parenthesis levels; cell dependencies are capped at 64 and aggregate evaluation work at 1,000,000 visits/instructions. The explicit operator-stack limit accommodates the observed deeply nested local-addition totals without recursive parsing; independent tests accept a 203-level addition chain and reject 257 levels. The character/token/work/dependency budgets remain separate limits. Invalid/circular/over-budget expressions fail instead of falling back to cached values.

The source file is opened read-only. Descriptor/path identity, size and modification time are compared around the read; change time is compared before/after within each API because Windows path and descriptor APIs can expose different change-time semantics. These checks require a frozen acquisition and trusted host; they are not an adversarial filesystem snapshot or proof of a common source cutoff across reports.

The JSON pins the exact outer ZIP and selected XLSX hashes, source byte counts, raw local inspector-script hash at module load and a canonical report digest. Preserve the trusted inspector file with its evidence; its local line endings are part of that script hash. Exact label hashes plus worksheet rows provide reproducible locators within those hashed files. **The reports do not provide durable source account or transaction IDs**, a complete native export/import contract, complete archives/attachments, open-item reconciliation, approvals, recovery or tenant isolation. No production data is written.

## Validation

```powershell
python -m unittest discover -s tests -p test_qbo_export.py -v
```

Synthetic cases cover stale zero caches, exact cents, deeply parenthesized arithmetic, unsupported/external formulas, fractional cents, one-cent TB and individual-journal errors, blank spacers/header-only rows, missing amounts/subtotals with private locators, balanced misposting, duplicate/path/budget ZIP rejection, untouched nonfinancial contents, ambiguous dates, UTF-16/32 DTD rejection, duplicate/mixed cell values, deep dependency/work budgets, selected-company mismatch, non-overwriting evidence and Windows path/descriptor change-time behavior. Synthetic tests establish the exercised parser/control behavior; actual pilot reports stay private and do not turn source parity or migration acceptance into a pass.

Use [MIGRATION_PREFLIGHT.md](MIGRATION_PREFLIGHT.md) to pin the pilot charter and declared export/archive inventory; use [CUTOVER_PLAN.md](CUTOVER_PLAN.md) for the remaining operational gates.

`scripts/stage-qbo-journal.py` can turn a ZIP that passes this assessment into the company-book staging the operator already accepts. Account ids are label-hash prefixes, not QuickBooks ids. `--synthetic-evidence` is a fixture assertion only. The stager does not post, and it leaves `live_cutover_allowed` false.

**All Dates journal with an as-of trial balance.** A full-history (`All Dates`) Journal may be paired with a Trial Balance `As of <date>` (full or three-letter month names). The journal must not be dated after the as-of date (`journal_date_outside_period`). Balances are reconciled from an empty opening; prior fiscal years closed by QuickBooks into retained earnings are accepted only when every differing account fits exactly at one fiscal start month. Staging refuses such a close (`year_end_close_without_prior_not_staged`) because it has no source entries to post; stage per year with `--prior-source` instead.
