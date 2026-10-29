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


## Re-score after merged work through #56 (2026-10-29, evening pass)

Same formula. Measured against main after #37–#42, #44, #45, #47–#54 and #56. #43 was closed without merging. Rows were added only where a merged PR introduced an original-product behaviour the audit had no row for. Nothing is "copied & tested" unless a test asserts it.

| Module | Previous | Now (rows: copied/partial/missing) | What moved |
|---|---|---|---|
| QuickBooks | 54.8% | **56.5%** (23: 4/18/1) | payroll .xls reader partial → copied & tested (#39; NXT Accounting totals match source: 14 rows, 6 employees); +QBD report profiles (partial, #40; vendor reports unverified); +IIF reader (partial, #41; no real sample). #37/#52/#54 strengthen existing partial rows. |
| Firm | 40.0% | **45.5%** (11: 1/8/2) | +staff users, roles, TOTP MFA and per-client access (copied & tested, #56 `staff_identity`) |
| Caseware | 44.4% | **47.4%** (19: 1/16/2) | automatic documents missing → partial (report/letter templates, PDF export, draft ASPE statements; not CaseView, #44/#48); +proposed entries and leadsheets (partial). Link documents (#54) and id-based reviewer sign-off (#56) strengthen existing partial rows. |
| DocuSign | 28.1% | **40.6%** (16: 2/9/5) | recipient signing partial → copied & tested, and completion certificate/signed PDF missing → copied & tested (native ceremony, #49); users/groups missing → partial (staff accounts and roles, #56; no groups, orgs or billing). Reminders and expiry sweep (#56) strengthen send/resend; delivery is still outbox-only. |
| DT Max | 36.4% | **41.7%** (12: 1/8/3) | +preparer/reviewer sign-off by staff id, with a completed T183 ceremony required before filed (copied & tested, #45/#47/#53/#56). The provinces row stays partial after #51 (QC TP-1 is only scoped). Slips (#45) and rate sources (#50) strengthen existing partial rows. |
| WebTax | 33.3% | **41.7%** (6: 0/5/1) | username/password/recovery missing → partial: NXT has its own per-user login, MFA and admin reset, but no WebTax account bridge |

Still not 100% anywhere. Every remaining row needs either vendor files or credentials (listed above), or the code items below.

## Remaining blockers, ranked by the 5% Canadian tax-prep GTM

1. **CRA EFILE / NETFILE transmission.** Filing stays off, and "filed" is a human record behind the T183 gate. Closing it needs a CRA EFILE registration and software certification. **External.**
2. **Vendor tie-out for T1/T2.** Real DT Max output (on Orville's PC, offline) and a WebTax credential, paired through parallel-compare. **External.**
3. **Outbound delivery.** E-sign links, OTP codes and reminders queue in the outbox and nothing is emailed. Closing it needs an email/SMS provider key and approval to send. The sweep needs a scheduler: a Vercel cron entry for `POST /api/signatures/esign/sweep`, not added. **External plus a config change.**
4. **Production staff cutover.** Create the staff accounts, enroll MFA, then set `NXT_BREAK_GLASS=off`. This is an operational step for Orville.
5. **Client scoping beyond tax and engagements.** Signatures, firm, company books and migrations still rely on role checks only. **Code.**
6. **Tax coverage.** QC TP-1 and years other than 2025 (T1); T3 slips and provincial tax; T5013 CCA. **Code, plus published forms.**
7. **QBO OAuth and payroll exports** for the remaining companies. **External.**
8. **Caseware CaseView/iXBRL/TaxCycle file and Xero.** **External: formats and accounts.**
