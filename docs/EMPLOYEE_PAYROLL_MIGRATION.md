# Employee and payroll migration

Every company requires its own employee and payroll coverage assessment. A company label, accounting journal or exported employee contact list does not establish that its payroll has migrated. Unknown source counts remain unknown. Source systems remain authoritative until a separately executed cutover satisfies the complete declared scope.

## Preserved employee contacts

The source-list API accepts only the observed `Employees.xlsx` contact export with Employee, Phone Numbers, Email and Address fields. It preserves source cell presence, blank values and formula text, original workbook/worksheet/row references, and archive-row aliases. It does not create operational employees or infer payroll identities. A zero-row export does not establish that a company has no employees.

An immutable capture is published through a company-specific compare-and-exchange after its original archive, published source snapshot and actual server canonical manifest hash are bound. A separately pinned operator equality proof identifies the original workbook comparison. The server validates this binding; it does not claim to decode Excel itself. Reads reject corrupt, missing or cross-company references and use byte-bounded continuation pages.

`EmployeeDetails.xls` and other HR/tax workbooks are excluded from this contact lane. Sensitive payroll and HR records require individual sign-in, verified company permissions and separate access auditing.

## Required coverage for each company

1. Actual payroll provider, jurisdictions, subscription and source-access coverage.
2. Operational employee profiles, active/inactive history and source identities.
3. Payroll runs, pay periods, dates, statuses and source identifiers.
4. Pay history, paycheques, paystubs and separately classified historical balances.
5. Earnings, hours, rates and payroll items with effective dates.
6. Employee deductions and tax withholding.
7. Employer contributions and benefits.
8. Leave policies, accruals, usage, balances and time records.
9. Remittances, settlements and outstanding payroll liabilities.
10. Payroll payment methods, statuses and accounting links.
11. Existing T4, T4A, RL-1 and summaries, including amendments and acknowledgments.
12. Existing ROEs and employment documents.
13. Supporting files, source audit history and retention/access metadata.
14. Independent reconciliation across employees, periods, runs, items, liabilities, payments, tax forms and the general ledger.

Each category remains pending until its actual source scope, preservation, normalized import and comparison are verified. Some records may be held by another provider or government portal; their absence from a QuickBooks report cannot close the category.

## Historical source interpretation

Preserve original standalone payroll exports separately from the accounting-report ZIP. A row alias derived from a source file, sheet and row is a provenance reference, not a durable employee or payrun identity. Do not infer missing identifiers, currency or amounts.

Paycheque lists, detailed payroll reports and accounting journals can describe overlapping populations. Retain source rows and explicit comparison rules without silently merging them. Historical-check summaries must not be added again when their detailed rows already form the reported total. Likewise, retain both a tax-liability parent and its components while preventing their double counting.

Blank, missing, unavailable and actual numeric zero remain distinct. Source signs, original numeric representations, styles and raw cells remain available for comparison. A normalized cents interpretation requires an explicit validated policy; it must not silently round a source discrepancy away.

The historical import must not initiate payroll, send money, remit taxes, create tax filings or post the same payroll accounting entries again. Source acquisition, historical-record import, operational payroll readiness and final cutover have separate statuses.
