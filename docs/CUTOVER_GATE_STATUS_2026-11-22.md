# Cutover gate status — 2026-11-22

Read-only audit of the gates that keep live financial cutover at **NO-GO**, run
against this checkout (`main` @ `25329e2`). Three independent agents audited
authentication/isolation, the migration importer, and backup/rehearsal evidence;
their findings were spot-checked against the files cited below.

Nothing here was executed against production, the private backup repository, the
age identity, or any client data. This document records status, not approval.

## Summary

| Gate | Verdict | Basis |
| --- | --- | --- |
| Individual authentication | **VERIFIED, two operational prerequisites** | per-person accounts, PBKDF2, TOTP MFA, lockout, roles and segregation tested in `tests/staff_identity.rs` |
| Company/client isolation | **VERIFIED at the API layer for client-keyed modules** | `scope_guard` + `filter_list` across ledger/tax/firm/signatures/books/migrations; not a database-tenancy guarantee |
| Validated migration importer | **NOT MET** | staging lanes only; no full-ledger importer; no approved account map |
| Rehearsal 1 (authorized, de-identified) | **NOT MET** | G3-01 blocked in the checklist; only a synthetic loopback pass exists |
| Rehearsal 2 (clean repeat with measured recovery) | **NOT MET** | G3-04 blocked; zero rehearsals exist |
| Backup drill | **PARTIAL** | pre-encryption isolated restore recorded; no second restore from ciphertext, no scheduled-trigger observation, no notification drill |
| Off-device key custody | **NOT MET** | `independent_off_device_key_custody_verified: false` in release evidence; DPAPI-local copies only |
| Cutover flag (`live_cutover_allowed`) | **GUARDED** | every shipped response hard-codes `false`; `tests/cutover_guard.test.mjs` fails the build if any shipped constant enables it |

## Individual authentication — VERIFIED (with prerequisites)

`src/firm/staff.rs` implements per-person accounts: PBKDF2 password hashing
(`pbkdf2`), TOTP MFA (`totp_at`/`totp_match`), sessions, temporary-password
change gating, login lockout, five roles, per-client assignment with `*`
reserved for admins, denial auditing, and a break-glass shared key whose writes
are audited.

`tests/staff_identity.rs::staff_accounts_roles_mfa_scoping_and_break_glass`
proves: wrong credentials 401, MFA required at login, role policy 403s, preparer
sign-off stamped from the verified account (a typed name is replaced), reviewer
separation of duties 409, list filtering, cross-client record 403, firm-summary
403, lockout 429, logout 401, and that `NXT_BREAK_GLASS=off` makes the shared
key 401. `tests/staff_scoping.rs` extends the same proof to signatures, firm
projects, company books and cron authentication.

Two prerequisites are configuration, not code: `NXT_BREAK_GLASS=off` (the
shared key is documented as "kept for migration") and `NXT_STAFF_REQUIRE_MFA`
left at its default on. Neither is asserted by a test against a deployed
environment.

## Company/client isolation — VERIFIED at the API layer

`scope_guard`/`scope_guard_reserved` (`src/firm/staff.rs`) is applied to the
company Ledger (`/api/company-ledger`), tax returns, engagements, firm projects
and closes, signatures, migrations, company books and the client directory.
Scoped users get filtered list responses (`filter_list` retains only records
whose `client_ref` they hold and drops firm-wide audit counters), 403 on other
clients' records, 403 on firm-wide views, and write-time checks that refuse
moving a record to a client outside their scope (`body_client`). The test above
also proves a scoped user cannot read another client's company book.

This is application-layer isolation over one store namespace, not database-level
multi-tenancy. The firm's own Ledger is intentionally a single workspace. The
gate should therefore be recorded as closed for per-client API isolation and
open for verified database tenancy/RLS, which is a separate decision.

## Migration importer — NOT MET

There is no validated importer for real financial data and no full-ledger
importer into the native Ledger. Three bounded lanes exist:

1. **Company-books journal/TB staging** (`src/company_books/`,
   `scripts/stage-qbo-journal.py`, `scripts/company-books-operator.py`) commits to
   a separate immutable company-books store and never posts to the Ledger
   (`posts_to_ledger: false`; `src/company_books/README.md`). Preview → CAS
   commit, exact-retry replay, one full-history load only, and journal↔TB
   controls must agree before commit.
2. **Opening balances** (`src/ledger/opening.rs`) posts one dated
   `opening_balance` entry plus the imported chart, requires an empty Ledger and
   a committed head, and claims once. It records `tied_out` and
   `accounts_not_tied` in the receipt but does **not** fail the import when
   accounts do not tie (`opening.rs:188`).
3. **Parallel review snapshots** (`src/migrations/`) preserve exact read-only
   ZIPs; `src/migrations/mod.rs` states "Never financial import".

Gaps: no real-data import evidence in git (only synthetic fixtures); account ids
are label hashes, not QuickBooks ids, with no approved mapping; no invoices,
bills, payments, AR/AP open items, payroll, attachments or bank reconciliation;
reviewer is a label, not a verified principal; no idempotent batch identity at
the Ledger boundary; reconciliation is report-only. `scripts/migration-preflight.mjs`
validates intake manifests only and always reports
`live_cutover_allowed:false, production_writes:0`.

Evidence: `docs/MIGRATION_PREFLIGHT.md`, `docs/MIGRATION_REHEARSAL.md`
(G1-02 partial, QBO-02 blocked, G3 blocked), `docs/MIGRATION_PRODUCTION_STEPS.md`,
`docs/CUTOVER_PLAN.md`, `docs/PARALLEL_RELEASE_EVIDENCE.json`
(`financial_imports: 0`).

## Rehearsals — NOT MET

`docs/CUTOVER_CHECKLIST.csv` records 64 gates: 13 completed, 31 blocked,
15 not started, 2 partial, plus three single pending states; the `reviewer`
column is empty on every row. G3-01 (authorized de-identified rehearsal) and
G3-04 (clean repeat with interruption and restore) are `blocked`; the only
synthetic pass is explicitly not the authorized rehearsal. No measured
RTO/RPO exists — the only number is a 1,035 ms synthetic namespace restore
request, disclaimed in `docs/CUTOVER_PLAN.md` as not a recovery measurement.

## Backup drill — PARTIAL

`.github/workflows/nxt-backup.yml` runs in the private backup repository only
(job-level guard on `repository.private`), 03:30 UTC, uploads only
`*.tar.gz.age` and a manifest, and restores into an isolated PostgreSQL 17
before comparing row counts, canonical row hashes, columns/constraints, RPC
definitions, RLS and role privileges.

Recorded evidence: run `40265656732`, artifact `12625963167` (51,403 bytes),
ciphertext `54597593…`, dump `714baf1e…`; later pins in
`docs/RELEASE_EVIDENCE.json` reference run `40382626698`, artifact
`12679566853` (3,959,853 bytes). All four flags in that record are false:
`new_local_postgresql_restore_performed`, `scheduled_trigger_observed`,
`notification_drill_complete`, `independent_off_device_key_custody_verified`.
`docs/BACKUPS.md` states plainly that no second local restore from the
downloaded dump was performed.

Gaps: no scheduled-trigger observation, no alert-delivery drill, no second
restore from ciphertext, no loss-of-machine recovery, no functional test of
`scripts/backup-nxt-store.mjs`, and `docs/BACKUPS.md` is stale relative to the
newer evidence JSON (run ids and artifact size differ without explanation).

## Off-device key custody — NOT MET

The age identity is held only in DPAPI-bound local copies. The documentation
calls them "not portable off-device escrow" and the release evidence sets
`independent_off_device_key_custody_verified: false`. The workflow holds the
recipient only, so it cannot decrypt; decryption proof depends on a human-run
local step whose artifact is not in this repository.

## Cutover flag — GUARDED

Every shipped response still reports `live_cutover_allowed:false`. The new
`tests/cutover_guard.test.mjs` scans `src/`, `public/` and `scripts/` and fails
if any shipped literal sets it to `true` (including Python `True`). The open
draft PR #145 flips that flag in 13 files and would fail this test; it remains
a draft and must not merge until the gates above are verified.

## What a human must do next

1. Observe a scheduled backup run in the private repository; record run and
   artifact ids and retention.
2. Force a backup failure and prove the alert reaches the named operator.
3. Move the age identity to genuinely off-device escrow and decrypt an archive
   on a different machine.
4. Perform the missing second PostgreSQL 17 restore from the downloaded
   ciphertext and record durations for RPO/RTO.
5. Produce an authorized real export, an independently approved account map,
   and a fail-closed reconciliation importer; then run G3-01..G3-04 with
   preparer ≠ reviewer and fill the checklist reviewer column.
6. Only then remove the tripwire and revisit PR #145.
