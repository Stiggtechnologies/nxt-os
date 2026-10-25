# Supabase persistence for NXT OS

This backend stores the existing NXT Store primitives in PostgreSQL. It creates no client/company records, imports no QuickBooks data, and does not integrate Supabase Auth into the application. The existing NXT API authorization remains the access gate. User identities, organization memberships, and individual approval authority need a separate Auth design before they can be claimed as implemented.

## Configuration and isolation

Set these **server environment variables** in the deployment hosting the Rust API:

| Variable | Value |
| --- | --- |
| `SUPABASE_URL` | The selected project's HTTPS API URL. |
| `SUPABASE_SECRET_KEY` | A backend secret key beginning `sb_secret_`. |
| `SUPABASE_SERVICE_ROLE_KEY` | Legacy alternative to the secret key; use one credential source. |
| `NXT_STORE_NAMESPACE` | Required deployment namespace, such as `production` or `preview:pr-1`. |

Namespaces must match `[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}`. Keep production and each preview in separate namespaces. The namespace comes from server configuration, never from a browser/request parameter. Namespace separation is deployment isolation under a powerful server credential; it is not per-user tenancy or an authorization boundary against a compromised service-role key.

Never put a secret/service-role key in `public/`, browser JavaScript, a client-prefixed environment variable, source control, example payloads, or logs. A publishable/anon key cannot call this RPC. Supabase's key guide distinguishes server secret keys from publishable keys and documents their elevated role. [Official key documentation](https://supabase.com/docs/guides/getting-started/api-keys).

When no Supabase setting is supplied, existing Redis or local-memory selection remains available. Partial or malformed Supabase configuration and Supabase request failures must fail closed; they must never silently create a fresh memory store. Local memory does not provide deployment durability. The production backend must have a valid namespace and durable configuration before operational data is entered.

## Database deployment

The migration is `supabase/migrations/20261025051013_nxt_store.sql`. It adds:

- Private `nxt_private.kv_values`, `set_members`, and `list_values` tables.
- Row Level Security on all three tables, with no ordinary-role policies and no direct table/schema grants to API roles.
- `public.nxt_store_command(text,jsonb)`, the only exposed operation, executable by `service_role` only.
- A fixed `pg_catalog, nxt_private` search path, qualified table references, and transaction-scoped advisory locking for single-key operations.

The function uses `SECURITY DEFINER` so its owner can access the private tables while callers have only function execution rights. Ordinary anonymous and authenticated roles cannot execute it. Supabase documents explicit function permissions and fixed search paths for this pattern. [Database-function security](https://supabase.com/docs/guides/database/functions), [RLS and grants](https://supabase.com/docs/guides/database/postgres/row-level-security).

After authenticating the Supabase CLI, link the intended project and inspect pending migrations before applying them:

```sh
supabase link --project-ref YOUR_PROJECT_REF
supabase migration list
supabase db push --dry-run
supabase db push
```

Run one migration deployment at a time and keep migration history in source control. Do not run `db reset` against a remote project or include seed data. The checked-in local configuration exposes only `public`, disables local seeding, and leaves local Auth disabled; it is not a claim that remote project settings were changed. [Official migration workflow](https://supabase.com/docs/guides/deployment/database-migrations).

The migration is transactional and can be re-applied without dropping store data. Future changes should use a new migration file after this one has been deployed. It does not migrate records from an existing Redis backend automatically: switching storage while operational data exists requires a separately reviewed transfer and reconciliation. Configure the backend before real records are added.

## Exact RPC contract

The Rust server calls `POST /rest/v1/rpc/nxt_store_command` with JSON:

```json
{
  "p_namespace": "production",
  "p_command": ["GET", "nxt:firm:state:v1"]
}
```

Every successful response is an object with a `result` field. Missing values are JSON `null`, not an omitted field. Errors are non-success PostgREST responses; callers must check HTTP status, envelope presence, and operation-specific result types before accepting them.

| Command array | `result` |
| --- | --- |
| `["GET", key]` | String or `null`. |
| `["SET", key, value]` | `"OK"`; replaces any previous key type. |
| `["SET", key, value, "NX"]` | `"OK"` when missing; `null` if any key type exists. |
| `["COMPARE_EXCHANGE", key, expected, value]` | Integer `1` when committed, otherwise `0`. `expected:null` means missing; `expected:""` means an existing empty string. |
| `["INCRBY", key, amount]` | Signed 64-bit integer; invalid/noninteger or overflowing values error without changing the key. |
| `["SADD", key, member]` | Integer `1` inserted or `0` already present. |
| `["SMEMBERS", key]` | Distinct string array, sorted for stable results. |
| `["LPUSH", key, value]` | New list length; newest entries first. |
| `["LPUSH_CAPPED", key, value, cap]` | Retained length; push and trim in one transaction. Cap `0` empties the list. |
| `["LTRIM", key, start, end]` | `"OK"`; inclusive Redis-style range. |
| `["LRANGE", key, start, end]` | String array in newest-first order; `0,-1` selects the whole list. |
| `["MGET", key1, key2, ...]` | Array preserving input order, repeated keys, and `null` for missing/non-string keys. |

Keys/values/members must be JSON strings. Integer arguments may be JSON integers or integer strings. No arbitrary SQL, `EVAL`, networking, or external-provider commands are accepted. Wrong-type operations fail, except Redis-style `SET` replacement and `MGET` returning `null` for non-string keys.

Bounds: namespace 1–64 ASCII characters; keys 1–512 bytes; string/list values at most 8 MiB; set members at most 1024 bytes; lists at most 10,000 items; `MGET` at most 500 keys; serialized command at most 40 MiB. These are storage primitives, not an API for browser callers. App-level report, operational-state, history, and HTTP response bounds continue to apply.

Single-key reads/writes take the same advisory transaction lock over namespace plus key, including missing keys. Compare-and-exchange therefore commits against the expected snapshot even across concurrent serverless instances. `MGET` is one read snapshot and takes no multi-key locks. Each RPC is one transaction; independent RPCs are not a transaction spanning multiple keys. Firm Operations already puts an approval and its internal effect in one CAS state document.

## Verification before enabling production

Apply the migration on a test/local project, then run the administrative SQL assertion script:

```sh
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f supabase/tests/store_contract.sql
```

It covers strings, missing/empty CAS, stale snapshots, namespaces, NX, ordered MGET, integer overflow, set deduplication, list order/caps/ranges, malformed commands, grants/RLS, and denied anon/authenticated calls. Its synthetic writes are inside a transaction that rolls back. It does not prove concurrent behavior by itself; run the application's concurrent approval test against the deployed backend as well.

Local preflight passed against PostgreSQL 18.6 through PGlite 0.5.11: migration creation, re-application preserving an existing sentinel, all SQL assertions, denied-role probes, a successful service-role RPC, and rollback cleanup. That embedded single-session check alone does not establish hosted connectivity or cross-session concurrency.

Hosted database verification then passed on project `qreiwxyknrswiiitbyqb` (`nxt-os`, Canada Central / `ca-central-1`), in organization **NXT OS** (`enpctrhzgqpyuedyrfrj`), with the organization confirmed on the **Free** plan. The database runs **PostgreSQL 17.11**. Its migration history contains version `20261025051013`, name `nxt_store`, matching the checked-in filename. The full rollback SQL contract completed without errors, including actual denied `anon`/`authenticated` calls. Follow-up queries found zero remaining synthetic KV, set, or list rows. All three private tables have RLS enabled; anonymous and authenticated roles lack private-schema usage and RPC execution; `service_role` has RPC execution. The function is `SECURITY DEFINER` with the fixed `pg_catalog, nxt_private` search path.

The SQL checks above used the authorized Management API database-query endpoint. Additional tests then exercised the actual Rust app and PostgREST with a modern server secret key:

- The local server reported existing workspace API authentication enabled and `storage: "supabase"`. This is the existing bearer-key gate, not Supabase Auth integration.
- A synthetic proposal and its idempotent replay, followed by two simultaneous approval requests, produced exactly one task.
- A balanced synthetic journal, with debit and credit totals both **12,345 cents**, persisted across a full local server process restart. The approval and generated task also remained available after restart.
- Sixteen simultaneous REST compare-and-exchange requests against a missing key produced exactly one winner in the targeted race test.
- A publishable-key call to the storage RPC was rejected with **HTTP 401**.

The verification server was stopped, the exact two synthetic test namespaces were removed, and total KV/set/list row counts of zero were confirmed at that cleanup checkpoint. A separate local preview now runs on port **3140**, connected to Supabase under namespace `local:firm-operations`; no clients or companies were created for it.

These checks establish live Rust/PostgREST connectivity, the tested approval/CAS race behavior, and persistence across local process restarts using the hosted database. They do not establish production Vercel environment configuration or behavior across deployed serverless instances. The Vercel connection remains pending sign-in. Preview/production namespace configuration and production smoke tests must be verified when that deployment is connected. QuickBooks, Excel write-back, and Supabase Auth remain unconnected.

A successful HTTP mock test or checked-in migration alone would not establish a live backend; the hosted checks above provide the additional evidence for the stated scope.

Keep database backups and recovery procedures appropriate to the project's selected plan. Secret rotation, production access controls, and migration application remain operational responsibilities. This integration adds persistence; it does not connect QuickBooks, Excel, or an identity provider.
