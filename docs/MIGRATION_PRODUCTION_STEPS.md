# Steps still required before a production migration

Nothing in this list was run against production. Do not point these commands at `os.nxtcorporation.ca` or the hosted database until Reliability has accepted the pull request and the CI on that pull request is green. Draft #4 (GuardVoice / Nixon) is not part of this migration and stays held for Orville's explicit go.

Roles below are the ones in `docs/CUTOVER_PLAN.md`. This file does not assign a person to a role.

## 1. Merge is a human decision

Owner: Reliability, then whoever is allowed to merge to `main`.

This branch must not be merged from the agent. Required first: green GitHub Actions on the pull request (`NXT validation` and `migrations`).

## 2. Confirm the hosted database before any SQL

Owner: database operator.

Access: Supabase organization **NXT OS**, project `qreiwxyknrswiiitbyqb` (`nxt-os`, Canada Central), or a separate staging project if Reliability wants the first apply off that project. `docs/SUPABASE.md` records that `20261025051013` was already present there. That was not re-checked for this note, because this work did not log in.

```sh
supabase link --project-ref qreiwxyknrswiiitbyqb
supabase migration list
supabase db push --dry-run
```

Read the dry run. The only new file this branch adds, relative to that historical note, is `supabase/migrations/20261026033000_nxt_backup_reader.sql`. It creates the read-only role `nxt_backup_reader`. Do not run `supabase db push`, and do not run `supabase db reset`, until Reliability has accepted that dry run. Do not put the database URL in a shell argument that gets committed.

Local repeat of the contract, which does not touch that project:

```sh
# DATABASE_URL host must be localhost, 127.0.0.1, or ::1.
# The database name must not contain prod, production, or live.
DATABASE_URL='postgres://postgres:PASSWORD@127.0.0.1:5432/nxt_migration_check' \
  bash scripts/check-store-migrations.sh
```

On 2026-10-28 that script passed on local PostgreSQL 16. CI runs it on PostgreSQL 17.

## 3. Vercel production env, only after merge

Owner: release owner.

Access: the Vercel project that serves `https://os.nxtcorporation.ca`. This agent did not deploy.

After the reviewed commit is what Vercel will build, set server environment variables only:

- `SUPABASE_URL` — HTTPS origin of the project Reliability named
- `SUPABASE_SECRET_KEY` — server secret (`sb_secret_...`), or the legacy service-role key, not both as competing sources
- `NXT_STORE_NAMESPACE` — `production` for that deployment, or `preview:...` for a preview. Never from the browser
- `ADMIN_API_KEY` — already the workspace gate; do not print it

Do not put the secret in a client-visible variable. Do not import a company book as part of the deploy. Filing stays off.

## 4. A real QuickBooks company, still NO-GO until the gates pass

Owner: source administrator for the export, import owner for the staging, accounting controller for the reconciliation, business sponsor for any later GO.

Access: an authorized QuickBooks report ZIP kept outside git. Not a fixture.

```sh
python3 scripts/inspect-qbo-export.py \
  --source /private/acquisition/source-reports.zip \
  --out /private/financial-assessment-001

python3 scripts/stage-qbo-journal.py \
  --source /private/acquisition/source-reports.zip \
  --out /private/stage-001 \
  --cohort COHORT_ALIAS
```

Do not pass `--synthetic-evidence` for a real company. Write `evidence.json` from observed identity, currency, and period-lock artifacts. The stager itself does not verify those.

Then, still on loopback, with `NXT_COMPANY_BOOKS_KEY` set in the environment and not on the command line:

```sh
python3 scripts/company-books-operator.py preview \
  --origin http://127.0.0.1:PORT \
  --private-root /private/stage-001 \
  --controls-manifest controls.json --controls-sha256 CONTROLS_SHA \
  --acquisition-manifest acquisition.json \
  --company COHORT_ALIAS \
  --evidence evidence.json --evidence-sha256 EVIDENCE_SHA \
  --request-id preview-COHORT_ALIAS \
  --receipt preview.json
```

Commit and verify use the flags in `docs/COMPANY_BOOKS_OPERATOR.md`. The only non-loopback origin the operator accepts is `https://os.nxtcorporation.ca`. Do not use it until G5 in `docs/CUTOVER_CHECKLIST.csv` is actually approved. A passing preview is not that approval.

Still out of this importer: invoices, bills, payments, payroll, attachments, bank reconciliation, and a second full-history load. DT Max T1/T2/T3/T5013 and WebTax have no importer here. Do not file a return from NXT.

## 5. What remains blocked on a person or a credential

| Blocker | Access needed | Owner |
| --- | --- | --- |
| Hosted migration apply | Supabase project access | Database operator, after Reliability |
| Vercel production env and deploy | Vercel project access | Release owner, after merge |
| Real QuickBooks export | QuickBooks company access, private acquisition directory | Source administrator |
| Cohort, owners, deadlines | Private roster | Migration lead and business sponsor |
| Individual login and company isolation | Product decision plus an auth design; the shared workspace key is not it | Security and engineering |
| Off-device backup key and the first scheduled backup observation | Private backup repository and key custodian | Records owner and operations lead |
| DT Max / WebTax parity | Vendor export or a reviewed re-entry, per program | The matching technical reviewer |
| Nixon / GuardVoice | Explicit go from Orville; draft #4 is separate | Not this migration |

Until those are done, QuickBooks remains the system of record for anything this fixture did not cover, and it remains the system of record for the fixture company as well. The fixture company is synthetic.
