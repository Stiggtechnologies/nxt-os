# Migration rehearsal, 2026-10-28

This records the fixture run on this branch. It is not a production cutover. `live_cutover_allowed` stayed false. No command in this run used `os.nxtcorporation.ca`, the hosted Supabase project, or Vercel production.

## Duplicate store migration

Drafts #1 and #2 both carried `supabase/migrations/20261025051013_nxt_store.sql`. The git blob is the same on both branches: `1a18213c13bac5e5111590094aeb965c446c8a35`. This branch keeps that file once and keeps `supabase/migrations/20261026033000_nxt_backup_reader.sql` once. `scripts/check-store-migrations.sh` fails if a third file appears or if two migration files share a checksum.

## What the fixture run did

Command, against a loopback process with in-memory storage and `ADMIN_API_KEY=fixture-rehearsal-only`:

```sh
cargo build --locked --example dev
python3 -B scripts/rehearse-fixture-migration.py target/debug/examples/dev
```

The fixture is the synthetic QuickBooks trial balance and journal already used by `tests/test_qbo_export.py` (formula-backed $100.01, exact cents). `scripts/stage-qbo-journal.py --synthetic-evidence` built the operator manifests. The operator then previewed, committed, replayed the same commit, and verified every source row against the local server.

| Check | Result |
| --- | --- |
| Staged accounts / entries / lines | 2 / 1 / 2 |
| Trial balance and journal debits | 10001 cents both |
| Archive SHA-256 | `d692dd25df3e0ca7386c5f960d5967995872f9271069feb32adff2fc7eda8237` |
| Journal member SHA-256 | `85459ddafb489b37d5cbd2ea2048a0a72c64d364f3edcd769437e6b529d83e72` |
| Trial balance member SHA-256 | `771b25b94cccdd106270c51756a8e11924aaededc12e377f1954c1367e236b45` |
| Controls manifest SHA-256 | `2b57169adcd010c1b8a43fd33a5329fab1fc97cf1194413a1b400e2870fb38d9` |
| Preview receipt SHA-256 | `5055a253e02c77a213da4c93c9d80608da87a7a1c2e1b10e70760e3482d919a0` |
| Commit receipt SHA-256 | `e5d709df2d29df7c9b7e85f76edd65b17f78944f1de0e11056ebfff6cf8ac34a` |
| Commit replay receipt SHA-256 | same as the commit receipt |
| Verify receipt SHA-256 | `7205a600dc1b04858552197106795558a4929507d57b93760fe46cdc8550b133` |
| Verify counts | 1 entry, 2 lines, 2 accounts |
| Cutover comparator | `Controls PASS; live_cutover_allowed=false; blockers=0` |
| Cutover report SHA-256 | `786c6257069677515a4a61860cd64f809940c733191c74e9f75a8658ca859e54` |
| Cutover debit control | 10001 cents |
| Intake preflight | `Intake READY FOR REVIEW; live_cutover_allowed=false; inventory_blockers=0` |
| Preflight report SHA-256 | `203de087c3cd7218c6c8e7a5d92bc084c68821ead0ef09bc79a704d37d1d093a` |
| Production writes | 0 |

The preflight tool still marks these operational gates blocking, which is what that tool is specified to do. The fixture import does not clear them:

1. `identity_and_company_isolation`
2. `source_contract_and_completeness`
3. `required_capability_coverage`
4. `staged_import_and_reconciliation`
5. `recovery_and_key_custody`
6. `external_acceptance`

`--synthetic-evidence` wrote operator evidence whose `verified` flags are fixture assertions. The observation file says identity, currency, and the period lock were not independently authenticated. Those flags let the importer commit the fixture. They are not a QuickBooks observation.

Account ids are a prefix of the exact label hash. They are not QuickBooks account ids. The report profile is All Dates, so `full_history` means the dates present on that report, not a witnessed source freeze.

`cargo test --locked` on this machine passed 94 tests, 0 failed. `scripts/check-store-migrations.sh` applied both SQL files to local PostgreSQL 16 (`nxt_migration_check` on `127.0.0.1`) and the rolled-back `supabase/tests/store_contract.sql` passed. CI repeats that contract on PostgreSQL 17. The hosted project was not contacted.

`--allow-native-adjustments` now recomputes native debit and credit from returned entries instead of rejecting the flag. `tests/test_company_books_operator.py` covers a matching delta and a one-cent lie. The fixture rehearsal had no native adjustment, so that path was not part of the loopback post.

## Checklist: done in this branch, or still blocked

Historical rows in `docs/CUTOVER_CHECKLIST.csv` are left as the 2026-10-26 record. This table is the reading after the fixture run.

| Item | Reading | Why |
| --- | --- | --- |
| PREP-01, DEV-01, BACKUP-01–04, BACKUP-08, RELEASE-01–04, RECOVERY-01–02 | Already recorded complete in the checklist | Not re-run here. They are not a financial GO. |
| QuickBooks journal import for the assessed trial-balance and journal profile | Done for a synthetic fixture | Stage, preview, commit, identical replay, and verify passed on loopback. |
| Trial-balance reconciliation for that profile | Done for the fixture | Assessor, stager, server, and cutover comparator all agreed at 10001 cents. A one-cent fixture is rejected before staging. |
| Native-delta verification | Done in code and unit tests | The operator recomputes the delta. Not exercised against a hosted book. |
| Store SQL migrations | Done against local Postgres | Both files once. Contract rolled back. Not applied to the hosted project by this run. |
| QBO-02 real export and mapping | Blocked | No authorized company export is in git. Label-hash ids are not a source account map. Currency was not read from QuickBooks. |
| G0-01–G0-04 | Blocked | No cohort charter, named owners, volume, or retention decision is in this run. |
| G1-01 individual roles and company isolation | Blocked | The workspace key is still the only gate. |
| G1-02 staged import | Partial | The journal lane has preview, CAS commit, replay, and verify. Invoices, bills, open items, payroll, attachments, and bank reconciliation are not imported. |
| G1-03 capacity | Blocked | Bounds remain the ones in `src/company_books/README.md`. No practice-sized load was run. |
| G1-04 durable backend | Partial, historical | `docs/SUPABASE.md` records an earlier hosted check of the store migration. This rehearsal used process memory on purpose. |
| G1-05, G1-06, BACKUP-05–07 | Blocked | Scheduled backup observation, alert drill, off-device key custody, and business-write reverse delta are not done. |
| G2 source archives, T1/T2/T3/T5013, WebTax | Blocked | No DT Max or WebTax importer exists. This run did not file a return. |
| G3-01–G3-04 authorized rehearsal | Blocked | The synthetic loopback pass is not the authorized de-identified rehearsal the gate requires, and G1-01 is still open. |
| G4 shadow pilots, G5 cutover, G6 stabilization, G7 retirement | Blocked | No source freeze, no authority switch, no production financial post. |

Remaining commands, access, and owners are in [MIGRATION_PRODUCTION_STEPS.md](MIGRATION_PRODUCTION_STEPS.md).
