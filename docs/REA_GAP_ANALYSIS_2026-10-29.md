# REA replica gap analysis, 2026-10-29

This re-measures every row of `REA_REPLICA_AUDIT.md` against main at `3a0bfdd`, which includes merged PRs #34 and #35.

Score = (copied & tested + 0.5 × partial) / rows. The "after" column reflects #34 and #35, both merged. No paired vendor golden files exist yet, so "copied & tested" means a test asserts the behaviour and, where noted, real staged files were checked.

## Scores

| Module | Before (rows: copied/partial/missing) | After | Change |
|---|---|---|---|
| QuickBooks | 52.8% (18: 2/15/1) | **54.8%** (21: 3/17/1) | +3 rows: company GL view (copied & tested, real counts), journal-derived payroll view (partial), payroll .xls cell reader (partial) |
| Firm | 40.0% (10: 0/8/2) | 40.0% | no code change this pass |
| Caseware | 34.4% (16: 0/11/5) | **44.4%** (18: 1/14/3) | CaseView missing→partial, WP file format missing→partial, +.cvw OLE2 inventory (copied & tested on 130 real files), +PR/HI/TS sign-off tables (partial) |
| DocuSign | 28.1% (16: 0/9/7) | 28.1% | no change |
| DT Max | 36.4% (11: 0/8/3) | 36.4% | no change |
| WebTax | 33.3% (6: 0/4/2) | 33.3% | no change |

None of the modules is at 100%, and code alone cannot get them there. Each remaining row is listed below with what closes it.

## Ledger and payroll per company (Orville's priority)

| Company | GL shown vs source (txns / lines) | Payroll shown vs source |
|---|---|---|
| Itrade | 622 / 1552 vs 622 / 1552 | 178 txns vs 178 (2023: 14, $26,633.75; 2024: 164, $346,779.38), from payroll accounts. Source has no Paycheque txns or employee detail. |
| 1612367 | 3611 / 9901 vs 3611 / 9901 | 0 vs 0 |
| 1700363 | 412 / 1124 vs 412 / 1124 | 1 WCB charge, $202.55 |
| NXT Accounting | not on box | gross $141,090.82 / net $113,255.85 **not verified**: no QBO export or payroll .xls on the box |

Root cause: the data was already committed in full, but the UI never showed it. There was no flat GL, accounts showed as hashes, and there was no payroll view. #34 fixes that.

## Gaps and closing items

| # | Gap | Closing item | Closes by |
|---|---|---|---|
| 1 | No flat GL / payroll view | #34 (merged) | code, closed |
| 2 | Payroll register cells (gross/net/deductions/employees) | #34 reader; still needs real payroll .xls from QBO/QBD for each company | **external: payroll exports** |
| 3 | NXT Accounting books | QBO export of NXT Accounting | **external: Orville sign-in/export** |
| 4 | QBO OAuth / realm sync | OAuth app plus tokens | **external: OAuth credentials** |
| 5 | QBD report profiles (P&L, BS, AR/AP aging as source controls) | parse `source-reports.zip` qbXML reports into controls | code (next PR) |
| 6 | IIF/CSV import | IIF parser into company chunks | code, but needs a real IIF sample to claim coverage |
| 7 | GIFI import / TaxCycle file | GIFI CSV import; TaxCycle file needs the vendor format | code + **external: TaxCycle sample** |
| 8 | CaseView rendering (formulas/layout) | #35 inventories streams; rendering needs the format | **external: Caseware binaries/spec** |
| 9 | Caseware role protection, SmartSync, iXBRL, Xero | — | **external: Caseware install / Xero account** |
| 10 | DT Max keyword parsing into tax compare | parser on DT Max export files | **external: DT Max files from Orville's PC (offline)** |
| 11 | WebTax save/calc/GIFI | — | **external: WebTax credential** |
| 12 | DocuSign Connect, PowerForms, Notary, users, OAuth, certificate | ingestion behind disabled-by-default config | code for the scaffold; **external: DocuSign OAuth** to verify |
| 13 | Firm integrations / Spreadsheet Sync | — | **external: provider credentials, Excel add-in** |
| 14 | Paired vendor golden files for every row | exports from Nixon's screen | **external: Nixon exports** |

## CI steps I would add (not applied; the token lacks workflow scope)

1. `python3 -m unittest discover -s tests -p 'test_*.py'`, which covers the new payroll .xls and Caseware tests.
2. `node --test tests/*.test.mjs`
3. `cargo test --locked --test company_gl_payroll_views`, already covered by `cargo test`.
4. Install a rustls crypto provider in test setup, or pin the reqwest feature. `nixon_finance::voice_opens_records_and_falls_back_without_a_provider_key` currently fails locally on main.
