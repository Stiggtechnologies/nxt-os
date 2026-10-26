# Encrypted NXT Store backups

## Status and scope

The repository supplies `scripts/backup-nxt-store.mjs` and `.github/workflows/nxt-backup.yml`. Their presence does not mean a schedule, credential or successful backup exists. Provisioning the dedicated private backup repository, read-only database role/policies and repository configuration requires the approved operational setup. Record the first successful manual run and local decryption check before calling this service operational.

This is a logical backup of NXT's three private Store tables: `nxt_private.kv_values`, `set_members` and `list_values`, plus the list identity sequence. It covers **all namespaces readable by the dedicated backup role**, including stored Ledger, Firm, Tax and Referrals state. This broader read scope must be explicitly authorized. It cannot write source records or invoke the write-capable Store RPC.

The encrypted archive also contains the checked-in Store schema migration, the source RPC definition, source commit, Cargo lockfile, tool versions, integrity/restore manifest and a list of configuration names that must be recovered separately. Only `20261025051013_nxt_store.sql` is applied to the disposable recovery database. Hosted-account and backup-reader provisioning migrations are deliberately excluded. Schema/RPC drift fails validation until this recovery schema allowlist is reviewed and updated.

It does **not** back up the whole managed Supabase project: Auth users, Storage objects, other schemas, roles/passwords, project settings, Edge Functions, Vercel configuration/secrets, GitHub configuration or the age private key are excluded. This is not point-in-time recovery or a complete disaster-recovery service. See [CUTOVER_PLAN.md](CUTOVER_PLAN.md) for the remaining gates.

## Off-site location and retention

Install the workflow and backup driver in the dedicated **private** repository `Stiggtechnologies/nxt-os-backups`. The public NXT source repository is not the backup destination; its copy of the workflow skips execution because of the private-repository guard. Protect the private repository's default branch and restrict workflow/secret administration to the authorized operators.

The workflow runs at **03:30 UTC nightly**, and supports manual dispatch from its default branch. Schedules use the default branch and may be delayed or dropped by GitHub; monitor the last successful artifact rather than assuming the cron expression guarantees delivery. See [GitHub's schedule behavior](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule).

Only an age-encrypted archive and a safe ciphertext checksum manifest are uploaded. Artifact retention is **30 days**, subject to the repository/organization's lower limit. GitHub Artifacts is a second provider relative to Supabase, but it is neither immutable nor permanent: authorized repository users can delete artifacts and retention expires them. Keep required longer-term copies separately. [GitHub artifact retention documentation](https://docs.github.com/en/actions/tutorials/store-and-share-data#configuring-a-custom-artifact-retention-period) describes these limits.

No paid backup service or plan upgrade is configured by these files. Runs and artifacts consume the account's available Actions minutes/storage and database egress. Confirm the included quota and zero-paid-spend policy before enabling the schedule, and monitor growth. A quota failure is a failed backup, not a reason to assume a retained copy exists.

## Credentials and keys

The private backup repository requires:

| Setting | Location | Purpose |
| --- | --- | --- |
| `BACKUP_DATABASE_URL` | Actions repository secret | PostgreSQL connection for the dedicated read-only role with `sslmode=verify-full`. Prefer a direct/session-pooler connection suitable for a held transaction and exported snapshot. Supabase service, management and application-admin keys are not needed. |
| `BACKUP_AGE_RECIPIENT` | Actions repository variable | One native `age1…` public recipient. It can encrypt but cannot decrypt. |
| `BACKUP_CA_PEM` | Actions repository variable | Public Supabase Root 2021 CA PEM. The workflow normalizes newlines, checks its pinned SHA-256 and materializes `BACKUP_CA_FILE` for libpq. No private key is involved. |
| `NXT_SOURCE_REF` | Actions repository variable | Reviewed NXT source revision to check out. Prefer an exact 40-character commit SHA and update it with each schema/application release; unset defaults to `main`. |

The driver has no npm dependencies. It needs Node.js 22+, PostgreSQL **17** `psql`/`pg_dump`/`pg_restore`, `age`, `git` and `tar` on PATH. The workflow uses the official PostgreSQL 17 Bookworm image for the job and a separate disposable PostgreSQL 17 service, installs age/archive prerequisites and pins GitHub Actions to reviewed commit hashes. Minor container/package updates remain upstream dependencies; their observed versions are recorded inside each encrypted manifest.

Remote source connections require `verify-full` and `BACKUP_CA_FILE`; `require`, `verify-ca` and disabled certificate checking are rejected. The approved public CA was obtained from the exact project's Database Settings certificate link: `https://supabase-downloads.s3-ap-southeast-1.amazonaws.com/prod/ssl/prod-ca-2021.crt`. Normalized PEM SHA-256 is `700723581420dd1ac98fd7e9ac529f0ef210eadcaf87fc868a3ad7d114c2f3b7`; its expiry is 2031-04-26. An authorized setup check verified a live connection with this CA and both certificate/hostname validation enabled; source table writes and Store RPC execution were denied. Certificate replacement requires an independently verified new certificate and reviewed workflow pin. See [Supabase's verified SSL connection guide](https://supabase.com/docs/guides/database/psql).

The read-only role must have schema usage, SELECT on the three tables and SELECT on `nxt_private.list_values_position_seq`. Each table needs an applicable permissive SELECT RLS policy `USING (true)` and no applicable restrictive SELECT policy. The role must not own the tables, be superuser, bypass RLS, hold table write privileges or execute `public.nxt_store_command(text,jsonb)`. The script checks those effective permissions/policies before exporting. Creation/grants/password delivery are performed separately by the authorized database administrator; never commit the password.

Keep the age identity **outside GitHub, Supabase and this repository**. Generate/escrow it locally with the official age tools; publish only its recipient to Actions. Preserve a separately secured off-device copy under the user's control. Losing every identity copy makes these encrypted archives unrecoverable. The workflow never receives an identity and therefore cannot prove decryption itself. [Official age usage](https://github.com/FiloSottile/age#usage) documents recipient encryption and identity-based decryption.

## What each run verifies

1. Rejects missing configuration, incompatible client tools, unexpected private tables, dirty/versionless schema sources and an unsafe restore destination.
2. Holds a read-only repeatable-read transaction and exports its snapshot. `pg_dump` creates a custom-format, data-only archive with INSERT statements and RLS enabled; the same snapshot supplies canonical content fingerprints. No source data is changed.
3. Applies the allowlisted Store migration to an empty isolated `nxt_restore` database. Only local/loopback or the workflow's `postgres` service hostname is accepted. Existing user tables or an existing `nxt_private` schema cause refusal; no database is dropped or cleaned.
4. Restores the archive with PostgreSQL 17, then compares row counts and SHA-256 canonical row streams for all three tables, schema/constraint fingerprints and the live RPC definition. Timestamps and list positions participate in the comparison; absent rows are not treated as an empty successful snapshot.
5. Checks the list sequence is not behind the restored positions and performs a synthetic Store SET/GET smoke inside a transaction that is rolled back. The source reader still has no RPC execute privilege.
6. Packages the dump and recovery metadata, encrypts with age, then removes temporary plaintext on normal completion/failure. The workflow retains only `*.tar.gz.age` and `*.manifest.json`. Tool errors that might contain connection details or SQL values are suppressed in logs.

The data snapshot is transaction-consistent. Sequence allocation is not transactional; a concurrently advancing allocator may be ahead of the captured rows. The restored sequence is checked to avoid reusing a captured position. PostgreSQL documents [snapshot synchronization](https://www.postgresql.org/docs/17/functions-admin.html#FUNCTIONS-SNAPSHOT-SYNCHRONIZATION) and [`pg_dump` snapshot/RLS/INSERT options](https://www.postgresql.org/docs/17/app-pgdump.html).

Plaintext exists temporarily in the isolated job container during export/recovery testing. File permissions and cleanup reduce exposure; they are not a claim of forensic secure erasure. A terminated job relies on disposal of its hosted runner/service. The disposable restore database contains the restored data until that service is destroyed. Do not run the script against a shared local database or an untrusted/self-hosted runner.

## Running without GitHub

Set `BACKUP_DATABASE_URL`, `BACKUP_AGE_RECIPIENT` and `BACKUP_RESTORE_DATABASE_URL` through private process configuration. Do not put passwords in shell history, command arguments, checked-in files or logs. The restore URL must name a fresh local `nxt_restore` database (or `nxt_restore_` followed by lowercase letters/digits). The script will not clear an existing schema.

For a remote source, also set `BACKUP_CA_FILE` to the verified public CA file. Optional variables are `BACKUP_SOURCE_DIR` (clean checked-out NXT source), `BACKUP_OUTPUT_DIR` (encrypted artifacts only) and `BACKUP_PG_BIN` (directory containing the PostgreSQL 17 tools). Invoke:

```sh
node scripts/backup-nxt-store.mjs
```

The output directory receives the encrypted archive and an outer manifest with its SHA-256, byte size, creation time, source commit and validation boundary. Table counts, source content fingerprints and RPC/schema details stay inside the encrypted archive. A successful script exit verifies the pre-encryption restore, not local decryption or an off-site upload. The outer manifest is a checksum, not a signature or immutable audit record.

## First-run acceptance and recovery drill

Do not mark setup complete until the authorized operator has recorded:

- The private repository/default-branch workflow and approved all-namespace read-only role, verified permissions, source revision and public recipient configuration. No source write or RPC privilege may be granted to the backup role.
- A successful manual Actions run with a retained encrypted artifact and its safe outer manifest. Confirm the artifact is in the private backup repository and has the intended 30-day retention.
- Download and SHA-256 comparison against the outer manifest, followed by **local decryption using the escrowed identity**. Decrypt into a private temporary directory; do not upload the plaintext or identity.
- Verification that the decrypted `nxt-store.dump` SHA-256 equals the encrypted manifest's `dump_sha256`, with the source commit and successful isolated restore checks recorded. Perform a PostgreSQL 17 recovery from that downloaded dump into a fresh disposable database, applying the bundled Store migration and comparing canonical fingerprints to `manifest.json`.
- Native application checks against an isolated recovered deployment/namespace, with record hashes, counts and module behavior compared at the selected boundary. Never overwrite production to test recovery. Namespace-level application recovery does not prove recovery of the whole Supabase project or missing deployment secrets.
- A confirmed operator response for failed/missed jobs, quota/egress growth and lost/rotated credentials, plus an off-device identity escrow copy. Re-run the drill after schema, backup driver, encryption or restoration changes.

For local decryption, official age syntax is:

```sh
age --decrypt --identity /private/path/backup-identity.txt --output /private/temp/nxt-store.tar.gz downloaded.tar.gz.age
tar -tzf /private/temp/nxt-store.tar.gz
```

Review the archive inventory before extracting. In a fresh isolated PostgreSQL 17 service, create NOLOGIN stubs for `anon`, `authenticated` and `service_role` if absent, apply `migrations/20261025051013_nxt_store.sql` with `psql -X -v ON_ERROR_STOP=1 -f`, and restore `nxt-store.dump` with `pg_restore --data-only --no-owner --no-privileges --exit-on-error --single-transaction`. Use local recovery credentials supplied through libpq environment configuration. Do not run recovery SQL or archive content against production. The canonical comparison queries are in the versioned backup driver; compare all three table hashes/counts, not merely a successful `pg_restore` exit.

## Evidence log

No hosted backup run, successful artifact upload or decrypted PostgreSQL archive is claimed by this implementation document. Append actual run/commit IDs, artifact SHA-256, decryption date, restore result and bounded application recovery evidence after they are verified. Never record database URLs/passwords, identities, taxpayer values or plaintext artifacts here.
